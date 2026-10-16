//! Where Loom keeps things, and expansion of the path templates plugins use.

use std::path::{Path, PathBuf};

use crate::error::{Error, Result};

/// The library root: `LOOM_HOME`, or `%LOCALAPPDATA%\Loom`.
pub fn default_home() -> PathBuf {
    if let Some(h) = std::env::var_os("LOOM_HOME") {
        return PathBuf::from(h);
    }
    dirs::data_local_dir().unwrap_or_else(|| PathBuf::from(".")).join("Loom")
}

/// Expands a plugin path template (`%LOCALAPPDATA%/Skyrim Special Edition/Plugins.txt`,
/// `{game}/Data`). `game` is the game root, when known.
pub fn expand(template: &str, game: Option<&Path>) -> Result<PathBuf> {
    let (var, rest) = match template.find(['/', '\\']) {
        Some(i) => (&template[..i], &template[i + 1..]),
        None => (template, ""),
    };
    let base = match var {
        "%LOCALAPPDATA%" => dirs::data_local_dir(),
        "%APPDATA%" => dirs::data_dir(),
        "%USERPROFILE%" => dirs::home_dir(),
        "%DOCUMENTS%" => dirs::document_dir(),
        "%MYGAMES%" => dirs::document_dir().map(|d| d.join("My Games")),
        "{game}" => game.map(Path::to_path_buf),
        _ => {
            return Err(Error::invalid(format!("unknown path template {var:?} in {template:?}")));
        }
    }
    .ok_or_else(|| Error::invalid(format!("cannot resolve {var} on this system")))?;
    Ok(join_rel(&base, rest))
}

/// Joins a `/`-separated relative path below `base`.
pub fn join_rel(base: &Path, rel: &str) -> PathBuf {
    rel.split(['/', '\\']).filter(|c| !c.is_empty()).fold(base.to_path_buf(), |p, c| p.join(c))
}

/// Case-insensitive lookup of a `/`-separated relative path below `dir`.
pub fn find_ci_path(dir: &Path, rel: &str) -> Option<PathBuf> {
    let mut cur = dir.to_path_buf();
    for part in rel.split(['/', '\\']).filter(|p| !p.is_empty()) {
        cur = std::fs::read_dir(&cur)
            .ok()?
            .flatten()
            .find(|e| e.file_name().to_string_lossy().eq_ignore_ascii_case(part))?
            .path();
    }
    Some(cur)
}

/// Lowercased, `/`-separated key for case-insensitive path comparison.
pub fn fold(rel: &str) -> String {
    rel.replace('\\', "/").to_lowercase()
}

/// A name that is safe as a single folder name on Windows.
pub fn is_safe_name(name: &str) -> bool {
    const RESERVED: &[&str] = &[
        "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8", "com9", "lpt1",
        "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
    ];
    let stem = name.split('.').next().unwrap_or("").to_ascii_lowercase();
    !name.trim().is_empty()
        && name.len() <= 200
        && !name.starts_with('.')
        && !name.ends_with(['.', ' '])
        && !name.contains(['/', '\\', ':', '*', '?', '"', '<', '>', '|'])
        && !name.chars().any(char::is_control)
        && !RESERVED.contains(&stem.as_str())
}

/// Case-insensitive glob match supporting `*` and `?`.
pub fn glob_match(pattern: &str, name: &str) -> bool {
    fn go(p: &[char], n: &[char]) -> bool {
        match (p.first(), n.first()) {
            (None, None) => true,
            (Some('*'), _) => go(&p[1..], n) || (!n.is_empty() && go(p, &n[1..])),
            (Some('?'), Some(_)) => go(&p[1..], &n[1..]),
            (Some(a), Some(b)) => a == b && go(&p[1..], &n[1..]),
            _ => false,
        }
    }
    let p: Vec<char> = pattern.to_lowercase().chars().collect();
    let n: Vec<char> = name.to_lowercase().chars().collect();
    go(&p, &n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn templates_expand() {
        let g = Path::new("C:/Games/X");
        assert_eq!(expand("{game}/Data/Plugins.txt", Some(g)).unwrap(), g.join("Data").join("Plugins.txt"));
        assert!(expand("%LOCALAPPDATA%/Skyrim/Plugins.txt", None).unwrap().ends_with("Skyrim/Plugins.txt"));
        assert!(expand("C:/x", None).is_err());
    }

    #[test]
    fn globs() {
        assert!(glob_match("*.esp", "Foo.ESP"));
        assert!(glob_match("skse64_*.dll", "skse64_1_6_1170.dll"));
        assert!(!glob_match("*.esp", "foo.esm"));
        assert!(glob_match("meshes", "Meshes"));
    }

    #[test]
    fn names() {
        assert!(is_safe_name("SkyUI 5.2"));
        assert!(!is_safe_name("con"));
        assert!(!is_safe_name("a/b"));
        assert!(!is_safe_name(".hidden"));
    }
}
