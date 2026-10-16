//! Mods: installing (with plugin-driven layout detection and case
//! normalization), listing, removing, renaming, and file conflicts.
//!
//! A mod is a folder under `games/<id>/mods/<name>/` laid out **relative to
//! the game root** (`Data/textures/x.dds`, `skse64_loader.exe`) and nothing
//! else — it is projected into the game as is. Where it came from lives in
//! `games/<id>/meta/<name>.toml`.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use loom_plugin::{Fallback, ModLayout};
use serde::{Deserialize, Serialize};

use crate::error::{Context, Error, Result};
use crate::fsx;
use crate::library::{Game, read_toml, write_toml};
use crate::paths::{fold, glob_match, join_rel};

/// Legacy in-folder metadata name; ignored when listing mod files.
pub const META_FILE: &str = "loom-mod.toml";

/// Where a mod came from.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModMeta {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Archive or folder it was installed from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nexus: Option<NexusRef>,
    /// Set for mods a tool produced (`Generated: xTranslator`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generated_by: Option<String>,
    /// Mod this one patches or translates; kept right above it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub patches: Option<String>,
    /// Unix seconds.
    #[serde(default)]
    pub installed: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NexusRef {
    pub game: String,
    pub mod_id: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_id: Option<u64>,
}

/// A mod as listed in the active profile.
#[derive(Debug, Clone, Serialize)]
pub struct ModInfo {
    pub name: String,
    /// 0 = highest priority.
    pub priority: usize,
    pub enabled: bool,
    pub files: usize,
    pub bytes: u64,
    pub meta: ModMeta,
    /// The folder is missing from disk.
    pub missing: bool,
}

/// Mod folder names on disk, sorted.
pub fn list_names(mods_dir: &Path) -> Result<Vec<String>> {
    let mut out = Vec::new();
    if mods_dir.is_dir() {
        for e in std::fs::read_dir(mods_dir).ctx(|| format!("reading {}", mods_dir.display()))? {
            let e = e.ctx(|| format!("reading {}", mods_dir.display()))?;
            let name = e.file_name().to_string_lossy().into_owned();
            if e.path().is_dir() && !name.starts_with('.') {
                out.push(name);
            }
        }
    }
    out.sort();
    Ok(out)
}

/// What [`Game::install_mod`] did.
#[derive(Debug, Clone, Serialize)]
pub struct Installed {
    pub name: String,
    pub files: usize,
    /// How the archive was mapped (`"pack/Data" → Data`, …).
    pub layout: Vec<String>,
    pub replaced: bool,
}

/// Options for [`Game::install_mod`].
#[derive(Debug, Clone, Default)]
pub struct InstallOptions {
    pub name: Option<String>,
    pub replace: bool,
    pub meta: ModMeta,
}

impl Game {
    /// Mods in the active profile's order, plus any mod folder the profile
    /// doesn't list yet (appended, disabled).
    pub fn mods(&self) -> Result<Vec<ModInfo>> {
        let profile = self.profile()?;
        let on_disk = list_names(&self.mods_dir())?;
        let mut out = Vec::new();
        for (i, m) in profile.mods.iter().enumerate() {
            out.push(self.mod_info(&m.name, i, m.enabled));
        }
        for name in on_disk.iter().filter(|n| profile.position(n).is_none()) {
            out.push(self.mod_info(name, out.len(), false));
        }
        Ok(out)
    }

    fn mod_info(&self, name: &str, priority: usize, enabled: bool) -> ModInfo {
        let dir = self.mods_dir().join(name);
        let files = fsx::walk_files(&dir).unwrap_or_default();
        let files: Vec<&String> = files.iter().filter(|f| *f != META_FILE).collect();
        let bytes = files.iter().filter_map(|f| std::fs::metadata(join_rel(&dir, f)).ok()).map(|m| m.len()).sum();
        ModInfo {
            name: name.into(),
            priority,
            enabled,
            files: files.len(),
            bytes,
            meta: read_toml(&self.meta_path(name)).unwrap_or_default(),
            missing: !dir.is_dir(),
        }
    }

    fn meta_path(&self, name: &str) -> PathBuf {
        self.library_dir().join("meta").join(format!("{name}.toml"))
    }

