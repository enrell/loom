//! Installed plugins: Go-style fetching, the lock file and the data-only policy.
//!
//! * **Identity.** A plugin is its import path, `github.com/<owner>/<repo>[/<subdir>]`.
//!   The manifest's `plugin.id` must equal the path it was fetched from, so a
//!   repository can't claim to be another plugin.
//! * **Versions** are git refs. `@v1.2.0` for a repository-root plugin,
//!   `@<subdir>/v1.2.0` for one in a sub-folder (Go's monorepo tag rule);
//!   `@latest` picks the highest such tag; a branch or commit also works.
//! * **Lock.** `plugins.lock` records the resolved commit and a content hash
//!   (`sum`) of every installed plugin, like `go.sum`.
//! * **Data only.** Plugins may contain manifests, docs and images — never
//!   programs or scripts. The core refuses anything else, so installing a
//!   plugin can't run code.

use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};

use loom_plugin::Manifest;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::{Context, Error, Result};
use crate::fsx;

/// File extensions a plugin may contain.
pub const ALLOWED_EXTENSIONS: &[&str] = &["toml", "md", "txt", "json", "png", "jpg", "jpeg", "webp", "svg"];
/// Extension-less files a plugin may contain.
pub const ALLOWED_NAMES: &[&str] = &["license", "licence", "copying", "notice", "readme", "authors"];
/// Largest plugin accepted, all files together.
pub const MAX_PLUGIN_BYTES: u64 = 5 * 1024 * 1024;

/// One entry of `plugins.lock`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Locked {
    pub id: String,
    /// The plugin's own version (from its manifest).
    pub version: String,
    /// Where it came from: `path:<dir>` or `<import-path>@<ref>`.
    pub source: String,
    /// Resolved commit, for fetched plugins.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
    /// `sha256:<hex>` over the plugin's files.
    pub sum: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct LockFile {
    #[serde(default, rename = "plugin")]
    plugins: Vec<Locked>,
}

/// An installed plugin.
#[derive(Debug, Clone, Serialize)]
pub struct Installed {
    pub manifest: Manifest,
    pub dir: PathBuf,
    pub lock: Locked,
}

/// The plugin store under `<library>/plugins`.
#[derive(Debug, Clone)]
pub struct Store {
    root: PathBuf,
    lock_path: PathBuf,
}

impl Store {
    pub fn new(home: &Path) -> Self {
        Self { root: home.join("plugins"), lock_path: home.join("plugins.lock") }
    }

