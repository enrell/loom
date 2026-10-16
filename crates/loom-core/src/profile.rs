//! Profiles: an ordered, enable-able list of mods (+ the load order and the
//! overwrite folder that live next to it).
//!
//! Order convention: **the first mod wins**. `mods[0]` overrides every mod
//! below it, and all mods override the game's own files.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::library::{read_toml, write_toml};

pub const PROFILE_FILE: &str = "profile.toml";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModRef {
    pub name: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    #[serde(default)]
    pub mods: Vec<ModRef>,
}

impl Profile {
    pub fn load(dir: &Path) -> Result<Self> {
        let path = dir.join(PROFILE_FILE);
        if !path.exists() {
            return Err(Error::UnknownProfile(dir.file_name().unwrap_or_default().to_string_lossy().into()));
        }
        read_toml(&path)
    }

    pub fn save(&self, dir: &Path) -> Result<()> {
        write_toml(&dir.join(PROFILE_FILE), self)
    }

    pub fn position(&self, name: &str) -> Option<usize> {
        self.mods.iter().position(|m| m.name == name)
    }

    /// Enabled mods, highest priority first.
    pub fn enabled(&self) -> impl Iterator<Item = &str> {
        self.mods.iter().filter(|m| m.enabled).map(|m| m.name.as_str())
    }

    pub fn set_enabled(&mut self, name: &str, enabled: bool) -> Result<()> {
        let m = self.mods.iter_mut().find(|m| m.name == name).ok_or_else(|| Error::UnknownMod(name.into()))?;
        m.enabled = enabled;
        Ok(())
    }

    /// Moves `name` to `index` (0 = highest priority), clamped.
    pub fn move_to(&mut self, name: &str, index: usize) -> Result<()> {
        let from = self.position(name).ok_or_else(|| Error::UnknownMod(name.into()))?;
        let m = self.mods.remove(from);
        self.mods.insert(index.min(self.mods.len()), m);
        Ok(())
    }

    /// Places `name` directly above (higher priority than) `other`.
    pub fn place_above(&mut self, name: &str, other: &str) -> Result<()> {
        let from = self.position(name).ok_or_else(|| Error::UnknownMod(name.into()))?;
        let m = self.mods.remove(from);
        let at = self.position(other).ok_or_else(|| Error::UnknownMod(other.into()))?;
        self.mods.insert(at, m);
        Ok(())
    }

    /// Adds a mod at `index` (if absent) — the default is the top.
    pub fn insert(&mut self, name: &str, enabled: bool, index: usize) {
        if self.position(name).is_none() {
            self.mods.insert(index.min(self.mods.len()), ModRef { name: name.into(), enabled });
        }
    }

    pub fn remove(&mut self, name: &str) {
        self.mods.retain(|m| m.name != name);
    }

    pub fn rename(&mut self, from: &str, to: &str) {
        for m in &mut self.mods {
            if m.name == from {
                m.name = to.into();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(names: &[&str]) -> Profile {
        Profile { mods: names.iter().map(|n| ModRef { name: (*n).into(), enabled: true }).collect() }
    }

    fn names(p: &Profile) -> Vec<&str> {
        p.mods.iter().map(|m| m.name.as_str()).collect()
    }

    #[test]
    fn ordering() {
        let mut x = p(&["a", "b", "c"]);
        x.move_to("c", 0).unwrap();
        assert_eq!(names(&x), ["c", "a", "b"]);
        x.place_above("c", "b").unwrap();
        assert_eq!(names(&x), ["a", "c", "b"]);
        x.insert("d", false, 0);
        assert_eq!(names(&x), ["d", "a", "c", "b"]);
        assert_eq!(x.enabled().collect::<Vec<_>>(), ["a", "c", "b"]);
        assert!(x.move_to("zz", 0).is_err());
    }
}