    pub fn mod_meta(&self, name: &str) -> Result<ModMeta> {
        self.mod_dir(name)?;
        Ok(read_toml(&self.meta_path(name)).unwrap_or_default())
    }

    pub fn set_mod_meta(&self, name: &str, meta: &ModMeta) -> Result<()> {
        self.mod_dir(name)?;
        write_toml(&self.meta_path(name), meta)
    }

    pub fn mod_dir(&self, name: &str) -> Result<PathBuf> {
        let d = self.mods_dir().join(name);
        if d.is_dir() { Ok(d) } else { Err(Error::UnknownMod(name.into())) }
    }

    /// Installs an archive or folder as a mod. It goes to the top of the
    /// active profile, enabled, and to the top of every other profile, disabled.
    pub fn install_mod(&self, source: &Path, opts: &InstallOptions) -> Result<Installed> {
        let name = opts.name.clone().unwrap_or_else(|| crate::archive::stem(source));
        if !crate::paths::is_safe_name(&name) {
            return Err(Error::invalid(format!("{name:?} is not a valid mod name; pass --name")));
        }
        if !source.exists() {
            return Err(Error::invalid(format!("{} does not exist", source.display())));
        }
        let mods = self.mods_dir();
        let target = mods.join(&name);
        let replaced = target.exists();
        if replaced && !opts.replace {
            return Err(Error::invalid(format!("mod {name:?} is already installed (use replace)")));
        }
        let staging = crate::plugins::tempdir(&mods)?;
        let built = crate::plugins::tempdir(&mods)?;
        let result = (|| {
            crate::archive::extract(source, &staging)?;
            let plan = plan_layout(&staging, &self.spec().mods, &name);
            let mut case = CaseMap::default();
            case.learn_dir(self.root(), &self.spec().mods)?;
            for other in list_names(&mods)? {
                if other != name {
                    case.learn_mod(&mods.join(other))?;
                }
            }
            let mut files = 0;
            for (from, prefix) in &plan.mappings {
                for rel in fsx::walk_files(from).ctx(|| format!("reading {}", from.display()))? {
                    let dest_rel = if prefix.is_empty() { rel.clone() } else { format!("{prefix}/{rel}") };
                    let dest_rel = case.canonical(&dest_rel);
                    let to = join_rel(&built, &dest_rel);
                    if let Some(p) = to.parent() {
                        std::fs::create_dir_all(p).ctx(|| format!("creating {}", p.display()))?;
                    }
                    std::fs::rename(join_rel(from, &rel), &to)
                        .or_else(|_| std::fs::copy(join_rel(from, &rel), &to).map(|_| ()))
                        .ctx(|| format!("placing {dest_rel}"))?;
                    files += 1;
                }
            }
            if files == 0 {
                return Err(Error::invalid(format!("{} contains no files", source.display())));
            }
            Ok((files, plan.describe))
        })();
        let _ = std::fs::remove_dir_all(&staging);
        let (files, layout) = match result {
            Ok(v) => v,
            Err(e) => {
                let _ = std::fs::remove_dir_all(&built);
                return Err(e);
            }
        };
        if replaced {
            std::fs::remove_dir_all(&target).ctx(|| format!("replacing {}", target.display()))?;
        }
        std::fs::rename(&built, &target).ctx(|| format!("moving the mod into {}", target.display()))?;
        let mut meta = opts.meta.clone();
        meta.source.get_or_insert_with(|| source.display().to_string());
        meta.installed = now();
        write_toml(&self.meta_path(&name), &meta)?;
        let active = self.active_profile().to_owned();
        self.for_each_profile(|p, profile| profile.insert(&name, p == active, 0))?;
        Ok(Installed { name, files, layout, replaced })
    }

    /// Deletes a mod's folder and drops it from every profile.
    pub fn remove_mod(&self, name: &str) -> Result<()> {
        let dir = self.mod_dir(name)?;
        std::fs::remove_dir_all(&dir).ctx(|| format!("removing {}", dir.display()))?;
        let _ = std::fs::remove_file(self.meta_path(name));
        self.for_each_profile(|_, p| p.remove(name))
    }

