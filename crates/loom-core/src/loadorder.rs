//! The `plugins-txt` load-order capability (Bethesda-style `Plugins.txt`).
//!
//! The order lives in the profile (`profiles/<p>/plugins.txt`, one name per
//! line, `*` marking active entries); [`Game::deploy_load_order`] writes it to
//! wherever the game reads it. Plugin headers (`TES4`/`TES3`) are read for
//! masters and flags, which `sort` and `doctor` use.

use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::path::{Path, PathBuf};

use loom_plugin::{HeaderFormat, LoadOrder};
use serde::Serialize;

use crate::error::{Context, Error, Result};
use crate::fsx;
use crate::library::Game;
use crate::paths::{expand, find_ci_path};

pub const PROFILE_PLUGINS: &str = "plugins.txt";

/// One plugin file in the load order.
#[derive(Debug, Clone, Serialize)]
#[allow(clippy::struct_excessive_bools)] // a plain record of the plugin's flags
pub struct PluginEntry {
    pub name: String,
    pub active: bool,
    /// Always loaded by the game; can't be disabled or moved.
    pub implicit: bool,
    /// `base` (the game) or the mod providing it.
    pub provider: String,
    pub masters: Vec<String>,
    /// ESM flag or `.esm`.
    pub is_master: bool,
    /// ESL flag or light extension: doesn't count toward the plugin limit.
    pub is_light: bool,
}

/// Plugin header facts.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Header {
    pub masters: Vec<String>,
    pub master_flag: bool,
    pub light_flag: bool,
}

impl Game {
    fn load_order_spec(&self) -> Result<&LoadOrder> {
        self.spec()
            .load_order
            .as_ref()
            .ok_or_else(|| Error::invalid(format!("{} has no plugin load order", self.spec().name)))
    }

    /// Whether this game has a plugin load order at all.
    pub fn has_load_order(&self) -> bool {
        self.spec().load_order.is_some()
    }

    /// Every plugin file the merged view provides: `name → (provider, path)`.
    /// Higher-priority providers win, like the virtual file system.
    fn available_plugins(&self, spec: &LoadOrder) -> Result<Vec<(String, String, PathBuf)>> {
        let data = self.spec().mods.data_dir.trim_matches('/').to_owned();
        let mut seen: HashSet<String> = HashSet::new();
        let mut out = Vec::new();
        let mut scan = |dir: Option<PathBuf>, provider: &str, out: &mut Vec<(String, String, PathBuf)>| {
            let Some(dir) = dir else { return };
            let Ok(rd) = std::fs::read_dir(&dir) else {
                return;
            };
            for e in rd.flatten() {
                let name = e.file_name().to_string_lossy().into_owned();
                let ext = name.rsplit_once('.').map(|(_, x)| x.to_ascii_lowercase()).unwrap_or_default();
                if e.path().is_file()
                    && spec.extensions.iter().any(|x| x.eq_ignore_ascii_case(&ext))
                    && seen.insert(name.to_lowercase())
                {
                    out.push((name, provider.to_owned(), e.path()));
                }
            }
        };
        let rel_data = |base: &Path| {
            if data.is_empty() { Some(base.to_path_buf()) } else { find_ci_path(base, &data) }
        };
        scan(rel_data(&self.overwrite_dir()), "overwrite", &mut out);
        for m in self.profile()?.enabled() {
            scan(rel_data(&self.mods_dir().join(m)), m, &mut out);
        }
        scan(rel_data(self.root()), "base", &mut out);
        Ok(out)
    }

