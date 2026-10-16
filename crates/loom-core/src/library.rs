//! [`Loom`]: the library on disk, and [`Game`]: one managed game.
//!
//! Every user-facing operation is a method here (or on [`Game`]); the CLI,
//! the MCP server and the desktop app are thin layers over these methods, so
//! a person and an agent always have the same powers.

use std::path::{Path, PathBuf};

use loom_plugin::GameSpec;
use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::error::{Context, Error, Result};
use crate::fsx;
use crate::plugins::{Installed, Store};
use crate::profile::Profile;

/// `games/<id>/instance.toml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Instance {
    /// Game slug, as declared by its plugin.
    pub game: String,
    /// The game's install folder.
    pub path: PathBuf,
    /// Active profile.
    pub profile: String,
}

/// The Loom library.
#[derive(Debug, Clone)]
pub struct Loom {
    home: PathBuf,
    pub config: Config,
    pub plugins: Store,
}

impl Loom {
    /// Opens (creating if needed) the library at `home`.
    pub fn open(home: impl Into<PathBuf>) -> Result<Self> {
        let home = home.into();
        std::fs::create_dir_all(home.join("games")).ctx(|| format!("creating {}", home.display()))?;
        let config = Config::load(&home.join("config.toml"))?;
        let plugins = Store::new(&home);
        Ok(Self { home, config, plugins })
    }

    /// Opens the default library ([`crate::paths::default_home`]).
    pub fn open_default() -> Result<Self> {
        Self::open(crate::paths::default_home())
    }

    pub fn home(&self) -> &Path {
        &self.home
    }

    pub fn save_config(&self) -> Result<()> {
        self.config.save(&self.home.join("config.toml"))
    }

    fn game_dir(&self, id: &str) -> PathBuf {
        self.home.join("games").join(id)
    }

    /// Games that have been set up, whether or not their plugin is installed.
    pub fn game_ids(&self) -> Result<Vec<String>> {
        let dir = self.home.join("games");
        let mut ids = Vec::new();
        for e in std::fs::read_dir(&dir).ctx(|| format!("reading {}", dir.display()))? {
            let e = e.ctx(|| format!("reading {}", dir.display()))?;
            if e.path().join("instance.toml").is_file() {
                ids.push(e.file_name().to_string_lossy().into_owned());
            }
        }
        ids.sort();
        Ok(ids)
    }

    /// Opens a set-up game. Fails with [`Error::NoPlugin`] when its plugin was
    /// removed — the game's data stays intact until the plugin comes back.
    pub fn game(&self, id: &str) -> Result<Game> {
        let dir = self.game_dir(id);
        let path = dir.join("instance.toml");
        let text = std::fs::read_to_string(&path).map_err(|_| Error::UnknownGame(id.into()))?;
        let instance: Instance =
            toml::from_str(&text).map_err(|e| Error::BadFile { path: path.clone(), message: e.to_string() })?;
        let plugin = self.plugins.for_game(id)?;
        Ok(Game { dir, instance, plugin, loom: self.clone() })
    }

    /// Sets up a game from an installed plugin. Without `path`, the game is
    /// detected; with several installs found the first wins.
    pub fn add_game(&self, id: &str, path: Option<&Path>) -> Result<Game> {
        let plugin = self.plugins.for_game(id)?;
        let spec = plugin.manifest.game.as_ref().expect("game plugin");
        let path =
            match path {
                Some(p) => {
                    let p = std::path::absolute(p).ctx(|| format!("resolving {}", p.display()))?;
                    if !crate::detect::is_install(spec, &p) {
                        return Err(Error::invalid(format!(
                            "{} does not look like {} (missing {})",
                            p.display(),
                            spec.name,
                            missing_requirement(spec, &p)
                        )));
                    }
                    p
                }
                None => crate::detect::find(spec).into_iter().next().map(|c| c.path).ok_or_else(|| {
                    Error::invalid(format!("{} was not found; pass its folder with --path", spec.name))
                })?,
            };
        let dir = self.game_dir(id);
        let instance_path = dir.join("instance.toml");
        if instance_path.exists() {
            return Err(Error::invalid(format!("{id} is already set up at {}", dir.display())));
        }
        for sub in ["mods", "downloads", "profiles"] {
            std::fs::create_dir_all(dir.join(sub)).ctx(|| format!("creating {}", dir.display()))?;
        }
        let instance = Instance { game: id.into(), path, profile: "default".into() };
        write_toml(&instance_path, &instance)?;
        let game = Game { dir, instance, plugin, loom: self.clone() };
        if !game.profile_dir("default").exists() {
            Profile::default().save(&game.profile_dir("default"))?;
        }
        Ok(game)
    }