    fn read_lock(&self) -> Result<LockFile> {
        match std::fs::read_to_string(&self.lock_path) {
            Ok(t) => {
                toml::from_str(&t).map_err(|e| Error::BadFile { path: self.lock_path.clone(), message: e.to_string() })
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(LockFile::default()),
            Err(e) => Err(Error::Io { context: format!("reading {}", self.lock_path.display()), source: e }),
        }
    }

    fn write_lock(&self, lock: &LockFile) -> Result<()> {
        let text = format!(
            "# Installed Loom plugins. Managed by `loom plugin`; do not edit.\n\n{}",
            toml::to_string_pretty(lock).expect("lock serializes")
        );
        fsx::write_atomic(&self.lock_path, text.as_bytes()).ctx(|| format!("writing {}", self.lock_path.display()))
    }

    fn dir_for(&self, id: &str) -> PathBuf {
        crate::paths::join_rel(&self.root, id)
    }

    /// Every installed plugin, sorted by id. Broken ones are reported, not hidden.
    pub fn list(&self) -> Result<Vec<Installed>> {
        let lock = self.read_lock()?;
        let mut out = Vec::new();
        for l in lock.plugins {
            let dir = self.dir_for(&l.id);
            let manifest = Manifest::load_dir(&dir)?;
            out.push(Installed { manifest, dir, lock: l });
        }
        out.sort_by(|a, b| a.lock.id.cmp(&b.lock.id));
        Ok(out)
    }

    pub fn get(&self, id: &str) -> Result<Option<Installed>> {
        Ok(self.list()?.into_iter().find(|p| p.lock.id == id))
    }

    /// The installed plugin that provides game `game_id`.
    pub fn for_game(&self, game_id: &str) -> Result<Installed> {
        self.list()?
            .into_iter()
            .find(|p| p.manifest.game.as_ref().is_some_and(|g| g.id == game_id))
            .ok_or_else(|| Error::NoPlugin(game_id.into()))
    }

    /// Installs (or replaces) a plugin from a local folder.
    pub fn install_dir(&self, dir: &Path) -> Result<Installed> {
        let dir = std::path::absolute(dir).ctx(|| format!("resolving {}", dir.display()))?;
        let source = format!("path:{}", dir.display());
        self.install_checked(&dir, source, None, None)
    }

    /// Installs `<import-path>[@ref]` from its host (GitHub for now).
    /// `expected_sum` pins the content (marketplace-reviewed versions).
    pub fn install_remote(&self, spec: &str, expected_sum: Option<&str>) -> Result<Installed> {
        let (path, reference) = spec.split_once('@').unwrap_or((spec, "latest"));
        loom_plugin::check_import_path(path).map_err(Error::Invalid)?;
        let remote = Remote::parse(path)?;
        let reference = if reference == "latest" { remote.latest_tag()? } else { reference.to_owned() };
        let staging = tempdir(&self.root)?;
        let commit = remote.download(&reference, &staging)?;
        let result = (|| {
            let found = Manifest::load_dir(&staging)?;
            if found.plugin.id != path {
                return Err(Error::invalid(format!(
                    "{spec}: the manifest says it is {:?}, not {path:?}; refusing it",
                    found.plugin.id
                )));
            }
            self.install_checked(&staging, format!("{path}@{reference}"), Some(commit), expected_sum)
        })();
        let _ = std::fs::remove_dir_all(&staging);
        result
    }

    fn install_checked(
        &self,
        src: &Path,
        source: String,
        commit: Option<String>,
        expected_sum: Option<&str>,
    ) -> Result<Installed> {
        let manifest = Manifest::load_dir(src)?;
        check_data_only(src)?;
        let sum = content_sum(src)?;
        if let Some(want) = expected_sum
            && want != sum
        {
            return Err(Error::invalid(format!(
                "{}: content hash {sum} does not match the reviewed {want}; refusing it",
                manifest.plugin.id
            )));
        }
        let id = manifest.plugin.id.clone();
        let mut lock = self.read_lock()?;
        if let Some(game) = &manifest.game
            && let Some(other) = lock.plugins.iter().find(|l| l.id != id && self.provides(&l.id, &game.id))
        {
            return Err(Error::invalid(format!(
                "game {:?} is already provided by {}; remove that plugin first",
                game.id, other.id
            )));
        }
        let dest = self.dir_for(&id);
        if dest.exists() {
            std::fs::remove_dir_all(&dest).ctx(|| format!("replacing {}", dest.display()))?;
        }
        copy_plugin(src, &dest)?;
        let locked = Locked { id: id.clone(), version: manifest.plugin.version.clone(), source, commit, sum };
        lock.plugins.retain(|l| l.id != id);
        lock.plugins.push(locked.clone());
        lock.plugins.sort_by(|a, b| a.id.cmp(&b.id));
        self.write_lock(&lock)?;
        Ok(Installed { manifest, dir: dest, lock: locked })
    }

    fn provides(&self, id: &str, game_id: &str) -> bool {
        Manifest::load_dir(&self.dir_for(id)).is_ok_and(|m| m.game.is_some_and(|g| g.id == game_id))
    }

    /// Uninstalls a plugin. Game data (mods, profiles) is kept: re-adding the
    /// plugin brings the game back exactly as it was.
    pub fn remove(&self, id: &str) -> Result<bool> {
        let mut lock = self.read_lock()?;
        let before = lock.plugins.len();
        lock.plugins.retain(|l| l.id != id);
        if lock.plugins.len() == before {
            return Ok(false);
        }
        let dir = self.dir_for(id);
        if dir.exists() {
            std::fs::remove_dir_all(&dir).ctx(|| format!("removing {}", dir.display()))?;
        }
        self.write_lock(&lock)?;
        Ok(true)
    }

    /// Re-hashes every installed plugin; returns the ids whose files changed.
    pub fn verify(&self) -> Result<Vec<String>> {
        let mut changed = Vec::new();
        for p in self.list()? {
            if content_sum(&p.dir)? != p.lock.sum {
                changed.push(p.lock.id);
            }
        }
        Ok(changed)
    }
}

/// Rejects programs, scripts and oversized plugins.
pub fn check_data_only(dir: &Path) -> Result<()> {
    let files = fsx::walk_files(dir).ctx(|| format!("reading {}", dir.display()))?;
    let mut bad = Vec::new();
    let mut total = 0;
    for f in files.iter().filter(|f| !f.starts_with(".git/")) {
        let name = f.rsplit('/').next().unwrap_or(f).to_ascii_lowercase();
        let ok = match name.rsplit_once('.') {
            Some((_, ext)) => ALLOWED_EXTENSIONS.contains(&ext),
            None => ALLOWED_NAMES.contains(&name.as_str()),
        };
        if !ok && !name.starts_with('.') {
            bad.push(f.clone());
        }
        total += std::fs::metadata(dir.join(f)).map_or(0, |m| m.len());
    }
    if !bad.is_empty() {
        return Err(Error::invalid(format!(
            "plugins may only contain data files ({}); found: {}",
            ALLOWED_EXTENSIONS.join(", "),
            bad.join(", ")
        )));
    }
    if total > MAX_PLUGIN_BYTES {
        return Err(Error::invalid(format!("plugin is {total} bytes; the limit is {MAX_PLUGIN_BYTES}")));
    }
    Ok(())
}

/// `sha256:` over every file (path + content hash), in path order. Hidden
/// files and `.git` are ignored, so a checkout and a tarball hash the same.
pub fn content_sum(dir: &Path) -> Result<String> {
    let files = fsx::walk_files(dir).ctx(|| format!("reading {}", dir.display()))?;
    let mut h = Sha256::new();
    for f in files.iter().filter(|f| !f.split('/').any(|c| c.starts_with('.'))) {
        let bytes = std::fs::read(dir.join(f)).ctx(|| format!("reading {f}"))?;
        // Normalize line endings: git may check text out as CRLF.
        let bytes = if is_text(f) { normalize_newlines(&bytes) } else { bytes };
        h.update(f.as_bytes());
        h.update([0]);
        h.update(Sha256::digest(&bytes));
    }
    Ok(format!("sha256:{}", hex(&h.finalize())))
}

fn is_text(f: &str) -> bool {
    let ext = f.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase());
    matches!(ext.as_deref(), Some("toml" | "md" | "txt" | "json" | "svg") | None)
}