    /// The current load order, reconciled with what's installed: plugins that
    /// disappeared are dropped, new ones are appended (active).
    pub fn load_order(&self) -> Result<Vec<PluginEntry>> {
        let spec = self.load_order_spec()?;
        let available = self.available_plugins(spec)?;
        let by_name: HashMap<String, &(String, String, PathBuf)> =
            available.iter().map(|a| (a.0.to_lowercase(), a)).collect();
        let saved = read_list(&self.profile_dir(self.active_profile()).join(PROFILE_PLUGINS));
        let mut order: Vec<(String, bool)> = Vec::new();
        let mut placed: HashSet<String> = HashSet::new();
        let implicit_list = implicit_plugins(spec, self.root());
        for imp in &implicit_list {
            if let Some(a) = by_name.get(&imp.to_lowercase())
                && placed.insert(imp.to_lowercase())
            {
                order.push((a.0.clone(), true));
            }
        }
        for (name, active) in saved {
            if let Some(a) = by_name.get(&name.to_lowercase())
                && placed.insert(name.to_lowercase())
            {
                order.push((a.0.clone(), active));
            }
        }
        // New plugins: base game files first (in folder order), then mods.
        let mut fresh: Vec<&(String, String, PathBuf)> =
            available.iter().filter(|a| !placed.contains(&a.0.to_lowercase())).collect();
        fresh.sort_by_key(|a| (a.1 != "base", !is_master_ext(&a.0)));
        for a in fresh {
            placed.insert(a.0.to_lowercase());
            order.push((a.0.clone(), true));
        }
        let implicit: HashSet<String> = implicit_list.iter().map(|s| s.to_lowercase()).collect();
        Ok(order
            .into_iter()
            .map(|(name, active)| {
                let a = by_name[&name.to_lowercase()];
                let header = read_header(&a.2, spec.header).unwrap_or_default();
                let ext = name.rsplit_once('.').map(|(_, x)| x.to_ascii_lowercase()).unwrap_or_default();
                let light_ext = spec.light_extension.as_deref().is_some_and(|l| l.eq_ignore_ascii_case(&ext));
                let is_implicit = implicit.contains(&name.to_lowercase());
                PluginEntry {
                    active: active || is_implicit,
                    implicit: is_implicit,
                    provider: a.1.clone(),
                    is_master: header.master_flag || ext == "esm" || light_ext,
                    is_light: header.light_flag || light_ext,
                    masters: header.masters,
                    name,
                }
            })
            .collect())
    }

    fn save_load_order(&self, order: &[PluginEntry]) -> Result<()> {
        let text: String = order
            .iter()
            .filter(|p| !p.implicit)
            .map(|p| if p.active { format!("*{}\n", p.name) } else { format!("{}\n", p.name) })
            .collect();
        let path = self.profile_dir(self.active_profile()).join(PROFILE_PLUGINS);
        fsx::write_atomic(&path, text.as_bytes()).ctx(|| format!("writing {}", path.display()))
    }

    pub fn set_plugin_active(&self, name: &str, active: bool) -> Result<()> {
        let mut order = self.load_order()?;
        let p = order
            .iter_mut()
            .find(|p| p.name.eq_ignore_ascii_case(name))
            .ok_or_else(|| Error::invalid(format!("no plugin file named {name:?}")))?;
        if p.implicit && !active {
            return Err(Error::invalid(format!("{} is always loaded by the game", p.name)));
        }
        p.active = active;
        self.save_load_order(&order)
    }

    /// Moves a plugin to `index` (0 = loads first), after the implicit ones.
    pub fn move_plugin(&self, name: &str, index: usize) -> Result<()> {
        let mut order = self.load_order()?;
        let from = order
            .iter()
            .position(|p| p.name.eq_ignore_ascii_case(name))
            .ok_or_else(|| Error::invalid(format!("no plugin file named {name:?}")))?;
        if order[from].implicit {
            return Err(Error::invalid(format!("{} always loads first", order[from].name)));
        }
        let floor = order.iter().filter(|p| p.implicit).count();
        let entry = order.remove(from);
        order.insert(index.clamp(floor, order.len()), entry);
        self.save_load_order(&order)
    }