    /// Points a set-up game at a different install folder (moved game).
    pub fn set_game_path(&self, id: &str, path: &Path) -> Result<()> {
        let game = self.game(id)?;
        let path = std::path::absolute(path).ctx(|| format!("resolving {}", path.display()))?;
        if !crate::detect::is_install(game.spec(), &path) {
            return Err(Error::invalid(format!("{} does not look like {}", path.display(), game.spec().name)));
        }
        let mut instance = game.instance.clone();
        instance.path = path;
        write_toml(&self.game_dir(id).join("instance.toml"), &instance)
    }

    /// Forgets a game. Mods and profiles are deleted only with `purge`.
    pub fn remove_game(&self, id: &str, purge: bool) -> Result<()> {
        let dir = self.game_dir(id);
        if !dir.join("instance.toml").exists() {
            return Err(Error::UnknownGame(id.into()));
        }
        if purge {
            std::fs::remove_dir_all(&dir).ctx(|| format!("removing {}", dir.display()))
        } else {
            std::fs::remove_file(dir.join("instance.toml")).ctx(|| format!("removing {}", dir.display()))
        }
    }

    /// Game plugins installed, with whether each game is found on this PC.
    pub fn available_games(&self) -> Result<Vec<AvailableGame>> {
        let set_up = self.game_ids()?;
        let mut out = Vec::new();
        for p in self.plugins.list()? {
            let Some(g) = &p.manifest.game else { continue };
            out.push(AvailableGame {
                id: g.id.clone(),
                name: g.name.clone(),
                plugin: p.lock.id.clone(),
                set_up: set_up.contains(&g.id),
                found: crate::detect::find(g),
            });
        }
        Ok(out)
    }
}

/// A game plugin and where its game was found.
#[derive(Debug, Clone, Serialize)]
pub struct AvailableGame {
    pub id: String,
    pub name: String,
    pub plugin: String,
    pub set_up: bool,
    pub found: Vec<crate::detect::Candidate>,
}

fn missing_requirement(spec: &GameSpec, dir: &Path) -> String {
    spec.detect
        .requires
        .iter()
        .find(|f| !crate::paths::join_rel(dir, f).exists())
        .cloned()
        .or_else(|| spec.executables.iter().find(|e| e.default).map(|e| e.path.clone()))
        .unwrap_or_default()
}

pub(crate) fn write_toml<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let text = toml::to_string_pretty(value).expect("serializable");
    fsx::write_atomic(path, text.as_bytes()).ctx(|| format!("writing {}", path.display()))
}

pub(crate) fn read_toml<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T> {
    let text = std::fs::read_to_string(path).ctx(|| format!("reading {}", path.display()))?;
    toml::from_str(&text).map_err(|e| Error::BadFile { path: path.into(), message: e.to_string() })
}

/// One managed game: its folder in the library, its install, its plugin.
#[derive(Debug, Clone)]
pub struct Game {
    dir: PathBuf,
    pub instance: Instance,
    pub plugin: Installed,
    pub(crate) loom: Loom,
}

impl Game {
    pub fn id(&self) -> &str {
        &self.instance.game
    }

