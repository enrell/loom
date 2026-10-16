//! `loom doctor`: everything wrong with a game setup, each problem with a
//! machine-readable code and — when there is one — the exact command that
//! fixes it. This is what an agent reads to repair a setup.

use std::collections::HashSet;

use serde::Serialize;

use crate::library::Game;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
    Info,
}

#[derive(Debug, Clone, Serialize)]
pub struct Problem {
    pub severity: Severity,
    /// Stable identifier (`missing-master`, `master-order`, …).
    pub code: &'static str,
    pub message: String,
    /// What it is about (a mod, a plugin file, a path).
    pub subject: String,
    /// A `loom` command that fixes it, when one exists.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fix: Option<String>,
}

fn p(
    severity: Severity,
    code: &'static str,
    subject: impl Into<String>,
    message: impl Into<String>,
    fix: Option<String>,
) -> Problem {
    Problem { severity, code, message: message.into(), subject: subject.into(), fix }
}

impl Game {
    /// Runs every check. Errors first.
    pub fn doctor(&self) -> Vec<Problem> {
        let mut out = Vec::new();
        let id = self.id().to_owned();
        let spec = self.spec();

        if !crate::detect::is_install(spec, self.root()) {
            out.push(p(
                Severity::Error,
                "game-missing",
                self.root().display().to_string(),
                format!("{} is not at {} anymore", spec.name, self.root().display()),
                Some(format!("loom game set-path {id} <folder>")),
            ));
        }
        if let Ok(changed) = self.loom.plugins.verify()
            && changed.contains(&self.plugin.lock.id)
        {
            out.push(p(
                Severity::Warning,
                "plugin-modified",
                &self.plugin.lock.id,
                "the game plugin's files changed since it was installed",
                Some(format!("loom plugin add {}", self.plugin.lock.source.trim_start_matches("path:"))),
            ));
        }
        let vfs_ok = crate::launch::mount_available()
            || crate::launch::openvfs_binaries(self.loom.config.openvfs_dir.as_deref()).is_ok();
        if !vfs_ok {
            out.push(p(
                Severity::Error,
                "no-vfs",
                "openvfs",
                "games can't be started: neither WinFsp nor the openvfs hook binaries are available",
                Some("winget install WinFsp.WinFsp".into()),
            ));
        }

        match self.profile() {
            Err(e) => out.push(p(Severity::Error, "profile-broken", self.active_profile(), e.to_string(), None)),
            Ok(profile) => {
                for m in &profile.mods {
                    let dir = self.mods_dir().join(&m.name);
                    if !dir.is_dir() {
                        out.push(p(
                            Severity::Error,
                            "mod-missing",
                            &m.name,
                            format!("mod {:?} is in the profile but its folder is gone", m.name),
                            Some(format!("loom mod remove {id} \"{}\"", m.name)),
                        ));
                    } else if m.enabled && crate::fsx::walk_files(&dir).map_or(true, |f| f.is_empty()) {
                        out.push(p(
                            Severity::Warning,
                            "mod-empty",
                            &m.name,
                            format!("mod {:?} has no files", m.name),
                            None,
                        ));
                    }
                }
            }
        }

        if self.has_load_order() {
            self.check_load_order(&id, &mut out);
        }

        let ow = crate::fsx::walk_files(&self.overwrite_dir()).map_or(0, |f| f.len());
        if ow > 0 {
            out.push(p(
                Severity::Info,
                "overwrite-not-empty",
                "overwrite",
                format!("{ow} file(s) written by the game or tools are in the overwrite folder"),
                Some(format!("loom overwrite to-mod {id} \"<new mod name>\"")),
            ));
        }
        if let Ok(c) = self.conflicts()
            && !c.is_empty()
        {
            out.push(p(
                Severity::Info,
                "file-conflicts",
                "mods",
                format!("{} file(s) are provided by more than one enabled mod; the higher mod wins", c.len()),
                Some(format!("loom mod conflicts {id}")),
            ));
        }
        out.sort_by_key(|x| x.severity);
        out
    }

    fn check_load_order(&self, id: &str, out: &mut Vec<Problem>) {
        let Ok(order) = self.load_order() else { return };
        let spec = self.spec().load_order.as_ref().expect("checked");
        let position: std::collections::HashMap<String, usize> =
            order.iter().enumerate().map(|(i, e)| (e.name.to_lowercase(), i)).collect();
        let mut reported_order = HashSet::new();
        for (i, e) in order.iter().enumerate().filter(|(_, e)| e.active) {
            for m in &e.masters {
                match position.get(&m.to_lowercase()) {
                    None => out.push(p(
                        Severity::Error,
                        "missing-master",
                        &e.name,
                        format!("{} needs {m}, which no enabled mod or the game provides", e.name),
                        Some(format!("loom order disable {id} \"{}\"", e.name)),
                    )),
                    Some(&j) if !order[j].active => out.push(p(
                        Severity::Error,
                        "inactive-master",
                        &e.name,
                        format!("{} needs {m}, which is disabled", e.name),
                        Some(format!("loom order enable {id} \"{m}\"")),
                    )),
                    Some(&j) if j > i && reported_order.insert(e.name.clone()) => out.push(p(
                        Severity::Error,
                        "master-order",
                        &e.name,
                        format!("{} loads before its master {m}", e.name),
                        Some(format!("loom order sort {id}")),
                    )),
                    _ => {}
                }
            }
        }
        let full = order.iter().filter(|e| e.active && !e.is_light).count();
        if full > spec.max_plugins as usize {
            out.push(p(
                Severity::Error,
                "too-many-plugins",
                "load order",
                format!("{full} full plugins are active; the game loads at most {}", spec.max_plugins),
                None,
            ));
        }
    }
}