    /// Sorts: implicit plugins first, masters before non-masters, and every
    /// plugin after its masters — otherwise keeping the current order.
    pub fn sort_load_order(&self) -> Result<Vec<PluginEntry>> {
        let order = self.load_order()?;
        let sorted = sort_by_masters(order);
        self.save_load_order(&sorted)?;
        Ok(sorted)
    }

    /// Writes the active profile's order where the game reads it. The file
    /// that was there before Loom first touched it is kept in
    /// `games/<id>/backup/` (it lives outside the game folder, in the user's
    /// AppData).
    pub fn deploy_load_order(&self) -> Result<Vec<PathBuf>> {
        let Some(spec) = self.spec().load_order.as_ref() else {
            return Ok(Vec::new());
        };
        let order = self.load_order()?;
        let mut written = Vec::new();
        let plugins_file = expand(&spec.plugins_file, Some(self.root()))?;
        let lines: String = order
            .iter()
            .filter(|p| !p.implicit)
            .filter_map(|p| match (spec.star_prefix, p.active) {
                (true, true) => Some(format!("*{}\r\n", p.name)),
                (true, false) | (false, true) => Some(format!("{}\r\n", p.name)),
                (false, false) => None,
            })
            .collect();
        self.backup_once(&plugins_file)?;
        fsx::write_atomic(&plugins_file, lines.as_bytes()).ctx(|| format!("writing {}", plugins_file.display()))?;
        written.push(plugins_file);
        if spec.timestamps {
            self.apply_timestamps(&order)?;
        }
        if let Some(lo) = &spec.loadorder_file {
            let path = expand(lo, Some(self.root()))?;
            let all = order.iter().fold(String::new(), |mut acc, p| {
                acc.push_str(&p.name);
                acc.push_str("\r\n");
                acc
            });
            self.backup_once(&path)?;
            fsx::write_atomic(&path, all.as_bytes()).ctx(|| format!("writing {}", path.display()))?;
            written.push(path);
        }
        Ok(written)
    }

    /// Timestamp-ordered games: gives the plugin files Loom owns (in mod
    /// folders) increasing modification times in load order, starting after
    /// the newest base-game plugin.
    fn apply_timestamps(&self, order: &[PluginEntry]) -> Result<()> {
        let data = self.spec().mods.data_dir.trim_matches('/').to_owned();
        let in_dir = |base: &Path, name: &str| {
            let dir = if data.is_empty() { Some(base.to_path_buf()) } else { find_ci_path(base, &data) };
            dir.and_then(|d| find_ci_path(&d, name))
        };
        let newest_base = order
            .iter()
            .filter(|p| p.provider == "base")
            .filter_map(|p| in_dir(self.root(), &p.name))
            .filter_map(|f| std::fs::metadata(f).and_then(|m| m.modified()).ok())
            .max()
            .unwrap_or(std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000_000));
        let mut t = newest_base;
        for p in order.iter().filter(|p| p.provider != "base") {
            t += std::time::Duration::from_secs(60);
            let base = if p.provider == "overwrite" { self.overwrite_dir() } else { self.mods_dir().join(&p.provider) };
            if let Some(file) = in_dir(&base, &p.name) {
                let f = std::fs::OpenOptions::new()
                    .write(true)
                    .open(&file)
                    .ctx(|| format!("opening {}", file.display()))?;
                f.set_modified(t).ctx(|| format!("setting the time of {}", file.display()))?;
            }
        }
        Ok(())
    }

    fn backup_once(&self, file: &Path) -> Result<()> {
        let backup = self.library_dir().join("backup").join(file.file_name().unwrap_or_default());
        if file.exists() && !backup.exists() {
            std::fs::create_dir_all(backup.parent().unwrap()).ctx(|| "creating the backup folder".into())?;
            std::fs::copy(file, &backup).ctx(|| format!("backing up {}", file.display()))?;
        }
        Ok(())
    }
}

