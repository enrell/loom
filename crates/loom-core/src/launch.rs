//! Starting the game (and tools) inside the profile's virtual view.
//!
//! Two backends, both from openvfs:
//!
//! * **mount** (default when WinFsp is installed): the merged view —
//!   `[runtime files, enabled mods…, game]`, with a writable upper layer — is
//!   mounted as a real folder and the program runs from there. Every process
//!   sees it, so launchers that start the game as a child (script extenders)
//!   work. A `steam_appid.txt` is added *to the view only*, so Steam games
//!   don't restart themselves from their real folder.
//! * **hook**: no driver needed. `openvfs-inject` starts the program with the
//!   openvfs hook DLL, which virtualizes the game folder for that one process.
//!
//! In both, the game folder itself is never written: writes land in the
//! profile's overwrite folder (or, for tools, in a generated mod).

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use serde::Serialize;

use crate::error::{Context, Error, Result};
use crate::library::Game;
use crate::paths::join_rel;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Mode {
    Mount,
    Hook,
}

impl std::str::FromStr for Mode {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self> {
        match s {
            "mount" => Ok(Self::Mount),
            "hook" => Ok(Self::Hook),
            _ => Err(Error::invalid(format!("unknown launch mode {s:?} (mount or hook)"))),
        }
    }
}

/// What to run.
#[derive(Debug, Clone, Default)]
pub struct LaunchOptions {
    /// Executable id from the plugin, or a path relative to the game root.
    /// Default: the plugin's default executable.
    pub exe: Option<String>,
    pub args: Vec<String>,
    pub mode: Option<Mode>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LaunchReport {
    pub mode: Mode,
    pub program: PathBuf,
    /// Where the program saw the game (the mount point, or the game folder).
    pub game_view: PathBuf,
    pub exit_code: Option<i32>,
    /// Files written while it ran (now in the overwrite folder / generated mod).
    pub captured: usize,
    pub load_order_files: Vec<PathBuf>,
}

/// Where a run's writes go.
pub(crate) struct Target {
    /// Upper (writable) layer.
    pub upper: PathBuf,
    /// Enabled mods excluded from the lower layers (a generated mod that is
    /// the upper).
    pub exclude: Option<String>,
}

impl Game {
    /// `mount` when WinFsp is available (or configured), else `hook`.
    pub fn launch_mode(&self, requested: Option<Mode>) -> Mode {
        if let Some(m) = requested {
            return m;
        }
        match self.loom.config.launch_mode.as_deref() {
            Some("mount") => Mode::Mount,
            Some("hook") => Mode::Hook,
            _ if mount_available() => Mode::Mount,
            _ => Mode::Hook,
        }
    }

    /// Writes the load order, then runs the game in the virtual view and
    /// waits for it to exit.
    pub fn launch(&self, opts: &LaunchOptions) -> Result<LaunchReport> {
        let exe = self.resolve_exe(opts.exe.as_deref())?;
        let mut args = self.default_args(opts.exe.as_deref());
        args.extend(opts.args.iter().cloned());
        let load_order_files = if self.has_load_order() { self.deploy_load_order()? } else { Vec::new() };
        let target = Target { upper: self.overwrite_dir(), exclude: None };
        let mut report = self.run_in_view(&exe, &args, None, &target, self.launch_mode(opts.mode))?;
        report.load_order_files = load_order_files;
        Ok(report)
    }

    fn default_args(&self, exe: Option<&str>) -> Vec<String> {
        let spec = self.spec();
        let e = match exe {
            Some(id) => spec.executables.iter().find(|e| e.id == id),
            None => spec.executables.iter().find(|e| e.default),
        };
        e.map(|e| e.args.clone()).unwrap_or_default()
    }

    /// Executable path relative to the game root.
    pub fn resolve_exe(&self, exe: Option<&str>) -> Result<String> {
        let spec = self.spec();
        let found = match exe {
            None => spec.executables.iter().find(|e| e.default).map(|e| e.path.clone()),
            Some(x) => spec
                .executables
                .iter()
                .find(|e| e.id == x)
                .map(|e| e.path.clone())
                .or_else(|| Some(x.replace('\\', "/"))),
        };
        let rel = found.ok_or_else(|| Error::invalid("no executable to start"))?;
        if rel.split('/').any(|c| c == "..") || Path::new(&rel).is_absolute() {
            return Err(Error::invalid(format!("{rel:?} must be a path inside the game folder")));
        }
        if self.provider_of(&rel).is_none() {
            return Err(Error::invalid(format!("{rel} is not in the game folder or any enabled mod")));
        }
        Ok(rel)
    }