    pub fn spec(&self) -> &GameSpec {
        self.plugin.manifest.game.as_ref().expect("game plugin")
    }

    /// The game's real install folder.
    pub fn root(&self) -> &Path {
        &self.instance.path
    }

    pub fn library_dir(&self) -> &Path {
        &self.dir
    }

    pub fn mods_dir(&self) -> PathBuf {
        self.dir.join("mods")
    }

    pub fn downloads_dir(&self) -> PathBuf {
        self.dir.join("downloads")
    }

    pub fn profile_dir(&self, name: &str) -> PathBuf {
        self.dir.join("profiles").join(name)
    }

    pub fn active_profile(&self) -> &str {
        &self.instance.profile
    }

    pub fn profile(&self) -> Result<Profile> {
        Profile::load(&self.profile_dir(&self.instance.profile))
    }

    pub fn save_profile(&self, profile: &Profile) -> Result<()> {
        profile.save(&self.profile_dir(&self.instance.profile))
    }

    pub fn overwrite_dir(&self) -> PathBuf {
        self.profile_dir(&self.instance.profile).join("overwrite")
    }

    pub fn profiles(&self) -> Result<Vec<String>> {
        let dir = self.dir.join("profiles");
        let mut out = Vec::new();
        for e in std::fs::read_dir(&dir).ctx(|| format!("reading {}", dir.display()))? {
            let e = e.ctx(|| format!("reading {}", dir.display()))?;
            if e.path().join(crate::profile::PROFILE_FILE).is_file() {
                out.push(e.file_name().to_string_lossy().into_owned());
            }
        }
        out.sort();
        Ok(out)
    }

    /// Creates a profile, empty or as a copy of `from` (mods, load order and
    /// overwrite included).
    pub fn create_profile(&self, name: &str, from: Option<&str>) -> Result<()> {
        if !crate::paths::is_safe_name(name) {
            return Err(Error::invalid(format!("{name:?} is not a valid profile name")));
        }
        let dest = self.profile_dir(name);
        if dest.exists() {
            return Err(Error::invalid(format!("profile {name:?} already exists")));
        }
        if let Some(src) = from {
            let src = self.profile_dir(src);
            if !src.join(crate::profile::PROFILE_FILE).exists() {
                return Err(Error::UnknownProfile(from.unwrap_or_default().into()));
            }
            fsx::copy_tree(&src, &dest).ctx(|| format!("copying profile to {}", dest.display()))?;
            Ok(())
        } else {
            let mut p = Profile::default();
            // A new profile knows every installed mod, all disabled.
            for m in crate::mods::list_names(&self.mods_dir())? {
                p.mods.push(crate::profile::ModRef { name: m, enabled: false });
            }
            p.save(&dest)
        }
    }

    pub fn switch_profile(&mut self, name: &str) -> Result<()> {
        if !self.profile_dir(name).join(crate::profile::PROFILE_FILE).exists() {
            return Err(Error::UnknownProfile(name.into()));
        }
        self.instance.profile = name.into();
        write_toml(&self.dir.join("instance.toml"), &self.instance)
    }

    pub fn remove_profile(&self, name: &str) -> Result<()> {
        if name == self.instance.profile {
            return Err(Error::invalid("cannot remove the active profile; switch first"));
        }
        let dir = self.profile_dir(name);
        if !dir.exists() {
            return Err(Error::UnknownProfile(name.into()));
        }
        std::fs::remove_dir_all(&dir).ctx(|| format!("removing {}", dir.display()))
    }

    /// Applies `f` to every profile (used when mods are added or removed).
    pub(crate) fn for_each_profile(&self, mut f: impl FnMut(&str, &mut Profile)) -> Result<()> {
        for name in self.profiles()? {
            let dir = self.profile_dir(&name);
            let mut p = Profile::load(&dir)?;
            f(&name, &mut p);
            p.save(&dir)?;
        }
        Ok(())
    }
}
