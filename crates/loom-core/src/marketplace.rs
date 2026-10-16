//! The marketplace: a git repository of links to plugins.
//!
//! Each plugin is one file, `plugins/<anything>.toml`, naming its import
//! path and the versions that passed review, each pinned to a content hash.
//! Installing from the marketplace verifies that hash, so what runs on a
//! user's machine is byte-for-byte what was reviewed. New plugins arrive as
//! pull requests (see `marketplace/README.md` and `marketplace/REVIEW.md`).

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{Context, Error, Result};
use crate::library::Loom;
use crate::plugins::Remote;

/// The official marketplace.
pub const DEFAULT: &str = "github.com/enrell/loom/marketplace";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    /// The plugin's import path.
    pub id: String,
    pub name: String,
    pub kind: loom_plugin::PluginKind,
    /// Game ids it provides or supports.
    #[serde(default)]
    pub games: Vec<String>,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub tags: Vec<String>,
    /// Reviewed versions, newest last.
    pub versions: Vec<Version>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Version {
    /// Plugin version (manifest `plugin.version`).
    pub version: String,
    /// Git ref to fetch (`v1.2.0`, `plugins/skyrimse/v1.2.0`).
    #[serde(rename = "ref")]
    pub reference: String,
    /// Content hash (`loom plugin sum <dir>`).
    pub sum: String,
    /// Review date, `YYYY-MM-DD`.
    #[serde(default)]
    pub reviewed: String,
}

impl Entry {
    pub fn latest(&self) -> Option<&Version> {
        self.versions.last()
    }
}

/// Reads every entry under `<dir>/plugins/` and validates the set.
pub fn read_dir(dir: &Path) -> Result<Vec<Entry>> {
    let plugins = dir.join("plugins");
    let mut out = Vec::new();
    let mut files: Vec<PathBuf> = std::fs::read_dir(&plugins)
        .ctx(|| format!("reading {}", plugins.display()))?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "toml"))
        .collect();
    files.sort();
    for f in files {
        let text = std::fs::read_to_string(&f).ctx(|| format!("reading {}", f.display()))?;
        let e: Entry = toml::from_str(&text).map_err(|e| Error::BadFile { path: f.clone(), message: e.to_string() })?;
        out.push(e);
    }
    let problems = validate(&out);
    if !problems.is_empty() {
        return Err(Error::invalid(format!("marketplace is invalid:\n  - {}", problems.join("\n  - "))));
    }
    Ok(out)
}

/// Problems with a set of entries (what marketplace CI checks).
pub fn validate(entries: &[Entry]) -> Vec<String> {
    let mut p = Vec::new();
    let mut ids = BTreeSet::new();
    let mut game_owner: BTreeMap<&str, &str> = BTreeMap::new();
    for e in entries {
        if let Err(msg) = loom_plugin::check_import_path(&e.id) {
            p.push(format!("{}: {msg}", e.id));
        }
        if !ids.insert(&e.id) {
            p.push(format!("{}: listed twice", e.id));
        }
        if e.versions.is_empty() {
            p.push(format!("{}: no reviewed versions", e.id));
        }
        for v in &e.versions {
            if !loom_plugin::is_semver(&v.version) {
                p.push(format!("{} {}: not a semantic version", e.id, v.version));
            }
            if !v.sum.starts_with("sha256:") || v.sum.len() != 71 {
                p.push(format!("{} {}: sum must be sha256:<64 hex>", e.id, v.version));
            }
            if v.reference.is_empty() {
                p.push(format!("{} {}: empty ref", e.id, v.version));
            }
        }
        if e.kind == loom_plugin::PluginKind::Game {
            for g in &e.games {
                if let Some(other) = game_owner.insert(g, &e.id) {
                    p.push(format!("game {g:?} is provided by both {other} and {}", e.id));
                }
            }
        }
    }
    p
}

impl Loom {
    fn marketplace_source(&self) -> String {
        self.config.marketplace.clone().unwrap_or_else(|| DEFAULT.to_owned())
    }

    /// Fetches the marketplace index (a local folder, or an import path
    /// fetched at its default branch) and caches it.
    pub fn marketplace(&self) -> Result<Vec<Entry>> {
        let source = self.marketplace_source();
        let local = Path::new(&source);
        if local.is_dir() {
            return read_dir(local);
        }
        let remote = Remote::parse(&source)?;
        let cache = self.home().join("cache").join("marketplace");
        let staging = crate::plugins::tempdir(&self.home().join("cache"))?;
        let fetched = remote.download("main", &staging).and_then(|_| read_dir(&staging));
        match fetched {
            Ok(entries) => {
                let _ = std::fs::remove_dir_all(&cache);
                std::fs::rename(&staging, &cache).ctx(|| "caching the marketplace".into())?;
                Ok(entries)
            }
            Err(e) => {
                let _ = std::fs::remove_dir_all(&staging);
                // Offline: fall back to the last good copy.
                read_dir(&cache).map_err(|_| e)
            }
        }
    }

    /// Entries whose name, id, games or tags contain `query` (case-insensitive).
    pub fn search_marketplace(&self, query: &str) -> Result<Vec<Entry>> {
        let q = query.to_lowercase();
        Ok(self
            .marketplace()?
            .into_iter()
            .filter(|e| {
                q.is_empty()
                    || e.name.to_lowercase().contains(&q)
                    || e.id.to_lowercase().contains(&q)
                    || e.games.iter().chain(&e.tags).any(|g| g.to_lowercase().contains(&q))
            })
            .collect())
    }

    /// Installs a marketplace plugin at a reviewed version (latest by
    /// default), verifying its content hash.
    pub fn install_from_marketplace(
        &self,
        id_or_game: &str,
        version: Option<&str>,
    ) -> Result<crate::plugins::Installed> {
        let entries = self.marketplace()?;
        let entry = entries
            .iter()
            .find(|e| e.id == id_or_game || e.games.iter().any(|g| g == id_or_game))
            .ok_or_else(|| Error::invalid(format!("{id_or_game:?} is not in the marketplace")))?;
        let v = match version {
            Some(want) => entry.versions.iter().find(|v| v.version == want),
            None => entry.latest(),
        }
        .ok_or_else(|| Error::invalid(format!("{} has no reviewed version {}", entry.id, version.unwrap_or("?"))))?;
        self.plugins.install_remote(&format!("{}@{}", entry.id, v.reference), Some(&v.sum))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str, game: &str) -> Entry {
        Entry {
            id: id.into(),
            name: "x".into(),
            kind: loom_plugin::PluginKind::Game,
            games: vec![game.into()],
            description: String::new(),
            tags: vec![],
            versions: vec![Version {
                version: "0.1.0".into(),
                reference: "v0.1.0".into(),
                sum: format!("sha256:{}", "0".repeat(64)),
                reviewed: String::new(),
            }],
        }
    }

    #[test]
    fn validation() {
        assert_eq!(
            validate(&[entry("github.com/a/b", "g1"), entry("github.com/a/c", "g2")]),
            [] as [std::string::String; 0]
        );
        let p = validate(&[entry("github.com/a/b", "g1"), entry("github.com/a/c", "g1")]);
        assert!(p[0].contains("provided by both"), "{p:?}");
        let mut bad = entry("nope", "g");
        bad.versions[0].sum = "md5:1".into();
        assert_eq!(validate(&[bad]).len(), 2);
    }
}