    /// The real file behind `rel` in the merged view (highest priority first).
    pub fn provider_of(&self, rel: &str) -> Option<PathBuf> {
        let mut candidates = vec![join_rel(&self.overwrite_dir(), rel)];
        if let Ok(p) = self.profile() {
            candidates.extend(p.enabled().map(|m| join_rel(&self.mods_dir().join(m), rel)));
        }
        candidates.push(join_rel(self.root(), rel));
        candidates.into_iter().find(|p| p.is_file())
    }

    /// Runs `exe` (relative to the game root, or absolute for tools) with the
    /// game folder virtualized. `cwd` defaults to the folder of `exe`.
    pub(crate) fn run_in_view(
        &self,
        exe: &str,
        args: &[String],
        cwd: Option<&Path>,
        target: &Target,
        mode: Mode,
    ) -> Result<LaunchReport> {
        std::fs::create_dir_all(&target.upper).ctx(|| format!("creating {}", target.upper.display()))?;
        let before = crate::fsx::walk_files(&target.upper).map_or(0, |f| f.len());
        let layers = self.layer_dirs(target.exclude.as_deref())?;
        let (exit_code, program, game_view) = match mode {
            Mode::Mount => self.run_mounted(exe, args, cwd, &layers, &target.upper)?,
            Mode::Hook => self.run_hooked(exe, args, cwd, &layers, &target.upper)?,
        };
        let after = crate::fsx::walk_files(&target.upper).map_or(0, |f| f.len());
        Ok(LaunchReport {
            mode,
            program,
            game_view,
            exit_code,
            captured: after.saturating_sub(before),
            load_order_files: Vec::new(),
        })
    }

    /// Enabled mod folders, highest priority first.
    fn layer_dirs(&self, exclude: Option<&str>) -> Result<Vec<(String, PathBuf)>> {
        let mut out = Vec::new();
        for m in self.profile()?.enabled() {
            if Some(m) == exclude {
                continue;
            }
            let dir = self.mods_dir().join(m);
            if dir.is_dir() {
                out.push((m.to_owned(), dir));
            }
        }
        Ok(out)
    }

    fn mountpoint(&self) -> PathBuf {
        self.library_dir().join("view")
    }

    #[cfg(windows)]
    fn run_mounted(
        &self,
        exe: &str,
        args: &[String],
        cwd: Option<&Path>,
        layers: &[(String, PathBuf)],
        upper: &Path,
    ) -> Result<(Option<i32>, PathBuf, PathBuf)> {
        use openvfs::{FileSystem, MemoryFs, OverlayFs, PhysicalFs, Vfs};
        let vfs_err = |e: openvfs::VfsError| Error::Vfs(e.to_string());
        let runtime = Arc::new(MemoryFs::new());
        // Steam games started outside Steam look for steam_appid.txt next
        // to the executable; without it they restart through Steam, from
        // the real folder, without mods. It exists only in the view.
        if let Some(app_id) = self.spec().detect.steam.first()
            && !Path::new(exe).is_absolute()
        {
            let dir = exe.rsplit_once('/').map_or("", |(d, _)| d);
            if !join_rel(self.root(), dir).join("steam_appid.txt").exists() {
                let vfs = Vfs::new(Arc::clone(&runtime));
                if !dir.is_empty() {
                    vfs.create_dir_all(format!("/{dir}")).map_err(vfs_err)?;
                }
                vfs.write(
                    format!("/{}steam_appid.txt", if dir.is_empty() { String::new() } else { format!("{dir}/") }),
                    app_id.to_string(),
                )
                .map_err(vfs_err)?;
            }
        }
        let mut lowers: Vec<Arc<dyn FileSystem>> = vec![runtime];
        for (_, dir) in layers {
            lowers.push(Arc::new(PhysicalFs::read_only(dir).map_err(vfs_err)?));
        }
        lowers.push(Arc::new(PhysicalFs::read_only(self.root()).map_err(vfs_err)?));
        let overlay = OverlayFs::with_layers(Arc::new(PhysicalFs::new(upper).map_err(vfs_err)?), lowers);
        let point = self.mountpoint();
        if point.exists() && std::fs::read_dir(&point).is_ok_and(|mut d| d.next().is_some()) {
            return Err(Error::Vfs(format!(
                "{} is not empty — is the game already running? If not, delete that folder",
                point.display()
            )));
        }
        let mount =
            openvfs_winfsp::Mount::mount(overlay, &point, openvfs_winfsp::MountOptions::default().volume_label("Loom"))
                .map_err(|e| Error::Vfs(e.to_string()))?;
        let view = PathBuf::from(mount.mountpoint());
        let program = if Path::new(exe).is_absolute() { PathBuf::from(exe) } else { join_rel(&view, exe) };
        let args: Vec<String> = args.iter().map(|a| a.replace("{game}", &view.display().to_string())).collect();
        let dir =
            cwd.map(Path::to_path_buf).or_else(|| program.parent().map(Path::to_path_buf)).unwrap_or(view.clone());
        let status = Command::new(&program).args(&args).current_dir(&dir).stdout(child_stdout()).status();
        drop(mount);
        let status = status.ctx(|| format!("starting {}", program.display()))?;
        Ok((status.code(), program, view))
    }

