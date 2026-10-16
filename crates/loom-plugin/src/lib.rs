//! # loom-plugin — the `loom-plugin.toml` format
//!
//! A Loom plugin is a folder (usually a git repository or a sub-folder of one)
//! with a `loom-plugin.toml` at its root. Plugins are **declarative**: they
//! describe a game — how to find it, how to start it, where mod files go, how
//! its load order is stored — and the companion tools that go with it. They
//! never contain code, which is what makes community plugins safe to review and
//! to install.
//!
//! The core implements generic *capabilities* (Steam library scanning, a
//! `plugins.txt`-style load order, archive layout detection); a plugin selects
//! and configures them. See `docs/PLUGINS.md` for the authoring guide.
//!
//! ```toml
//! [plugin]
//! id = "github.com/enrell/loom/plugins/stardewvalley"
//! name = "Stardew Valley"
//! version = "0.1.0"
//! kind = "game"
//!
//! [game]
//! id = "stardewvalley"
//! name = "Stardew Valley"
//! nexus_domain = "stardewvalley"
//! executables = [{ id = "smapi", name = "SMAPI", path = "StardewModdingAPI.exe", default = true }]
//!
//! [game.detect]
//! steam = [413150]
//! requires = ["Stardew Valley.dll"]
//!
//! [game.mods]
//! data_dir = "Mods"
//! data_markers = ["manifest.json"]
//! ```

use std::collections::BTreeSet;
use std::fmt;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// The file name every plugin must have at its root.
pub const MANIFEST_FILE: &str = "loom-plugin.toml";

/// Manifest format version this crate reads and writes.
pub const FORMAT_VERSION: u32 = 1;

/// Placeholders a path template may use. Expanded by the core at runtime.
pub const TEMPLATE_VARS: &[&str] =
    &["%LOCALAPPDATA%", "%APPDATA%", "%USERPROFILE%", "%DOCUMENTS%", "%MYGAMES%", "{game}"];

/// A parsed `loom-plugin.toml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    /// Manifest format version; defaults to the current one.
    #[serde(default = "default_format")]
    pub format: u32,
    pub plugin: PluginMeta,
    /// Present for `kind = "game"` plugins.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub game: Option<GameSpec>,
    /// Companion tools (translators, patchers, xEdit-style editors, …).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<ToolSpec>,
}

fn default_format() -> u32 {
    FORMAT_VERSION
}

/// `[plugin]`: identity and provenance.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginMeta {
    /// Import path, Go-style: `github.com/<owner>/<repo>[/<subdir>]`.
    pub id: String,
    /// Human-readable name.
    pub name: String,
    /// Semantic version of the plugin itself (`1.2.0`, no leading `v`).
    pub version: String,
    pub kind: PluginKind,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub authors: Vec<String>,
    /// SPDX license expression.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub license: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub homepage: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PluginKind {
    /// Adds support for one game.
    Game,
    /// Adds tools for games other plugins provide.
    Tools,
}

/// `[game]`: everything Loom needs to manage one game.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GameSpec {
    /// Stable slug (`skyrimse`). Unique across the marketplace.
    pub id: String,
    pub name: String,
    /// The game's domain on nexusmods.com (`skyrimspecialedition`), for
    /// `nxm://` links and API calls.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nexus_domain: Option<String>,
    /// Programs that can be started for this game, relative to the game root.
    pub executables: Vec<Executable>,
    #[serde(default)]
    pub detect: Detect,
    pub mods: ModLayout,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub load_order: Option<LoadOrder>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Executable {
    pub id: String,
    pub name: String,
    /// Path relative to the game root.
    pub path: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    /// The one `loom launch` uses without `--exe`.
    #[serde(default)]
    pub default: bool,
    /// Only offered when the file exists (script extenders, launchers).
    #[serde(default)]
    pub optional: bool,
}

