//! Nexus Mods: `nxm://` links and downloads through the public API with the
//! user's personal API key.
//!
//! Flow: the website's "Mod Manager Download" button yields
//! `nxm://<domain>/mods/<mod>/files/<file>?key=…&expires=…`; Loom asks the API
//! for a download URL (the key/expires pair authorizes non-premium users),
//! downloads the archive into the game's `downloads/` and installs it.

use std::io::{Read, Write};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error::{Context, Error, Result};
use crate::library::{Game, Loom};
use crate::mods::{InstallOptions, ModMeta, NexusRef};

const API: &str = "https://api.nexusmods.com/v1";

/// A parsed `nxm://` link.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NxmLink {
    pub domain: String,
    pub mod_id: u64,
    pub file_id: u64,
    pub key: Option<String>,
    pub expires: Option<u64>,
}

impl NxmLink {
    pub fn parse(url: &str) -> Result<Self> {
        let bad = || Error::invalid(format!("{url:?} is not an nxm://<game>/mods/<id>/files/<id> link"));
        let rest = url.trim().get(6..).filter(|_| url.trim()[..6].eq_ignore_ascii_case("nxm://")).ok_or_else(bad)?;
        let (path, query) = rest.split_once('?').unwrap_or((rest, ""));
        let parts: Vec<&str> = path.trim_end_matches('/').split('/').collect();
        let [domain, "mods", m, "files", f] = parts.as_slice() else {
            return Err(bad());
        };
        let mut link = NxmLink {
            domain: domain.to_ascii_lowercase(),
            mod_id: m.parse().map_err(|_| bad())?,
            file_id: f.parse().map_err(|_| bad())?,
            key: None,
            expires: None,
        };
        for kv in query.split('&') {
            match kv.split_once('=') {
                Some(("key", v)) => link.key = Some(v.to_owned()),
                Some(("expires", v)) => link.expires = v.parse().ok(),
                _ => {}
            }
        }
        Ok(link)
    }
}