    #[cfg(not(windows))]
    fn run_mounted(
        &self,
        _exe: &str,
        _args: &[String],
        _cwd: Option<&Path>,
        _layers: &[(String, PathBuf)],
        _upper: &Path,
    ) -> Result<(Option<i32>, PathBuf, PathBuf)> {
        Err(Error::Vfs("mount mode needs Windows with WinFsp".into()))
    }

    fn run_hooked(
        &self,
        exe: &str,
        args: &[String],
        cwd: Option<&Path>,
        layers: &[(String, PathBuf)],
        upper: &Path,
    ) -> Result<(Option<i32>, PathBuf, PathBuf)> {
        let (inject, dll) = openvfs_binaries(self.loom.config.openvfs_dir.as_deref())?;
        let config = serde_json::json!({
            "virtual_root": self.root(),
            "overwrite": upper,
            "layers": layers.iter().map(|(name, dir)| serde_json::json!({ "name": name, "dir": dir, "enabled": true })).collect::<Vec<_>>(),
        });
        let config_path = self.library_dir().join("hook.json");
        crate::fsx::write_atomic(&config_path, serde_json::to_string_pretty(&config).expect("json").as_bytes())
            .ctx(|| format!("writing {}", config_path.display()))?;
        // The injector starts a real file: a program provided by a mod is
        // started from the mod's folder, with the game folder as cwd.
        let program = if Path::new(exe).is_absolute() {
            PathBuf::from(exe)
        } else {
            self.provider_of(exe).ok_or_else(|| Error::invalid(format!("{exe} not found")))?
        };
        let view = self.root().to_path_buf();
        let args: Vec<String> = args.iter().map(|a| a.replace("{game}", &view.display().to_string())).collect();
        let dir = cwd.map_or_else(
            || {
                if Path::new(exe).is_absolute() {
                    program.parent().map(Path::to_path_buf).unwrap_or(view.clone())
                } else {
                    view.clone()
                }
            },
            Path::to_path_buf,
        );
        let status = Command::new(&inject)
            .arg("--dll")
            .arg(&dll)
            .arg("--")
            .arg(&program)
            .args(&args)
            .current_dir(&dir)
            .env("OPENVFS_HOOK_CONFIG", &config_path)
            .stdout(child_stdout())
            .status()
            .ctx(|| format!("starting {}", inject.display()))?;
        Ok((status.code(), program, view))
    }
}

/// A launched program's stdout goes to our stderr: our stdout carries
/// results (JSON for agents) and must stay clean.
fn child_stdout() -> std::process::Stdio {
    std::process::Stdio::from(std::io::stderr())
}

/// `true` when the WinFsp runtime can be loaded.
pub fn mount_available() -> bool {
    #[cfg(windows)]
    {
        openvfs_winfsp::Mount::is_available()
    }
    #[cfg(not(windows))]
    {
        false
    }
}

/// `(openvfs-inject.exe, openvfs_hook.dll)` from the configured folder, the
/// running executable's folder, or `PATH`.
pub fn openvfs_binaries(configured: Option<&Path>) -> Result<(PathBuf, PathBuf)> {
    let mut dirs: Vec<PathBuf> = configured.map(Path::to_path_buf).into_iter().collect();
    if let Ok(exe) = std::env::current_exe()
        && let Some(d) = exe.parent()
    {
        dirs.push(d.to_path_buf());
    }
    if let Some(path) = std::env::var_os("PATH") {
        dirs.extend(std::env::split_paths(&path));
    }
    for d in dirs {
        let (inject, dll) = (d.join("openvfs-inject.exe"), d.join("openvfs_hook.dll"));
        if inject.is_file() && dll.is_file() {
            return Ok((inject, dll));
        }
    }
    Err(Error::Vfs(
        "hook mode needs openvfs-inject.exe and openvfs_hook.dll — put them next to loom.exe or set openvfs_dir; \
         or install WinFsp (winget install WinFsp.WinFsp) to use mount mode"
            .into(),
    ))
}