/// `[game.detect]`: how to find an installation.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Detect {
    /// Steam app ids; the game folder is found through the Steam libraries.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub steam: Vec<u32>,
    /// GOG product ids (looked up in GOG Galaxy's registry entries).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub gog: Vec<String>,
    /// Registry values holding the install path.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub registry: Vec<RegistryHint>,
    /// Steam/GOG folder name, when the library manifest is missing.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub folder_names: Vec<String>,
    /// Files that must exist under the game root for a folder to count.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requires: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegistryHint {
    /// Full key, e.g. `HKLM\SOFTWARE\WOW6432Node\Bethesda Softworks\Fallout4`.
    pub key: String,
    /// Value name; empty for the default value.
    #[serde(default)]
    pub value: String,
}

/// `[game.mods]`: where mod content goes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModLayout {
    /// Folder (relative to the game root) most mod content belongs in:
    /// `Data` for Bethesda games, `Mods` for SMAPI, `""` for the root itself.
    #[serde(default)]
    pub data_dir: String,
    /// Names or globs that identify the *data* level of an archive: when one
    /// of them is found, that folder maps to `data_dir`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub data_markers: Vec<String>,
    /// Names or globs that identify the *game root* level of an archive
    /// (script extender DLLs, `bin/`, `archive/pc/mod/` …).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub root_markers: Vec<String>,
    /// How a mod that matches no marker is placed.
    #[serde(default)]
    pub fallback: Fallback,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Fallback {
    /// Put it under `data_dir` (the usual case).
    #[default]
    Data,
    /// Put it at the game root.
    Root,
    /// Each top-level folder is its own mod package (SMAPI, BepInEx plugins):
    /// keep the folder and put it under `data_dir`.
    Package,
}

/// `[game.load_order]`: the order the game loads its plugin files in.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoadOrder {
    pub format: LoadOrderFormat,
    /// Where the active list lives (template), e.g.
    /// `%LOCALAPPDATA%/Skyrim Special Edition/Plugins.txt`.
    pub plugins_file: String,
    /// Optional full-order file (`loadorder.txt`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loadorder_file: Option<String>,
    /// File extensions that are load-order plugins (`esm`, `esp`, `esl`).
    pub extensions: Vec<String>,
    /// `*Name.esp` marks active entries (newer Bethesda games); without it
    /// the file lists active plugins only.
    #[serde(default)]
    pub star_prefix: bool,
    /// Always loaded first, in this order, and never written to the file.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub implicit: Vec<String>,
    /// A file listing more always-loaded plugins, one per line, loaded after
    /// `implicit` (Creation Club content: `{game}/Skyrim.ccc`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub implicit_file: Option<String>,
    /// Extension of "light" plugins that don't count toward `max_plugins`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub light_extension: Option<String>,
    /// Largest number of full (non-light) active plugins.
    #[serde(default = "default_max_plugins")]
    pub max_plugins: u32,
    /// Plugin header format, for reading masters and flags.
    #[serde(default)]
    pub header: HeaderFormat,
    /// The game orders plugins by file modification time (Oblivion, Fallout
    /// 3/New Vegas, Skyrim LE). Loom then sets the times of the plugin files
    /// in its own mod folders to match the load order (never the game's).
    #[serde(default)]
    pub timestamps: bool,
}

fn default_max_plugins() -> u32 {
    255
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LoadOrderFormat {
    /// Bethesda-style text file of plugin names.
    PluginsTxt,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HeaderFormat {
    /// Don't read headers (no master checks).
    #[default]
    None,
    /// `TES4` record (Oblivion, Fallout 3/NV, Skyrim, Fallout 4, Starfield).
    Tes4,
    /// `TES3` record (Morrowind).
    Tes3,
}

/// `[[tools]]`: a companion program.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolSpec {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// Executable file names to look for.
    pub exe: Vec<String>,
    /// Folders (templates) searched for the executable, besides the game root
    /// and the user's configured tool paths.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub search: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    /// Where the tool's writes go.
    #[serde(default)]
    pub output: ToolOutput,
    /// Where to get the tool (shown to users; never downloaded automatically).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub homepage: Option<String>,
    /// Guidance for agents driving this tool through MCP.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub agent_notes: String,
    /// Game ids this tool is for. Empty: the game of this plugin (game
    /// plugins) or any game (tools plugins).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub games: Vec<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ToolOutput {
    /// A dedicated mod, `Generated: <tool name>`.
    #[default]
    GeneratedMod,
    /// The profile's overwrite folder.
    Overwrite,
}

// ---------------------------------------------------------------- parsing

/// A manifest that failed to parse or validate.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{MANIFEST_FILE}: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("{MANIFEST_FILE} is invalid:\n{0}")]
    Invalid(Problems),
    #[error("{0}: {1}")]
    Io(String, std::io::Error),
}