#[derive(Debug, Clone, Deserialize)]
struct DownloadLink {
    #[serde(rename = "URI")]
    uri: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FileInfo {
    pub file_name: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub version: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ModInfo {
    pub name: String,
    #[serde(default)]
    pub version: Option<String>,
}

/// Result of [`Loom::handle_nxm`].
#[derive(Debug, Clone, Serialize)]
pub struct Downloaded {
    pub game: String,
    pub archive: PathBuf,
    pub installed: Option<crate::mods::Installed>,
}

impl Loom {
    fn nexus_key(&self) -> Result<String> {
        self.config.nexus_api_key.clone().or_else(|| std::env::var("NEXUS_API_KEY").ok()).ok_or_else(|| {
            Error::invalid(
                "no Nexus API key: run `loom config set nexus_api_key <key>` (nexusmods.com → Site preferences → API)",
            )
        })
    }

    fn api_get(&self, url: &str) -> Result<Vec<u8>> {
        let key = self.nexus_key()?;
        let mut resp = ureq::get(url)
            .header("apikey", &key)
            .header("Application-Name", "Loom")
            .header("Application-Version", env!("CARGO_PKG_VERSION"))
            .header("User-Agent", concat!("Loom/", env!("CARGO_PKG_VERSION")))
            .call()
            .map_err(|e| Error::Network(format!("Nexus API: {e}")))?;
        let mut body = Vec::new();
        resp.body_mut().as_reader().read_to_end(&mut body).map_err(|e| Error::Network(e.to_string()))?;
        Ok(body)
    }

    /// The set-up game whose plugin declares this Nexus domain.
    pub fn game_for_domain(&self, domain: &str) -> Result<Game> {
        for id in self.game_ids()? {
            if let Ok(g) = self.game(&id)
                && g.spec().nexus_domain.as_deref() == Some(domain)
            {
                return Ok(g);
            }
        }
        Err(Error::invalid(format!("no set-up game uses the Nexus domain {domain:?}; add its plugin and game first")))
    }

    /// Downloads an `nxm://` link and installs it (unless `install` is false).
    pub fn handle_nxm(
        &self,
        url: &str,
        install: bool,
        progress: &mut dyn FnMut(u64, Option<u64>),
    ) -> Result<Downloaded> {
        let link = NxmLink::parse(url)?;
        let game = self.game_for_domain(&link.domain)?;
        let base = format!("{API}/games/{}/mods/{}", link.domain, link.mod_id);
        let file: FileInfo = serde_json::from_slice(&self.api_get(&format!("{base}/files/{}.json", link.file_id))?)
            .map_err(|e| Error::Network(format!("file info: {e}")))?;
        let info: Option<ModInfo> =
            self.api_get(&format!("{base}.json")).ok().and_then(|b| serde_json::from_slice(&b).ok());
        let mut url = format!("{base}/files/{}/download_link.json", link.file_id);
        if let (Some(k), Some(e)) = (&link.key, link.expires) {
            use std::fmt::Write as _;
            let _ = write!(url, "?key={k}&expires={e}");
        }
        let links: Vec<DownloadLink> =
            serde_json::from_slice(&self.api_get(&url)?).map_err(|e| Error::Network(format!("download link: {e}")))?;
        let uri =
            links.first().ok_or_else(|| Error::Network("Nexus returned no download location".into()))?.uri.clone();
        let file_name = safe_file_name(&file.file_name);
        let archive = game.downloads_dir().join(&file_name);
        download(&uri, &archive, progress)?;
        let installed = if install {
            let name = info.as_ref().map(|i| i.name.clone()).filter(|n| crate::paths::is_safe_name(n));
            let opts = InstallOptions {
                name,
                replace: true,
                meta: ModMeta {
                    version: file.version.clone().or(info.and_then(|i| i.version)),
                    nexus: Some(NexusRef {
                        game: link.domain.clone(),
                        mod_id: link.mod_id,
                        file_id: Some(link.file_id),
                    }),
                    ..ModMeta::default()
                },
            };
            Some(game.install_mod(&archive, &opts)?)
        } else {
            None
        };
        Ok(Downloaded { game: game.id().to_owned(), archive, installed })
    }
}

fn safe_file_name(name: &str) -> String {
    let n: String = name.chars().map(|c| if r#"<>:"/\|?*"#.contains(c) || c.is_control() { '_' } else { c }).collect();
    if n.trim().is_empty() { "download.bin".into() } else { n }
}

/// Streams `url` to `dest` (via a `.part` file), reporting bytes done/total.
pub fn download(url: &str, dest: &std::path::Path, progress: &mut dyn FnMut(u64, Option<u64>)) -> Result<()> {
    let mut resp = ureq::get(url)
        .header("User-Agent", concat!("Loom/", env!("CARGO_PKG_VERSION")))
        .call()
        .map_err(|e| Error::Network(format!("download: {e}")))?;
    let total = resp.body().content_length();
    if let Some(p) = dest.parent() {
        std::fs::create_dir_all(p).ctx(|| format!("creating {}", p.display()))?;
    }
    let part = dest.with_extension("part");
    let mut out = std::fs::File::create(&part).ctx(|| format!("creating {}", part.display()))?;
    let mut reader = resp.body_mut().as_reader();
    let mut buf = vec![0u8; 256 * 1024];
    let mut done = 0u64;
    loop {
        let n = reader.read(&mut buf).map_err(|e| Error::Network(format!("download: {e}")))?;
        if n == 0 {
            break;
        }
        out.write_all(&buf[..n]).ctx(|| format!("writing {}", part.display()))?;
        done += n as u64;
        progress(done, total);
    }
    drop(out);
    std::fs::rename(&part, dest).ctx(|| format!("saving {}", dest.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_links() {
        let l = NxmLink::parse("nxm://SkyrimSpecialEdition/mods/266/files/123456?key=abc&expires=1700000000&user_id=1")
            .unwrap();
        assert_eq!(l.domain, "skyrimspecialedition");
        assert_eq!((l.mod_id, l.file_id), (266, 123_456));
        assert_eq!(l.key.as_deref(), Some("abc"));
        assert_eq!(l.expires, Some(1_700_000_000));
        assert!(NxmLink::parse("https://nexusmods.com").is_err());
        assert!(NxmLink::parse("nxm://x/mods/a/files/1").is_err());
    }
}
