//! `<library>/config.toml`: user settings.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{Context, Error, Result};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// Personal Nexus Mods API key (nexusmods.com → Site preferences → API).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nexus_api_key: Option<String>,
    /// Folder holding `openvfs-inject.exe` and `openvfs_hook.dll` (hook mode).
    /// Defaults to the folder of the running executable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub openvfs_dir: Option<PathBuf>,
    /// Extra folders searched for tools (xTranslator, xEdit, …).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tool_paths: Vec<PathBuf>,
    /// Marketplace import path; defaults to [`crate::marketplace::DEFAULT`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub marketplace: Option<String>,
    /// How games are started: `auto` (mount when WinFsp is installed, else
    /// hook), `mount` or `hook`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub launch_mode: Option<String>,
    /// The desktop app already offered the AI-agent setup wizard.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub agents_wizard_seen: bool,
}

impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        match std::fs::read_to_string(path) {
            Ok(text) => toml::from_str(&text).map_err(|e| Error::BadFile { path: path.into(), message: e.to_string() }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(Error::Io { context: format!("reading {}", path.display()), source: e }),
        }
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let text = toml::to_string_pretty(self).expect("config serializes");
        crate::fsx::write_atomic(path, text.as_bytes()).ctx(|| format!("writing {}", path.display()))
    }
}