/// Every validation problem found, one per line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problems(pub Vec<String>);

impl fmt::Display for Problems {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for p in &self.0 {
            writeln!(f, "  - {p}")?;
        }
        Ok(())
    }
}

impl Manifest {
    /// Parses and validates a manifest.
    pub fn parse(text: &str) -> Result<Self, Error> {
        let m: Self = toml::from_str(text)?;
        m.validate().map_err(Error::Invalid)?;
        Ok(m)
    }

    /// Reads `<dir>/loom-plugin.toml`.
    pub fn load_dir(dir: &Path) -> Result<Self, Error> {
        let path = dir.join(MANIFEST_FILE);
        let text = std::fs::read_to_string(&path).map_err(|e| Error::Io(path.display().to_string(), e))?;
        Self::parse(&text)
    }

    pub fn to_toml(&self) -> String {
        toml::to_string_pretty(self).expect("manifest serializes")
    }

    /// Checks everything that the schema alone can't. Returns all problems.
    pub fn validate(&self) -> Result<(), Problems> {
        let mut p = Vec::new();
        if self.format != FORMAT_VERSION {
            p.push(format!("format = {} is not supported (this Loom reads format {FORMAT_VERSION})", self.format));
        }
        let meta = &self.plugin;
        if let Err(e) = check_import_path(&meta.id) {
            p.push(format!("plugin.id: {e}"));
        }
        if meta.name.trim().is_empty() {
            p.push("plugin.name is empty".into());
        }
        if !is_semver(&meta.version) {
            p.push(format!("plugin.version {:?} is not a semantic version like 1.2.0", meta.version));
        }
        if let Some(h) = &meta.homepage
            && !h.starts_with("https://")
        {
            p.push(format!("plugin.homepage {h:?} must be an https:// URL"));
        }
        match (meta.kind, &self.game) {
            (PluginKind::Game, None) => p.push("kind = \"game\" needs a [game] table".into()),
            (PluginKind::Tools, Some(_)) => p.push("kind = \"tools\" must not have a [game] table".into()),
            (PluginKind::Tools, None) if self.tools.is_empty() => {
                p.push("kind = \"tools\" needs at least one [[tools]]".into());
            }
            _ => {}
        }
        if let Some(g) = &self.game {
            validate_game(g, &mut p);
        }
        let mut tool_ids = BTreeSet::new();
        for (i, t) in self.tools.iter().enumerate() {
            let at = format!("tools[{i}] ({})", t.id);
            if !is_slug(&t.id) {
                p.push(format!("{at}: id must be a lowercase slug"));
            }
            if !tool_ids.insert(&t.id) {
                p.push(format!("{at}: duplicate tool id"));
            }
            if t.exe.is_empty() {
                p.push(format!("{at}: exe lists no file names"));
            }
            for e in &t.exe {
                if e.contains(['/', '\\']) || !e.to_ascii_lowercase().ends_with(".exe") {
                    p.push(format!("{at}: exe {e:?} must be a bare .exe file name"));
                }
            }
            for s in &t.search {
                check_template(s, &format!("{at}.search"), &mut p);
            }
            if let Some(h) = &t.homepage
                && !h.starts_with("https://")
            {
                p.push(format!("{at}: homepage must be an https:// URL"));
            }
            for g in &t.games {
                if !is_slug(g) {
                    p.push(format!("{at}: games entry {g:?} must be a game id slug"));
                }
            }
        }
        if p.is_empty() { Ok(()) } else { Err(Problems(p)) }
    }
}

