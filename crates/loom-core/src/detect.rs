//! Finding installed games. Generic store support (Steam libraries, GOG,
//! registry values, well-known folders) — plugins only supply the ids.

use std::path::{Path, PathBuf};

use loom_plugin::GameSpec;
use serde::Serialize;

/// A folder that looks like the game.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Candidate {
    pub path: PathBuf,
    /// `steam`, `gog`, `registry`, `folder`.
    pub source: &'static str,
}

/// Every installation of `game` found on this machine, best first, deduplicated.
pub fn find(game: &GameSpec) -> Vec<Candidate> {
    let mut out: Vec<Candidate> = Vec::new();
    let mut push = |path: PathBuf, source: &'static str| {
        if is_install(game, &path) && !out.iter().any(|c| same_path(&c.path, &path)) {
            out.push(Candidate { path, source });
        }
    };
    let libraries = steam_libraries();
    for id in &game.detect.steam {
        for lib in &libraries {
            if let Some(dir) = steam_app_dir(lib, *id) {
                push(dir, "steam");
            }
        }
    }
    for id in &game.detect.gog {
        if let Some(p) = registry_value(&format!(r"HKLM\SOFTWARE\WOW6432Node\GOG.com\Games\{id}"), "path") {
            push(PathBuf::from(p), "gog");
        }
    }
    for hint in &game.detect.registry {
        if let Some(p) = registry_value(&hint.key, &hint.value) {
            push(PathBuf::from(p.trim_end_matches(['\\', '/'])), "registry");
        }
    }
    for name in &game.detect.folder_names {
        for lib in &libraries {
            push(lib.join("steamapps").join("common").join(name), "folder");
        }
        for base in [
            r"C:\GOG Games",
            r"C:\Program Files (x86)\GOG Galaxy\Games",
            r"C:\Program Files\GOG Galaxy\Games",
            r"C:\Games",
        ] {
            push(Path::new(base).join(name), "folder");
        }
    }
    out
}

/// `true` if `dir` has every file the plugin requires (and at least one executable).
pub fn is_install(game: &GameSpec, dir: &Path) -> bool {
    dir.is_dir()
        && game.detect.requires.iter().all(|f| crate::paths::join_rel(dir, f).exists())
        && game.executables.iter().any(|e| e.optional || crate::paths::join_rel(dir, &e.path).is_file())
}

fn same_path(a: &Path, b: &Path) -> bool {
    let n = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf()).to_string_lossy().to_lowercase();
    n(a) == n(b)
}

// ---------------------------------------------------------------- Steam

/// Steam's install folder.
pub fn steam_root() -> Option<PathBuf> {
    registry_value(r"HKCU\Software\Valve\Steam", "SteamPath")
        .or_else(|| registry_value(r"HKLM\SOFTWARE\WOW6432Node\Valve\Steam", "InstallPath"))
        .map(PathBuf::from)
        .or_else(|| Some(PathBuf::from(r"C:\Program Files (x86)\Steam")))
        .filter(|p| p.is_dir())
}

/// Every Steam library folder (the ones holding `steamapps/`).
pub fn steam_libraries() -> Vec<PathBuf> {
    let Some(root) = steam_root() else {
        return Vec::new();
    };
    let mut libs = vec![root.clone()];
    if let Ok(text) = std::fs::read_to_string(root.join("steamapps").join("libraryfolders.vdf")) {
        for (key, value) in vdf_pairs(&text) {
            if key.eq_ignore_ascii_case("path") {
                let p = PathBuf::from(value.replace("\\\\", "\\"));
                if p.is_dir() && !libs.iter().any(|l| same_path(l, &p)) {
                    libs.push(p);
                }
            }
        }
    }
    libs
}

fn steam_app_dir(library: &Path, app_id: u32) -> Option<PathBuf> {
    let acf = std::fs::read_to_string(library.join("steamapps").join(format!("appmanifest_{app_id}.acf"))).ok()?;
    let dir = vdf_pairs(&acf).into_iter().find(|(k, _)| k.eq_ignore_ascii_case("installdir"))?.1;
    Some(library.join("steamapps").join("common").join(dir))
}

/// Flat `"key" "value"` pairs of a Valve KeyValues (VDF) text; nesting is
/// ignored, which is all game detection needs.
pub fn vdf_pairs(text: &str) -> Vec<(String, String)> {
    let mut tokens = Vec::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                let mut s = String::new();
                while let Some(c) = chars.next() {
                    match c {
                        '\\' => {
                            if let Some(n) = chars.next() {
                                s.push('\\');
                                s.push(n);
                            }
                        }
                        '"' => break,
                        _ => s.push(c),
                    }
                }
                tokens.push(Some(s));
            }
            '{' | '}' => tokens.push(None),
            '/' if chars.peek() == Some(&'/') => {
                for c in chars.by_ref() {
                    if c == '\n' {
                        break;
                    }
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    let mut i = 0;
    while i + 1 < tokens.len() {
        if let (Some(k), Some(v)) = (&tokens[i], &tokens[i + 1]) {
            out.push((k.clone(), v.clone()));
            i += 2;
        } else {
            i += 1;
        }
    }
    out
}

// ---------------------------------------------------------------- registry

/// Reads `HKLM\…` / `HKCU\…` string values (both registry views).
#[cfg(windows)]
pub fn registry_value(key: &str, value: &str) -> Option<String> {
    use winreg::RegKey;
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_32KEY, KEY_WOW64_64KEY};
    let (hive, sub) = key.split_once('\\')?;
    let root = match hive.to_ascii_uppercase().as_str() {
        "HKLM" | "HKEY_LOCAL_MACHINE" => RegKey::predef(HKEY_LOCAL_MACHINE),
        "HKCU" | "HKEY_CURRENT_USER" => RegKey::predef(HKEY_CURRENT_USER),
        _ => return None,
    };
    [KEY_WOW64_64KEY, KEY_WOW64_32KEY]
        .into_iter()
        .find_map(|view| root.open_subkey_with_flags(sub, KEY_READ | view).ok()?.get_value::<String, _>(value).ok())
}

#[cfg(not(windows))]
pub fn registry_value(_key: &str, _value: &str) -> Option<String> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vdf() {
        let text = r#"
"libraryfolders"
{
	"0"
	{
		"path"		"C:\\Program Files (x86)\\Steam"
		"apps" { "489830" "123" }
	}
	"1" { "path" "D:\\SteamLibrary" } // comment
}"#;
        let pairs = vdf_pairs(text);
        let paths: Vec<&str> = pairs.iter().filter(|(k, _)| k == "path").map(|(_, v)| v.as_str()).collect();
        assert_eq!(paths, [r"C:\\Program Files (x86)\\Steam", r"D:\\SteamLibrary"]);
        assert!(pairs.iter().any(|(k, v)| k == "489830" && v == "123"));
    }
}