fn normalize_newlines(b: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'\r' && b.get(i + 1) == Some(&b'\n') {
            i += 1;
            continue;
        }
        out.push(b[i]);
        i += 1;
    }
    out
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes.iter().fold(String::new(), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

fn copy_plugin(src: &Path, dst: &Path) -> Result<()> {
    for f in fsx::walk_files(src).ctx(|| format!("reading {}", src.display()))? {
        if f.split('/').any(|c| c.starts_with('.')) {
            continue;
        }
        let to = crate::paths::join_rel(dst, &f);
        if let Some(p) = to.parent() {
            std::fs::create_dir_all(p).ctx(|| format!("creating {}", p.display()))?;
        }
        std::fs::copy(crate::paths::join_rel(src, &f), &to).ctx(|| format!("copying {f}"))?;
    }
    Ok(())
}

pub(crate) fn tempdir(near: &Path) -> Result<PathBuf> {
    std::fs::create_dir_all(near).ctx(|| format!("creating {}", near.display()))?;
    let n = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos();
    let d = near.join(format!(".tmp-{}-{n}", std::process::id()));
    std::fs::create_dir_all(&d).ctx(|| format!("creating {}", d.display()))?;
    Ok(d)
}

// ---------------------------------------------------------------- remote

/// A plugin hosted on GitHub: `github.com/<owner>/<repo>[/<subdir>]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Remote {
    pub owner: String,
    pub repo: String,
    /// `""` for a repository-root plugin.
    pub subdir: String,
}