fn validate_game(g: &GameSpec, p: &mut Vec<String>) {
    if !is_slug(&g.id) {
        p.push(format!("game.id {:?} must be a lowercase slug (a-z, 0-9, -)", g.id));
    }
    if let Some(d) = &g.nexus_domain
        && !is_slug(d)
    {
        p.push(format!("game.nexus_domain {d:?} must be a lowercase slug"));
    }
    if g.executables.is_empty() {
        p.push("game.executables is empty".into());
    }
    let defaults = g.executables.iter().filter(|e| e.default).count();
    if defaults != 1 {
        p.push(format!("game.executables needs exactly one default = true (has {defaults})"));
    }
    let mut ids = BTreeSet::new();
    for e in &g.executables {
        if !is_slug(&e.id) {
            p.push(format!("executable {:?}: id must be a lowercase slug", e.id));
        }
        if !ids.insert(&e.id) {
            p.push(format!("executable {:?}: duplicate id", e.id));
        }
        check_relative(&e.path, &format!("executable {:?} path", e.id), p);
    }
    let d = &g.detect;
    if d.steam.is_empty() && d.gog.is_empty() && d.registry.is_empty() && d.folder_names.is_empty() {
        p.push("game.detect has no way to find the game (steam, gog, registry or folder_names)".into());
    }
    for r in &d.registry {
        let k = r.key.to_ascii_uppercase();
        if !(k.starts_with("HKLM\\") || k.starts_with("HKCU\\")) {
            p.push(format!("game.detect.registry key {:?} must start with HKLM\\ or HKCU\\", r.key));
        }
    }
    for f in &d.requires {
        check_relative(f, "game.detect.requires", p);
    }
    if !g.mods.data_dir.is_empty() {
        check_relative(&g.mods.data_dir, "game.mods.data_dir", p);
    }
    if g.mods.data_markers.is_empty() && g.mods.root_markers.is_empty() {
        p.push("game.mods needs data_markers or root_markers to lay out archives".into());
    }
    for m in g.mods.data_markers.iter().chain(&g.mods.root_markers) {
        check_relative(m, "game.mods marker", p);
    }
    if let Some(lo) = &g.load_order {
        check_template(&lo.plugins_file, "game.load_order.plugins_file", p);
        if let Some(f) = &lo.loadorder_file {
            check_template(f, "game.load_order.loadorder_file", p);
        }
        if let Some(f) = &lo.implicit_file {
            check_template(f, "game.load_order.implicit_file", p);
        }
        if lo.extensions.is_empty() {
            p.push("game.load_order.extensions is empty".into());
        }
        for e in &lo.extensions {
            if e.starts_with('.') || e.contains(['/', '\\']) {
                p.push(format!("game.load_order.extensions: {e:?} must be a bare extension like \"esp\""));
            }
        }
        if let Some(l) = &lo.light_extension
            && !lo.extensions.iter().any(|e| e.eq_ignore_ascii_case(l))
        {
            p.push(format!("game.load_order.light_extension {l:?} is not in extensions"));
        }
        if lo.max_plugins == 0 {
            p.push("game.load_order.max_plugins must be at least 1".into());
        }
    }
}

/// `github.com/owner/repo[/sub/dir]`: lowercase host with a dot, at least
/// owner and repo, no `..`, no scheme.
pub fn check_import_path(id: &str) -> Result<(), String> {
    if id.contains("://") {
        return Err(format!("{id:?} must not include a scheme; use e.g. github.com/owner/repo"));
    }
    let parts: Vec<&str> = id.split('/').collect();
    if parts.len() < 3 {
        return Err(format!("{id:?} must look like host/owner/repo[/subdir]"));
    }
    if !parts[0].contains('.') || parts[0] != parts[0].to_ascii_lowercase() {
        return Err(format!("{id:?}: the host {:?} must be a lowercase domain", parts[0]));
    }
    for part in &parts[1..] {
        let ok = !part.is_empty()
            && *part != "."
            && *part != ".."
            && part.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
        if !ok {
            return Err(format!("{id:?}: path element {part:?} is not allowed"));
        }
    }
    Ok(())
}

pub fn is_slug(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !s.starts_with('-')
        && !s.ends_with('-')
}

