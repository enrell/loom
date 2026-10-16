//! End to end through the `loom` binary: plugin → game → mod → load order →
//! doctor → launch (mount mode, when WinFsp is installed).
//!
//! The "game" is a folder with `cmd.exe` copied in as `game.exe`, so launching
//! it with `/c copy …` proves the program really sees the merged view and
//! that its writes land in the overwrite folder.

#![cfg(windows)]

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

const PLUGIN: &str = r#"
[plugin]
id = "github.com/example/loom-testgame"
name = "Test Game"
version = "0.1.0"
kind = "game"

[game]
id = "testgame"
name = "Test Game"
executables = [{ id = "game", name = "Game", path = "game.exe", default = true }]

[game.detect]
folder_names = ["Loom Test Game"]
requires = ["Data"]

[game.mods]
data_dir = "Data"
data_markers = ["*.esp", "textures"]

[game.load_order]
format = "plugins-txt"
plugins_file = "{game}/Plugins.txt"
extensions = ["esm", "esp"]
star_prefix = true
implicit = ["Base.esm"]
header = "tes4"
"#;

struct World {
    _tmp: tempfile::TempDir,
    home: PathBuf,
    game: PathBuf,
    root: PathBuf,
}

fn loom(w: &World, args: &[&str]) -> (bool, Value) {
    let out =
        Command::new(env!("CARGO_BIN_EXE_loom")).arg("--json").arg("--home").arg(&w.home).args(args).output().unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    let v = serde_json::from_str(&text)
        .unwrap_or_else(|_| panic!("not json: {text}\nstderr: {}", String::from_utf8_lossy(&out.stderr)));
    (out.status.success(), v)
}

fn world() -> World {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    let plugin = root.join("plugin");
    std::fs::create_dir_all(&plugin).unwrap();
    std::fs::write(plugin.join("loom-plugin.toml"), PLUGIN).unwrap();
    let game = root.join("Loom Test Game");
    std::fs::create_dir_all(game.join("Data").join("textures")).unwrap();
    let cmd = Path::new(&std::env::var("SystemRoot").unwrap()).join("System32").join("cmd.exe");
    std::fs::copy(cmd, game.join("game.exe")).unwrap();
    std::fs::write(game.join("Data").join("Base.esm"), loom_core::loadorder::fake_tes4(&[], 1)).unwrap();
    std::fs::write(game.join("Data").join("textures").join("a.txt"), "from game").unwrap();
    let w = World { home: root.join("home"), game, root, _tmp: tmp };
    let (ok, v) = loom(&w, &["plugin", "add", plugin.to_str().unwrap()]);
    assert!(ok, "{v}");
    w
}

fn make_zip(path: &Path, files: &[(&str, &[u8])]) {
    let mut z = zip::ZipWriter::new(std::fs::File::create(path).unwrap());
    for (name, bytes) in files {
        z.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
        z.write_all(bytes).unwrap();
    }
    z.finish().unwrap();
}

#[test]
fn full_flow() {
    let w = world();
    let game = w.game.to_str().unwrap();

    let (ok, v) = loom(&w, &["game", "add", "testgame", "--path", game]);
    assert!(ok, "{v}");

    // A mod in a wrapper folder, lowercase "data"-level content, a plugin
    // with a missing master.
    let esp = loom_core::loadorder::fake_tes4(&["Base.esm", "Missing.esm"], 0);
    let archive = w.root.join("Cool Mod-1.0.zip");
    make_zip(&archive, &[("Cool Mod/cool.esp", &esp), ("Cool Mod/TEXTURES/a.txt", b"from mod")]);
    let (ok, v) = loom(&w, &["mod", "install", "testgame", archive.to_str().unwrap(), "--name", "Cool Mod"]);
    assert!(ok, "{v}");
    assert_eq!(v["files"], 2);
    let mod_dir = w.home.join("games/testgame/mods/Cool Mod");
    assert!(mod_dir.join("Data/textures/a.txt").is_file(), "case follows the game's folders");
    assert!(mod_dir.join("Data/cool.esp").is_file());

    let (_, v) = loom(&w, &["mod", "list", "testgame"]);
    assert_eq!(v[0]["name"], "Cool Mod");
    assert_eq!(v[0]["enabled"], true);

    let (_, v) = loom(&w, &["order", "list", "testgame"]);
    let names: Vec<&str> = v.as_array().unwrap().iter().map(|p| p["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["Base.esm", "cool.esp"]);
    assert_eq!(v[1]["masters"][1], "Missing.esm");

    // doctor finds the missing master and fails...
    let (ok, v) = loom(&w, &["doctor", "testgame"]);
    assert!(!ok);
    let codes: Vec<&str> = v.as_array().unwrap().iter().map(|p| p["code"].as_str().unwrap()).collect();
    assert!(codes.contains(&"missing-master"), "{v}");
    // ...and following its fix makes it pass.
    let (ok, _) = loom(&w, &["order", "disable", "testgame", "cool.esp"]);
    assert!(ok);
    let (_, v) = loom(&w, &["doctor", "testgame"]);
    assert!(v.as_array().unwrap().iter().all(|p| p["code"] != "missing-master"), "{v}");

    // A second profile starts with the mod disabled.
    let (ok, _) = loom(&w, &["profile", "new", "testgame", "clean"]);
    assert!(ok);
    let (_, v) = loom(&w, &["profile", "list", "testgame"]);
    assert_eq!(v["profiles"], serde_json::json!(["clean", "default"]));

    // Removing the plugin keeps the game data; re-adding brings it back.
    let (ok, _) = loom(&w, &["plugin", "remove", "github.com/example/loom-testgame"]);
    assert!(ok);
    let (ok, v) = loom(&w, &["mod", "list", "testgame"]);
    assert!(!ok && v["error"].as_str().unwrap().contains("no installed plugin"), "{v}");
    let (ok, _) = loom(&w, &["plugin", "add", w.root.join("plugin").to_str().unwrap()]);
    assert!(ok);
    let (ok, v) = loom(&w, &["mod", "list", "testgame"]);
    assert!(ok && v[0]["name"] == "Cool Mod", "{v}");

    if !loom_core::launch::mount_available() {
        eprintln!("skipping launch: WinFsp is not installed");
        return;
    }
    // The game (cmd.exe) copies a file it can only see through the merged
    // view; the copy must land in the overwrite folder, not the game folder.
    let (ok, v) =
        loom(&w, &["launch", "testgame", "--mode", "mount", "--", "/c", r"copy Data\textures\a.txt Data\copied.txt"]);
    assert!(ok, "{v}");
    assert_eq!(v["exit_code"], 0, "{v}");
    let copied = w.home.join("games/testgame/profiles/default/overwrite/Data/copied.txt");
    assert_eq!(std::fs::read_to_string(copied).unwrap(), "from mod", "the mod's file wins over the game's");
    assert!(!w.game.join("Data/copied.txt").exists(), "the game folder is never written");
    assert!(w.game.join("Plugins.txt").is_file(), "load order deployed on launch");

    let (ok, v) = loom(&w, &["overwrite", "to-mod", "testgame", "Saved Output"]);
    assert!(ok && v["moved"] == 1, "{v}");
}
