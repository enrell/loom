//! Archive extraction: `.zip` natively; `.7z`, `.rar`, `.tar.*` through 7-Zip
//! or bsdtar (`System32\tar.exe` reads 7z and RAR on Windows 10+).
//!
//! Extraction always targets an empty staging folder; entries escaping it
//! are refused and links an external tool created are removed afterwards.

use std::io;
use std::path::{Component, Path, PathBuf};
use std::process::Command;

use crate::error::{Context, Error, Result};

/// Extracts `archive` into the empty folder `dest`, or copies it when it is
/// a folder.
pub fn extract(archive: &Path, dest: &Path) -> Result<()> {
    if archive.is_dir() {
        crate::fsx::copy_tree(archive, dest).ctx(|| format!("copying {}", archive.display()))?;
    } else if ext(archive).as_deref() == Some("zip") {
        extract_zip(archive, dest)?;
    } else {
        extract_external(archive, dest)?;
    }
    scrub_links(dest).ctx(|| format!("checking {}", dest.display()))
}

fn ext(p: &Path) -> Option<String> {
    p.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase)
}

/// The archive's name without archive extensions (`Foo-1.2.tar.gz` → `Foo-1.2`).
pub fn stem(p: &Path) -> String {
    let mut name = p.file_name().unwrap_or_default().to_string_lossy().into_owned();
    if p.is_dir() {
        return name;
    }
    for _ in 0..2 {
        match name.rsplit_once('.') {
            Some((s, e))
                if !s.is_empty()
                    && matches!(
                        e.to_ascii_lowercase().as_str(),
                        "zip" | "7z" | "rar" | "tar" | "gz" | "tgz" | "bz2" | "xz" | "zst"
                    ) =>
            {
                name = s.to_owned();
            }
            _ => break,
        }
    }
    name
}

fn extract_zip(archive: &Path, dest: &Path) -> Result<()> {
    let file = std::fs::File::open(archive).ctx(|| format!("opening {}", archive.display()))?;
    let mut zip = zip::ZipArchive::new(io::BufReader::new(file))
        .map_err(|e| Error::invalid(format!("{} is not a readable zip: {e}", archive.display())))?;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| Error::invalid(format!("zip entry #{i}: {e}")))?;
        let Some(rel) = entry.enclosed_name().filter(|p| p.components().all(|c| matches!(c, Component::Normal(_))))
        else {
            return Err(Error::invalid(format!("zip entry {:?} escapes the mod folder", entry.name())));
        };
        let out = dest.join(rel);
        if entry.is_dir() {
            std::fs::create_dir_all(&out).ctx(|| format!("creating {}", out.display()))?;
        } else if entry.is_file() {
            if let Some(p) = out.parent() {
                std::fs::create_dir_all(p).ctx(|| format!("creating {}", p.display()))?;
            }
            let mut w = std::fs::File::create(&out).ctx(|| format!("creating {}", out.display()))?;
            io::copy(&mut entry, &mut w).ctx(|| format!("extracting zip entry #{i}"))?;
        }
    }
    Ok(())
}

fn extractors(archive: &Path, dest: &Path) -> Vec<(PathBuf, Vec<std::ffi::OsString>)> {
    let seven = |exe: &str| {
        let mut out = std::ffi::OsString::from("-o");
        out.push(dest);
        (PathBuf::from(exe), vec!["x".into(), "-y".into(), "-bd".into(), out, archive.into()])
    };
    let tar = |exe: PathBuf| (exe, vec!["-x".into(), "-f".into(), archive.into(), "-C".into(), dest.into()]);
    let mut list = vec![seven("7z"), seven("7za"), seven("7zz")];
    if cfg!(windows) {
        list.push(seven(r"C:\Program Files\7-Zip\7z.exe"));
        let root = std::env::var_os("SystemRoot").unwrap_or_else(|| r"C:\Windows".into());
        list.push(tar(Path::new(&root).join("System32").join("tar.exe")));
    }
    list.push(tar(PathBuf::from("bsdtar")));
    list
}

fn extract_external(archive: &Path, dest: &Path) -> Result<()> {
    let archive = std::path::absolute(archive).ctx(|| format!("resolving {}", archive.display()))?;
    let mut tried = Vec::new();
    for (exe, args) in extractors(&archive, dest) {
        match Command::new(&exe).args(&args).output() {
            Err(_) => tried.push(exe.display().to_string()),
            Ok(out) if out.status.success() => return Ok(()),
            Ok(out) => {
                return Err(Error::invalid(format!(
                    "{} could not extract {}: {}",
                    exe.display(),
                    archive.display(),
                    String::from_utf8_lossy(&out.stderr).trim()
                )));
            }
        }
    }
    Err(Error::invalid(format!(
        "no program to extract {} — install 7-Zip (tried {})",
        archive.display(),
        tried.join(", ")
    )))
}

/// Removes symlinks/junctions: a mod is plain files.
fn scrub_links(dir: &Path) -> io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let meta = std::fs::symlink_metadata(&path)?;
        if is_link(&meta) {
            if meta.is_dir() { std::fs::remove_dir(&path)? } else { std::fs::remove_file(&path)? }
        } else if meta.is_dir() {
            scrub_links(&path)?;
        }
    }
    Ok(())
}

#[cfg(windows)]
fn is_link(meta: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    meta.file_type().is_symlink() || meta.file_attributes() & 0x400 != 0 // REPARSE_POINT
}

#[cfg(not(windows))]
fn is_link(meta: &std::fs::Metadata) -> bool {
    meta.file_type().is_symlink()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stems() {
        assert_eq!(stem(Path::new("SkyUI_5_2_SE-12604-5-2SE.7z")), "SkyUI_5_2_SE-12604-5-2SE");
        assert_eq!(stem(Path::new("a.tar.gz")), "a");
        assert_eq!(stem(Path::new("v1.2.zip")), "v1.2");
    }
}
