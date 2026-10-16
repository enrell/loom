//! Companion tools (translators, xEdit-style editors, patchers) run inside
//! the profile's virtual view, writing into a **generated mod**.
//!
//! A tool sees exactly the modded game, without anything being deployed;
//! whatever it writes becomes `Generated - <tool>`, an ordinary mod that can
//! be reordered, disabled or deleted. `{game}` in a tool's arguments is the
//! game folder *as the tool sees it* (the mount point, or the hooked folder).

use std::path::{Path, PathBuf};

use loom_plugin::{ToolOutput, ToolSpec};
use serde::Serialize;

use crate::error::{Error, Result};
use crate::launch::{Mode, Target};
use crate::library::Game;
use crate::mods::{ModMeta, now};

#[derive(Debug, Clone, Serialize)]
pub struct ToolInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub homepage: Option<String>,
    /// Where the executable was found, if it was.
    pub found: Option<PathBuf>,
    pub output: ToolOutput,
    pub agent_notes: String,
    /// The plugin that declares it.
    pub plugin: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolReport {
    pub tool: String,
    pub run: crate::launch::LaunchReport,
    /// The mod that received the tool's output (generated-mod tools).
    pub output_mod: Option<String>,
}

impl Game {
    /// Tools for this game: its own plugin's plus any `tools` plugin's.
    pub fn tools(&self) -> Result<Vec<ToolInfo>> {
        let mut out = Vec::new();
        for p in self.loom.plugins.list()? {
            let own = p.lock.id == self.plugin.lock.id;
            for t in &p.manifest.tools {
                let applies = if t.games.is_empty() {
                    own || p.manifest.game.is_none()
                } else {
                    t.games.iter().any(|g| g == self.id())
                };
                if applies {
                    out.push(ToolInfo {
                        id: t.id.clone(),
                        name: t.name.clone(),
                        description: t.description.clone(),
                        homepage: t.homepage.clone(),
                        found: self.find_tool_exe(t),
                        output: t.output,
                        agent_notes: t.agent_notes.clone(),
                        plugin: p.lock.id.clone(),
                    });
                }
            }
        }
        Ok(out)
    }

    fn tool_spec(&self, id: &str) -> Result<ToolSpec> {
        for p in self.loom.plugins.list()? {
            if let Some(t) = p.manifest.tools.iter().find(|t| t.id == id) {
                return Ok(t.clone());
            }
        }
        Err(Error::invalid(format!("no installed plugin declares a tool {id:?}")))
    }

    /// Looks for the tool in the configured tool folders, the plugin's search
    /// folders, the game folder and the enabled mods.
    pub fn find_tool_exe(&self, t: &ToolSpec) -> Option<PathBuf> {
        let mut roots: Vec<PathBuf> = self.loom.config.tool_paths.clone();
        roots.extend(t.search.iter().filter_map(|s| crate::paths::expand(s, Some(self.root())).ok()));
        roots.push(self.root().to_path_buf());
        if let Ok(p) = self.profile() {
            roots.extend(p.enabled().map(|m| self.mods_dir().join(m)));
        }
        roots.iter().find_map(|r| search(r, &t.exe, 3))
    }

    /// Runs a tool in the virtual view and waits for it.
    pub fn run_tool(&self, id: &str, extra_args: &[String], mode: Option<Mode>) -> Result<ToolReport> {
        let t = self.tool_spec(id)?;
        let exe = self.find_tool_exe(&t).ok_or_else(|| {
            Error::invalid(format!(
                "{} was not found{}; add its folder to tool_paths in config.toml",
                t.name,
                t.homepage.as_deref().map(|h| format!(" (get it from {h})")).unwrap_or_default()
            ))
        })?;
        let (target, output_mod) = match t.output {
            ToolOutput::Overwrite => (Target { upper: self.overwrite_dir(), exclude: None }, None),
            ToolOutput::GeneratedMod => {
                let name = generated_name(&t.name);
                self.ensure_generated_mod(&name, &t.id)?;
                (Target { upper: self.mods_dir().join(&name), exclude: Some(name.clone()) }, Some(name))
            }
        };
        let mut args = t.args.clone();
        args.extend(extra_args.iter().cloned());
        let cwd = exe.parent().map(Path::to_path_buf);
        let run =
            self.run_in_view(&exe.display().to_string(), &args, cwd.as_deref(), &target, self.launch_mode(mode))?;
        Ok(ToolReport { tool: t.id, run, output_mod })
    }

    /// Creates the generated mod (at the top of the profile, enabled) if needed.
    fn ensure_generated_mod(&self, name: &str, tool: &str) -> Result<()> {
        let dir = self.mods_dir().join(name);
        if !dir.is_dir() {
            std::fs::create_dir_all(&dir)
                .map_err(|e| Error::Io { context: format!("creating {}", dir.display()), source: e })?;
            let meta = ModMeta { generated_by: Some(tool.into()), installed: now(), ..ModMeta::default() };
            self.set_mod_meta(name, &meta)?;
            let active = self.active_profile().to_owned();
            self.for_each_profile(|p, profile| profile.insert(name, p == active, 0))?;
        }
        Ok(())
    }

    /// Moves everything in the overwrite folder into a (new or existing) mod.
    pub fn overwrite_to_mod(&self, name: &str) -> Result<usize> {
        if !crate::paths::is_safe_name(name) {
            return Err(Error::invalid(format!("{name:?} is not a valid mod name")));
        }
        let ow = self.overwrite_dir();
        let files = crate::fsx::walk_files(&ow).unwrap_or_default();
        if files.is_empty() {
            return Ok(0);
        }
        let dest = self.mods_dir().join(name);
        for f in &files {
            let to = crate::paths::join_rel(&dest, f);
            if let Some(p) = to.parent() {
                std::fs::create_dir_all(p)
                    .map_err(|e| Error::Io { context: format!("creating {}", p.display()), source: e })?;
            }
            std::fs::rename(crate::paths::join_rel(&ow, f), &to)
                .map_err(|e| Error::Io { context: format!("moving {f}"), source: e })?;
        }
        let _ = std::fs::remove_dir_all(&ow);
        if self.mod_meta(name).map_or(true, |m| m.installed == 0) {
            self.set_mod_meta(
                name,
                &ModMeta { source: Some("overwrite".into()), installed: now(), ..ModMeta::default() },
            )?;
        }
        let active = self.active_profile().to_owned();
        self.for_each_profile(|p, profile| profile.insert(name, p == active, 0))?;
        Ok(files.len())
    }
}

/// `Generated - <tool name>`, made folder-safe.
pub fn generated_name(tool: &str) -> String {
    let clean: String =
        tool.chars().map(|c| if r#"<>:"/\|?*"#.contains(c) || c.is_control() { '_' } else { c }).collect();
    format!("Generated - {}", clean.trim())
}

fn search(dir: &Path, names: &[String], depth: usize) -> Option<PathBuf> {
    let rd = std::fs::read_dir(dir).ok()?;
    let mut subdirs = Vec::new();
    for e in rd.flatten() {
        let n = e.file_name().to_string_lossy().into_owned();
        let p = e.path();
        if p.is_file() && names.iter().any(|x| x.eq_ignore_ascii_case(&n)) {
            return Some(p);
        }
        if p.is_dir() {
            subdirs.push(p);
        }
    }
    if depth == 0 {
        return None;
    }
    subdirs.into_iter().find_map(|d| search(&d, names, depth - 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        assert_eq!(generated_name("xTranslator"), "Generated - xTranslator");
        assert_eq!(generated_name("SSE: AT"), "Generated - SSE_ AT");
    }
}
