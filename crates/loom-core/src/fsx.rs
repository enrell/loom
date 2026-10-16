//! Small filesystem helpers.

use std::io::{self, Write};
use std::path::Path;

/// Writes via a temporary sibling + rename, so readers never see half a file.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut tmp_name = path.file_name().unwrap_or_default().to_os_string();
    tmp_name.push(".tmp");
    let tmp = path.with_file_name(tmp_name);
    {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, path)
}

/// Recursively copies `src` into `dst` (regular files and directories only).
pub fn copy_tree(src: &Path, dst: &Path) -> io::Result<u64> {
    std::fs::create_dir_all(dst)?;
    let mut n = 0;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ft = entry.file_type()?;
        let to = dst.join(entry.file_name());
        if ft.is_dir() {
            n += copy_tree(&entry.path(), &to)?;
        } else if ft.is_file() {
            std::fs::copy(entry.path(), &to)?;
            n += 1;
        }
    }
    Ok(n)
}

/// Every regular file below `dir`, as `/`-separated paths relative to it, sorted.
pub fn walk_files(dir: &Path) -> io::Result<Vec<String>> {
    fn go(base: &Path, dir: &Path, out: &mut Vec<String>) -> io::Result<()> {
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let ft = entry.file_type()?;
            if ft.is_dir() {
                go(base, &entry.path(), out)?;
            } else if ft.is_file() {
                let rel = entry.path().strip_prefix(base).expect("below base").to_string_lossy().replace('\\', "/");
                out.push(rel);
            }
        }
        Ok(())
    }
    let mut out = Vec::new();
    if dir.is_dir() {
        go(dir, dir, &mut out)?;
    }
    out.sort();
    Ok(out)
}

/// Total size in bytes of the files below `dir`.
pub fn dir_size(dir: &Path) -> u64 {
    walk_files(dir)
        .unwrap_or_default()
        .iter()
        .filter_map(|f| std::fs::metadata(dir.join(f)).ok())
        .map(|m| m.len())
        .sum()
}