    pub fn rename_mod(&self, from: &str, to: &str) -> Result<()> {
        if !crate::paths::is_safe_name(to) {
            return Err(Error::invalid(format!("{to:?} is not a valid mod name")));
        }
        let src = self.mod_dir(from)?;
        let dst = self.mods_dir().join(to);
        if dst.exists() {
            return Err(Error::invalid(format!("a mod named {to:?} already exists")));
        }
        std::fs::rename(&src, &dst).ctx(|| format!("renaming {from}"))?;
        let _ = std::fs::rename(self.meta_path(from), self.meta_path(to));
        self.for_each_profile(|_, p| p.rename(from, to))
    }

    pub fn set_mod_enabled(&self, name: &str, enabled: bool) -> Result<()> {
        self.mod_dir(name)?;
        let mut p = self.profile()?;
        if p.position(name).is_none() {
            p.insert(name, enabled, p.mods.len());
        }
        p.set_enabled(name, enabled)?;
        self.save_profile(&p)
    }

    pub fn move_mod(&self, name: &str, index: usize) -> Result<()> {
        let mut p = self.profile()?;
        p.move_to(name, index)?;
        self.save_profile(&p)
    }

    /// Files provided by more than one enabled mod, with the winner first.
    pub fn conflicts(&self) -> Result<Vec<Conflict>> {
        let profile = self.profile()?;
        let mut providers: BTreeMap<String, (String, Vec<String>)> = BTreeMap::new();
        for name in profile.enabled() {
            for rel in fsx::walk_files(&self.mods_dir().join(name)).unwrap_or_default() {
                if rel == META_FILE {
                    continue;
                }
                providers.entry(fold(&rel)).or_insert_with(|| (rel.clone(), Vec::new())).1.push(name.to_owned());
            }
        }
        Ok(providers
            .into_values()
            .filter(|(_, mods)| mods.len() > 1)
            .map(|(path, mut mods)| {
                let winner = mods.remove(0);
                Conflict { path, winner, losers: mods }
            })
            .collect())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Conflict {
    pub path: String,
    pub winner: String,
    pub losers: Vec<String>,
}

pub(crate) fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs()
}

// ---------------------------------------------------------------- layout

/// `(folder in the extracted archive, destination prefix relative to the game root)`.
#[derive(Debug, Default)]
struct Plan {
    mappings: Vec<(PathBuf, String)>,
    describe: Vec<String>,
}

fn entries(dir: &Path) -> Vec<std::fs::DirEntry> {
    std::fs::read_dir(dir).map(|rd| rd.flatten().collect()).unwrap_or_default()
}

/// `true` if `marker` (a name/glob, or a relative path whose last part may be
/// a glob) exists directly in `dir`.
fn has_marker(dir: &Path, marker: &str) -> bool {
    let (parent, last) = match marker.rsplit_once('/') {
        Some((p, l)) => (find_ci(dir, p), l),
        None => (Some(dir.to_path_buf()), marker),
    };
    parent.is_some_and(|p| entries(&p).iter().any(|e| glob_match(last, &e.file_name().to_string_lossy())))
}

fn find_ci(dir: &Path, rel: &str) -> Option<PathBuf> {
    crate::paths::find_ci_path(dir, rel)
}

fn rel_name(base: &Path, dir: &Path) -> String {
    let r = dir.strip_prefix(base).unwrap_or(dir).to_string_lossy().replace('\\', "/");
    if r.is_empty() { "(archive root)".into() } else { r }
}

/// Decides where an extracted archive's content goes (see [`ModLayout`]).
fn plan_layout(staging: &Path, layout: &ModLayout, mod_name: &str) -> Plan {
    let data_dir = layout.data_dir.trim_matches('/');
    let mut plan = Plan::default();

    if layout.fallback == Fallback::Package {
        // Every folder holding a data marker is a package placed under data_dir.
        let mut packages = Vec::new();
        collect_packages(staging, &layout.data_markers, 0, &mut packages);
        if packages.iter().any(|p| p == staging) {
            packages = vec![staging.to_path_buf()];
        }
        for p in packages {
            let folder =
                if p == staging { mod_name.to_owned() } else { p.file_name().unwrap().to_string_lossy().into_owned() };
            let dest = join_prefix(data_dir, &folder);
            plan.describe.push(format!("{} → {dest}", rel_name(staging, &p)));
            plan.mappings.push((p, dest));
        }
        if !plan.mappings.is_empty() {
            return plan;
        }
    } else {
        // Breadth-first: the shallowest folder that looks like the game root
        // or like the data folder decides.
        let mut level = vec![staging.to_path_buf()];
        for _ in 0..4 {
            for dir in &level {
                let is_root = layout.root_markers.iter().any(|m| has_marker(dir, m))
                    || (!data_dir.is_empty() && find_ci(dir, data_dir).is_some_and(|d| d.is_dir()));
                if is_root {
                    plan.describe.push(format!("{} → game root", rel_name(staging, dir)));
                    plan.mappings.push((dir.clone(), String::new()));
                    return plan;
                }
                if layout.data_markers.iter().any(|m| has_marker(dir, m)) {
                    let dest = data_dir.to_owned();
                    plan.describe.push(format!(
                        "{} → {}",
                        rel_name(staging, dir),
                        if dest.is_empty() { "game root".into() } else { dest.clone() }
                    ));
                    plan.mappings.push((dir.clone(), dest));
                    return plan;
                }
            }
            level = level
                .iter()
                .flat_map(|d| entries(d).into_iter().filter(|e| e.path().is_dir()).map(|e| e.path()))
                .collect();
            if level.is_empty() {
                break;
            }
        }
    }

    // Nothing recognizable: unwrap lone wrapper folders, then apply the fallback.
    let mut root = staging.to_path_buf();
    loop {
        let e = entries(&root);
        match e.as_slice() {
            [only] if only.path().is_dir() => root = only.path(),
            _ => break,
        }
    }
    let dest = match layout.fallback {
        Fallback::Root => String::new(),
        Fallback::Data | Fallback::Package => data_dir.to_owned(),
    };
    plan.describe.push(format!(
        "{} → {} (no known layout found)",
        rel_name(staging, &root),
        if dest.is_empty() { "game root".to_owned() } else { dest.clone() }
    ));
    plan.mappings.push((root, dest));
    plan
}

fn collect_packages(dir: &Path, markers: &[String], depth: usize, out: &mut Vec<PathBuf>) {
    if markers.iter().any(|m| has_marker(dir, m)) {
        out.push(dir.to_path_buf());
        return;
    }
    if depth >= 3 {
        return;
    }
    for e in entries(dir) {
        if e.path().is_dir() {
            collect_packages(&e.path(), markers, depth + 1, out);
        }
    }
}

fn join_prefix(a: &str, b: &str) -> String {
    match (a.is_empty(), b.is_empty()) {
        (true, _) => b.to_owned(),
        (_, true) => a.to_owned(),
        _ => format!("{a}/{b}"),
    }
}

// ---------------------------------------------------------------- case

/// Canonical casing per folded path. The game folder and the mods already
/// installed decide; a new mod's `textures/` becomes the game's `Textures/`,
/// so the merged view never shows the same folder twice.
#[derive(Debug, Default)]
pub struct CaseMap {
    known: HashMap<String, String>,
}

impl CaseMap {
    /// Learns the game folder's top level and its data folder.
    fn learn_dir(&mut self, root: &Path, layout: &ModLayout) -> Result<()> {
        for e in entries(root) {
            let n = e.file_name().to_string_lossy().into_owned();
            self.known.entry(fold(&n)).or_insert(n);
        }
        let data = layout.data_dir.trim_matches('/');
        if !data.is_empty()
            && let Some(d) = find_ci(root, data)
        {
            let base = self.canonical(data);
            for f in fsx::walk_files(&d).ctx(|| format!("reading {}", d.display()))? {
                self.learn(&format!("{base}/{f}"));
            }
        }
        Ok(())
    }