impl Remote {
    pub fn parse(import_path: &str) -> Result<Self> {
        let parts: Vec<&str> = import_path.split('/').collect();
        match parts.as_slice() {
            ["github.com", owner, repo, rest @ ..] => {
                Ok(Self { owner: (*owner).into(), repo: (*repo).into(), subdir: rest.join("/") })
            }
            [host, ..] => {
                Err(Error::invalid(format!("{import_path}: host {host:?} is not supported yet (only github.com)")))
            }
            [] => Err(Error::invalid("empty import path")),
        }
    }

    fn api(&self, tail: &str) -> String {
        format!("https://api.github.com/repos/{}/{}/{tail}", self.owner, self.repo)
    }

    /// Highest semver tag for this module (`v1.2.0`, or `<subdir>/v1.2.0`).
    pub fn latest_tag(&self) -> Result<String> {
        #[derive(Deserialize)]
        struct Tag {
            name: String,
        }
        let body = http_get(&self.api("tags?per_page=100"))?;
        let tags: Vec<Tag> = serde_json::from_slice(&body).map_err(|e| Error::Network(format!("tag list: {e}")))?;
        let prefix = if self.subdir.is_empty() { "v".to_owned() } else { format!("{}/v", self.subdir) };
        tags.into_iter()
            .filter_map(|t| {
                let ver = t.name.strip_prefix(&prefix)?.to_owned();
                loom_plugin::is_semver(&ver).then(|| (semver_key(&ver), t.name))
            })
            .max()
            .map(|(_, name)| name)
            .ok_or_else(|| Error::invalid(format!("no {prefix}X.Y.Z tags in {}/{}", self.owner, self.repo)))
    }

    /// Downloads `reference` and extracts the plugin folder into `dest`.
    /// Returns the resolved commit (short sha from the tarball's top folder).
    pub fn download(&self, reference: &str, dest: &Path) -> Result<String> {
        let body = http_get(&self.api(&format!("tarball/{reference}")))?;
        let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(&body[..]));
        let mut commit = None;
        let mut found = false;
        let want = if self.subdir.is_empty() { String::new() } else { format!("{}/", self.subdir) };
        for entry in archive.entries().ctx(|| "reading the plugin tarball".into())? {
            let mut entry = entry.ctx(|| "reading the plugin tarball".into())?;
            let path = entry.path().ctx(|| "reading the plugin tarball".into())?.to_string_lossy().replace('\\', "/");
            let Some((top, rest)) = path.split_once('/') else {
                continue;
            };
            commit.get_or_insert_with(|| top.rsplit('-').next().unwrap_or(top).to_owned());
            let Some(rel) = rest.strip_prefix(&want) else {
                continue;
            };
            if rel.is_empty() || !entry.header().entry_type().is_file() {
                continue;
            }
            if rel.split('/').any(|c| c == ".." || c.is_empty()) {
                return Err(Error::invalid(format!("tarball entry {path:?} escapes the plugin folder")));
            }
            let out = crate::paths::join_rel(dest, rel);
            if let Some(p) = out.parent() {
                std::fs::create_dir_all(p).ctx(|| format!("creating {}", p.display()))?;
            }
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes).ctx(|| format!("extracting {rel}"))?;
            std::fs::write(&out, bytes).ctx(|| format!("writing {}", out.display()))?;
            found = true;
        }
        if !found {
            return Err(Error::invalid(format!(
                "{}/{}@{reference} has no files under {:?}",
                self.owner, self.repo, self.subdir
            )));
        }
        Ok(commit.unwrap_or_default())
    }
}

fn semver_key(v: &str) -> (u64, u64, u64, bool) {
    let (core, pre) = v.split_once('-').map_or((v, false), |(c, _)| (c, true));
    let n: Vec<u64> = core.split('.').map(|x| x.parse().unwrap_or(0)).collect();
    (n[0], n[1], n[2], !pre)
}