/// `implicit`, then the entries of `implicit_file` (if it exists).
fn implicit_plugins(spec: &LoadOrder, root: &Path) -> Vec<String> {
    let mut out = spec.implicit.clone();
    if let Some(f) = &spec.implicit_file
        && let Ok(path) = expand(f, Some(root))
    {
        out.extend(read_list(&path).into_iter().map(|(n, _)| n));
    }
    out
}

fn is_master_ext(name: &str) -> bool {
    name.to_ascii_lowercase().ends_with(".esm")
}

/// `*Name.esp` / `Name.esp` lines; `#` comments and blanks ignored.
fn read_list(path: &Path) -> Vec<(String, bool)> {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| match l.strip_prefix('*') {
            Some(n) => (n.to_owned(), true),
            None => (l.to_owned(), false),
        })
        .collect()
}

/// Stable sort: implicit first, then masters, then the rest; within that,
/// every plugin is placed after its masters.
pub fn sort_by_masters(order: Vec<PluginEntry>) -> Vec<PluginEntry> {
    fn visit(i: usize, order: &[PluginEntry], index: &HashMap<String, usize>, done: &mut [bool], out: &mut Vec<usize>) {
        if done[i] {
            return;
        }
        done[i] = true;
        for m in &order[i].masters {
            if let Some(&j) = index.get(&m.to_lowercase()) {
                visit(j, order, index, done, out);
            }
        }
        out.push(i);
    }
    let index: HashMap<String, usize> = order.iter().enumerate().map(|(i, p)| (p.name.to_lowercase(), i)).collect();
    let mut rank: Vec<usize> = (0..order.len()).collect();
    rank.sort_by_key(|&i| (!order[i].implicit, !order[i].is_master, i));
    let mut done = vec![false; order.len()];
    let mut out_idx = Vec::with_capacity(order.len());
    for i in rank {
        visit(i, &order, &index, &mut done, &mut out_idx);
    }
    let mut slots: Vec<Option<PluginEntry>> = order.into_iter().map(Some).collect();
    out_idx.into_iter().map(|i| slots[i].take().expect("each once")).collect()
}

// ---------------------------------------------------------------- headers

/// Reads masters and flags from a plugin's header record.
pub fn read_header(path: &Path, format: HeaderFormat) -> Option<Header> {
    let mut buf = vec![0u8; 64 * 1024];
    let mut f = std::fs::File::open(path).ok()?;
    let n = f.read(&mut buf).ok()?;
    buf.truncate(n);
    match format {
        HeaderFormat::None => None,
        HeaderFormat::Tes4 => parse_tes4(&buf),
        HeaderFormat::Tes3 => parse_tes3(&buf),
    }
}

fn u16_at(b: &[u8], i: usize) -> Option<usize> {
    Some(u16::from_le_bytes(b.get(i..i + 2)?.try_into().ok()?) as usize)
}

fn u32_at(b: &[u8], i: usize) -> Option<usize> {
    Some(u32::from_le_bytes(b.get(i..i + 4)?.try_into().ok()?) as usize)
}

fn zstring(b: &[u8]) -> String {
    let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    // Plugin names are Windows-1252; ASCII covers practically all of them.
    b[..end].iter().map(|&c| c as char).collect()
}

/// `TES4` record: 24-byte header (20 in Oblivion), then 2-byte-size subrecords.
fn parse_tes4(b: &[u8]) -> Option<Header> {
    if b.get(0..4)? != b"TES4" {
        return None;
    }
    let size = u32_at(b, 4)?;
    let flags = u32_at(b, 8)?;
    let start = if b.get(24..28) == Some(b"HEDR") {
        24
    } else if b.get(20..24) == Some(b"HEDR") {
        20
    } else {
        return None;
    };
    let end = (start + size).min(b.len());
    let mut h = Header { master_flag: flags & 0x1 != 0, light_flag: flags & 0x200 != 0, ..Header::default() };
    let mut i = start;
    while i + 6 <= end {
        let kind = &b[i..i + 4];
        let len = u16_at(b, i + 4)?;
        let data = b.get(i + 6..i + 6 + len)?;
        if kind == b"MAST" {
            h.masters.push(zstring(data));
        }
        i += 6 + len;
    }
    Some(h)
}