    fn learn_mod(&mut self, dir: &Path) -> Result<()> {
        for f in fsx::walk_files(dir).ctx(|| format!("reading {}", dir.display()))? {
            if f != META_FILE {
                self.learn(&f);
            }
        }
        Ok(())
    }

    fn learn(&mut self, rel: &str) {
        let mut prefix = String::new();
        for part in rel.split('/') {
            if !prefix.is_empty() {
                prefix.push('/');
            }
            prefix.push_str(part);
            self.known.entry(fold(&prefix)).or_insert_with(|| prefix.clone());
        }
    }

    /// `rel` with every known prefix replaced by its canonical casing;
    /// unknown parts are learned as given.
    pub fn canonical(&mut self, rel: &str) -> String {
        let mut out = String::new();
        for part in rel.split('/') {
            let candidate = if out.is_empty() { part.to_owned() } else { format!("{out}/{part}") };
            out.clone_from(self.known.entry(fold(&candidate)).or_insert(candidate));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layout(data: &str, data_markers: &[&str], root_markers: &[&str], fallback: Fallback) -> ModLayout {
        ModLayout {
            data_dir: data.into(),
            data_markers: data_markers.iter().map(|s| (*s).into()).collect(),
            root_markers: root_markers.iter().map(|s| (*s).into()).collect(),
            fallback,
        }
    }

    fn tree(root: &Path, files: &[&str]) {
        for f in files {
            let p = join_rel(root, f);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, "x").unwrap();
        }
    }

    fn dests(plan: &Plan, staging: &Path) -> Vec<(String, String)> {
        plan.mappings.iter().map(|(p, d)| (rel_name(staging, p), d.clone())).collect()
    }

    #[test]
    #[allow(clippy::many_single_char_names)]
    fn bethesda_layouts() {
        let l = layout("Data", &["meshes", "textures", "*.esp"], &["skse64_*.dll"], Fallback::Data);
        let t = tempfile::tempdir().unwrap();
        // wrapper folder with data content
        let a = t.path().join("a");
        tree(&a, &["Cool Mod/textures/x.dds", "Cool Mod/cool.esp"]);
        assert_eq!(dests(&plan_layout(&a, &l, "m"), &a), [("Cool Mod".into(), "Data".into())]);
        // already has Data/
        let b = t.path().join("b");
        tree(&b, &["Data/meshes/x.nif"]);
        assert_eq!(dests(&plan_layout(&b, &l, "m"), &b), [("(archive root)".into(), String::new())]);
        // script extender: root marker in a versioned folder
        let c = t.path().join("c");
        tree(&c, &["skse64_2_02/skse64_1_6.dll", "skse64_2_02/skse64_loader.exe", "skse64_2_02/Data/Scripts/a.pex"]);
        assert_eq!(dests(&plan_layout(&c, &l, "m"), &c), [("skse64_2_02".into(), String::new())]);
        // unknown content falls back to Data after unwrapping
        let d = t.path().join("d");
        tree(&d, &["wrap/readme-only/notes.txt"]);
        assert_eq!(dests(&plan_layout(&d, &l, "m"), &d), [("wrap/readme-only".into(), "Data".into())]);
    }

    #[test]
    #[allow(clippy::many_single_char_names)]
    fn package_layouts() {
        let l = layout("Mods", &["manifest.json"], &[], Fallback::Package);
        let t = tempfile::tempdir().unwrap();
        let a = t.path().join("a");
        tree(&a, &["Pack/ModA/manifest.json", "Pack/ModB/manifest.json", "Pack/ModB/assets/x.png"]);
        let mut got = dests(&plan_layout(&a, &l, "pack"), &a);
        got.sort();
        assert_eq!(got, [("Pack/ModA".into(), "Mods/ModA".into()), ("Pack/ModB".into(), "Mods/ModB".into())]);
        let b = t.path().join("b");
        tree(&b, &["manifest.json", "x.dll"]);
        assert_eq!(dests(&plan_layout(&b, &l, "Solo"), &b), [("(archive root)".into(), "Mods/Solo".into())]);
    }

    #[test]
    fn case_map_prefers_known_casing() {
        let mut c = CaseMap::default();
        c.learn("Data/Textures/Armor/a.dds");
        assert_eq!(c.canonical("data/textures/armor/b.dds"), "Data/Textures/Armor/b.dds");
        assert_eq!(c.canonical("DATA/NewDir/x"), "Data/NewDir/x");
        assert_eq!(c.canonical("data/newdir/y"), "Data/NewDir/y");
    }
}