/// GET with a User-Agent (GitHub requires one) and an optional token from
/// `GITHUB_TOKEN` (raises the anonymous rate limit).
pub(crate) fn http_get(url: &str) -> Result<Vec<u8>> {
    let mut req = ureq::get(url).header("User-Agent", concat!("loom/", env!("CARGO_PKG_VERSION")));
    if let Ok(token) = std::env::var("GITHUB_TOKEN")
        && url.starts_with("https://api.github.com/")
    {
        req = req.header("Authorization", &format!("Bearer {token}"));
    }
    let mut resp = req.call().map_err(|e| Error::Network(format!("{url}: {e}")))?;
    let mut body = Vec::new();
    resp.body_mut().as_reader().read_to_end(&mut body).map_err(|e| Error::Network(format!("{url}: {e}")))?;
    Ok(body)
}

/// Collected manifests keyed by id (for callers that need lookups).
pub fn by_id(list: &[Installed]) -> BTreeMap<&str, &Installed> {
    list.iter().map(|p| (p.lock.id.as_str(), p)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const MANIFEST: &str = r#"
[plugin]
id = "github.com/example/loom-testgame"
name = "Test Game"
version = "0.1.0"
kind = "game"

[game]
id = "testgame"
name = "Test Game"
executables = [{ id = "game", name = "Game", path = "game.exe", default = true }]

[game.detect]
folder_names = ["Test Game"]

[game.mods]
data_dir = "Data"
data_markers = ["*.pak"]
"#;

    fn plugin_dir(root: &Path) -> PathBuf {
        let d = root.join("src");
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join(loom_plugin::MANIFEST_FILE), MANIFEST).unwrap();
        std::fs::write(d.join("README.md"), "hi").unwrap();
        d
    }

    #[test]
    fn install_list_remove() {
        let t = tempfile::tempdir().unwrap();
        let store = Store::new(&t.path().join("home"));
        let src = plugin_dir(t.path());
        let p = store.install_dir(&src).unwrap();
        assert_eq!(p.lock.id, "github.com/example/loom-testgame");
        assert!(p.lock.sum.starts_with("sha256:"));
        assert_eq!(store.list().unwrap().len(), 1);
        assert_eq!(store.for_game("testgame").unwrap().lock.id, p.lock.id);
        assert_eq!(store.verify().unwrap(), [] as [std::string::String; 0]);
        std::fs::write(p.dir.join("README.md"), "tampered").unwrap();
        assert_eq!(store.verify().unwrap(), vec![p.lock.id.clone()]);
        assert!(store.remove(&p.lock.id).unwrap());
        assert!(store.list().unwrap().is_empty());
    }

    #[test]
    fn programs_are_refused() {
        let t = tempfile::tempdir().unwrap();
        let store = Store::new(&t.path().join("home"));
        let src = plugin_dir(t.path());
        std::fs::write(src.join("helper.ps1"), "evil").unwrap();
        let err = store.install_dir(&src).unwrap_err();
        assert!(err.to_string().contains("helper.ps1"), "{err}");
    }

    #[test]
    fn pinned_sum_must_match() {
        let t = tempfile::tempdir().unwrap();
        let store = Store::new(&t.path().join("home"));
        let src = plugin_dir(t.path());
        let err = store.install_checked(&src, "x".into(), None, Some("sha256:00")).unwrap_err();
        assert!(err.to_string().contains("does not match"), "{err}");
    }

    #[test]
    fn crlf_does_not_change_the_sum() {
        let t = tempfile::tempdir().unwrap();
        let a = t.path().join("a");
        let b = t.path().join("b");
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        std::fs::write(a.join("x.toml"), "a = 1\nb = 2\n").unwrap();
        std::fs::write(b.join("x.toml"), "a = 1\r\nb = 2\r\n").unwrap();
        assert_eq!(content_sum(&a).unwrap(), content_sum(&b).unwrap());
    }

    #[test]
    fn remotes() {
        let r = Remote::parse("github.com/enrell/loom/plugins/skyrimse").unwrap();
        assert_eq!((r.owner.as_str(), r.repo.as_str(), r.subdir.as_str()), ("enrell", "loom", "plugins/skyrimse"));
        assert!(Remote::parse("gitlab.com/a/b").is_err());
        assert!(semver_key("1.10.0") > semver_key("1.9.9"));
        assert!(semver_key("1.0.0") > semver_key("1.0.0-beta"));
    }
}