/// `TES3` record: 16-byte header, then 4-byte-size subrecords.
fn parse_tes3(b: &[u8]) -> Option<Header> {
    if b.get(0..4)? != b"TES3" {
        return None;
    }
    let size = u32_at(b, 4)?;
    let end = (16 + size).min(b.len());
    let mut h = Header::default();
    let mut i = 16;
    while i + 8 <= end {
        let kind = &b[i..i + 4];
        let len = u32_at(b, i + 4)?;
        let data = b.get(i + 8..i + 8 + len)?;
        if kind == b"MAST" {
            h.masters.push(zstring(data));
        }
        i += 8 + len;
    }
    Some(h)
}

/// Builds a minimal TES4 plugin (for tests and fixtures).
#[doc(hidden)]
pub fn fake_tes4(masters: &[&str], flags: u32) -> Vec<u8> {
    let mut sub = Vec::new();
    sub.extend_from_slice(b"HEDR");
    sub.extend_from_slice(&12u16.to_le_bytes());
    sub.extend_from_slice(&[0u8; 12]);
    for m in masters {
        sub.extend_from_slice(b"MAST");
        sub.extend_from_slice(&u16::try_from(m.len() + 1).unwrap().to_le_bytes());
        sub.extend_from_slice(m.as_bytes());
        sub.push(0);
        sub.extend_from_slice(b"DATA");
        sub.extend_from_slice(&8u16.to_le_bytes());
        sub.extend_from_slice(&[0u8; 8]);
    }
    let mut out = Vec::new();
    out.extend_from_slice(b"TES4");
    out.extend_from_slice(&u32::try_from(sub.len()).unwrap().to_le_bytes());
    out.extend_from_slice(&flags.to_le_bytes());
    out.extend_from_slice(&[0u8; 12]);
    out.extend_from_slice(&sub);
    out
}

/// Plugin files in `dir` (top level), for diagnostics.
pub fn plugin_files(dir: &Path, extensions: &[String]) -> Vec<String> {
    fsx::walk_files(dir)
        .unwrap_or_default()
        .into_iter()
        .filter(|f| !f.contains('/'))
        .filter(|f| {
            extensions.iter().any(|e| f.to_ascii_lowercase().ends_with(&format!(".{}", e.to_ascii_lowercase())))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, masters: &[&str], is_master: bool) -> PluginEntry {
        PluginEntry {
            name: name.into(),
            active: true,
            implicit: false,
            provider: "x".into(),
            masters: masters.iter().map(|s| (*s).into()).collect(),
            is_master,
            is_light: false,
        }
    }

    #[test]
    fn tes4_headers() {
        let bytes = fake_tes4(&["Skyrim.esm", "Update.esm"], 0x201);
        let h = parse_tes4(&bytes).unwrap();
        assert_eq!(h.masters, ["Skyrim.esm", "Update.esm"]);
        assert!(h.master_flag && h.light_flag);
        assert!(parse_tes4(b"NOPE").is_none());
    }

    #[test]
    fn sort_respects_masters_and_keeps_order() {
        let order = vec![
            entry("Patch.esp", &["Big.esp", "Lib.esm"], false),
            entry("Other.esp", &[], false),
            entry("Big.esp", &["Lib.esm"], false),
            entry("Lib.esm", &[], true),
        ];
        let names: Vec<String> = sort_by_masters(order).into_iter().map(|p| p.name).collect();
        assert_eq!(names, ["Lib.esm", "Big.esp", "Patch.esp", "Other.esp"]);
    }
}