/// `MAJOR.MINOR.PATCH` with an optional `-pre` suffix.
pub fn is_semver(s: &str) -> bool {
    let core = s.split_once('-').map_or(s, |(c, pre)| if pre.is_empty() { "" } else { c });
    let nums: Vec<&str> = core.split('.').collect();
    nums.len() == 3 && nums.iter().all(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
}

/// A path relative to the game root: no drive, no leading slash, no `..`.
fn check_relative(path: &str, what: &str, p: &mut Vec<String>) {
    let bad = path.is_empty()
        || path.starts_with(['/', '\\'])
        || path.contains(':')
        || path.split(['/', '\\']).any(|c| c == "..");
    if bad {
        p.push(format!("{what} {path:?} must be a relative path inside the game folder"));
    }
}

/// A path template must start with a known placeholder and not climb out.
fn check_template(t: &str, what: &str, p: &mut Vec<String>) {
    if !TEMPLATE_VARS.iter().any(|v| t.starts_with(v)) {
        p.push(format!("{what} {t:?} must start with one of {}", TEMPLATE_VARS.join(", ")));
    }
    if t.split(['/', '\\']).any(|c| c == "..") {
        p.push(format!("{what} {t:?} must not contain .."));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD: &str = r#"
[plugin]
id = "github.com/enrell/loom/plugins/skyrimse"
name = "Skyrim Special Edition"
version = "0.1.0"
kind = "game"

[game]
id = "skyrimse"
name = "The Elder Scrolls V: Skyrim Special Edition"
nexus_domain = "skyrimspecialedition"
executables = [
  { id = "skyrim", name = "Skyrim Special Edition", path = "SkyrimSE.exe", default = true },
  { id = "skse", name = "SKSE", path = "skse64_loader.exe", optional = true },
]

[game.detect]
steam = [489830]
requires = ["SkyrimSE.exe"]

[game.mods]
data_dir = "Data"
data_markers = ["meshes", "textures", "*.esp"]
root_markers = ["skse64_*.dll"]

[game.load_order]
format = "plugins-txt"
plugins_file = "%LOCALAPPDATA%/Skyrim Special Edition/Plugins.txt"
extensions = ["esm", "esp", "esl"]
star_prefix = true
light_extension = "esl"
header = "tes4"

[[tools]]
id = "xtranslator"
name = "xTranslator"
exe = ["xTranslator.exe"]
"#;

    #[test]
    fn good_manifest_parses_and_round_trips() {
        let m = Manifest::parse(GOOD).unwrap();
        assert_eq!(m.game.as_ref().unwrap().load_order.as_ref().unwrap().max_plugins, 255);
        assert_eq!(Manifest::parse(&m.to_toml()).unwrap(), m);
    }

    #[test]
    fn unknown_fields_are_rejected() {
        let err = Manifest::parse(&GOOD.replace("kind = \"game\"", "kind = \"game\"\nsurprise = 1")).unwrap_err();
        assert!(err.to_string().contains("surprise"), "{err}");
    }

    #[test]
    fn validation_collects_every_problem() {
        let bad = GOOD
            .replace("github.com/enrell/loom/plugins/skyrimse", "https://evil")
            .replace("version = \"0.1.0\"", "version = \"one\"")
            .replace("path = \"SkyrimSE.exe\"", "path = \"../../Windows/x.exe\"")
            .replace("%LOCALAPPDATA%/Skyrim", "C:/Skyrim");
        let Err(Error::Invalid(Problems(p))) = Manifest::parse(&bad) else { panic!("should be invalid") };
        assert!(p.len() >= 4, "{p:#?}");
    }

    #[test]
    fn import_paths() {
        assert!(check_import_path("github.com/a/b").is_ok());
        assert!(check_import_path("github.com/a/b/c/d").is_ok());
        assert!(check_import_path("github.com/a").is_err());
        assert!(check_import_path("github.com/a/../b").is_err());
        assert!(check_import_path("GitHub.com/a/b").is_err());
    }

    #[test]
    fn semver() {
        assert!(is_semver("1.2.3"));
        assert!(is_semver("0.1.0-beta.1"));
        assert!(!is_semver("v1.2.3"));
        assert!(!is_semver("1.2"));
    }
}
