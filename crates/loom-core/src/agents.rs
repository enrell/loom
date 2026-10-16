//! AI coding agents: detection, and registering Loom's MCP server and skill
//! in their configuration.
//!
//! Each agent keeps its MCP servers in its own file and format; Loom merges
//! its entry in (keeping everything else, backing the file up once as
//! `<file>.loom-backup`) and never rewrites a file it can't parse. Agents
//! that read [Agent Skills](https://agentskills.io) also get the Loom skill
//! (`SKILL.md`), which teaches the repair loop and the conventions.
//!
//! Paths come from an [`AgentEnv`], so tests run against a fake home.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Serialize;
use serde_json::{Map, Value, json};

use crate::error::{Context, Error, Result};
use crate::fsx;

/// The Loom skill, shared by every agent that reads skills.
pub const SKILL: &str = include_str!("../assets/SKILL.md");

/// MCP server name in agents' configs.
pub const SERVER: &str = "loom";

/// Where agents keep their files.
#[derive(Debug, Clone)]
pub struct AgentEnv {
    /// The user's home folder (`~`).
    pub home: PathBuf,
    /// `%APPDATA%` (roaming).
    pub appdata: PathBuf,
    /// Folders searched for agent executables (`PATH`).
    pub path: Vec<PathBuf>,
    /// Use an agent's own CLI when it has one (`claude mcp add`); off in tests.
    pub use_cli: bool,
}

impl AgentEnv {
    /// This machine.
    pub fn system() -> Self {
        Self {
            home: dirs::home_dir().unwrap_or_default(),
            appdata: dirs::data_dir().unwrap_or_default(),
            path: std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect()).unwrap_or_default(),
            use_cli: true,
        }
    }

    /// An isolated environment rooted at `root` (tests, previews).
    pub fn sandbox(root: &Path) -> Self {
        Self { home: root.join("home"), appdata: root.join("appdata"), path: vec![root.join("bin")], use_cli: false }
    }

    /// An executable named `name` on `path` (`.exe`, `.cmd`, `.bat` or bare).
    fn which(&self, name: &str) -> Option<PathBuf> {
        self.path.iter().find_map(|d| {
            ["exe", "cmd", "bat", ""]
                .iter()
                .map(|ext| if ext.is_empty() { d.join(name) } else { d.join(format!("{name}.{ext}")) })
                .find(|p| p.is_file())
        })
    }
}

/// How Loom's MCP server is started: `command args…` (+ env).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct McpCommand {
    pub command: PathBuf,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
}

impl McpCommand {
    /// `<exe> mcp`, passing `LOOM_HOME` along when the library isn't the default.
    pub fn for_exe(exe: PathBuf, home: &Path) -> Self {
        let env = if home == crate::paths::default_home() && std::env::var_os("LOOM_HOME").is_none() {
            Vec::new()
        } else {
            vec![("LOOM_HOME".into(), home.display().to_string())]
        };
        Self { command: exe, args: vec!["mcp".into()], env }
    }

    fn env_json(&self) -> Option<Value> {
        (!self.env.is_empty()).then(|| Value::Object(self.env.iter().map(|(k, v)| (k.clone(), json!(v))).collect()))
    }
}

/// The config format an agent uses for MCP servers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Format {
    /// `{ "mcpServers": { name: { command, args, env } } }` (Claude, Cursor, Gemini).
    McpServers,
    /// `{ "servers": { name: { type: "stdio", command, args, env } } }` (VS Code).
    VsCode,
    /// `{ "mcp": { name: { type: "local", command: [..], environment, enabled } } }` (OpenCode).
    OpenCode,
    /// `[mcp_servers.<name>] command, args, env` (Codex).
    CodexToml,
}

struct Spec {
    id: &'static str,
    name: &'static str,
    /// Executable names that mean "installed".
    exes: &'static [&'static str],
    /// Folders (relative to home / appdata) that mean "installed".
    home_dirs: &'static [&'static str],
    appdata_dirs: &'static [&'static str],
    format: Format,
    /// Config file, relative to home (`~`) or appdata (`%APPDATA%`).
    config: Base,
    /// Skills folder, relative to home, for agents that read skills.
    skills: Option<&'static str>,
    homepage: &'static str,
    install: &'static str,
}

#[derive(Debug, Clone, Copy)]
enum Base {
    Home(&'static str),
    AppData(&'static str),
}

const AGENTS: &[Spec] = &[
    Spec {
        id: "claude-code",
        name: "Claude Code",
        exes: &["claude"],
        home_dirs: &[".claude"],
        appdata_dirs: &[],
        format: Format::McpServers,
        config: Base::Home(".claude.json"),
        skills: Some(".claude/skills"),
        homepage: "https://claude.com/claude-code",
        install: "npm install -g @anthropic-ai/claude-code",
    },
    Spec {
        id: "claude-desktop",
        name: "Claude Desktop",
        exes: &[],
        home_dirs: &[],
        appdata_dirs: &["Claude"],
        format: Format::McpServers,
        config: Base::AppData("Claude/claude_desktop_config.json"),
        skills: None,
        homepage: "https://claude.ai/download",
        install: "winget install Anthropic.Claude",
    },
    Spec {
        id: "codex",
        name: "Codex CLI",
        exes: &["codex"],
        home_dirs: &[".codex"],
        appdata_dirs: &[],
        format: Format::CodexToml,
        config: Base::Home(".codex/config.toml"),
        skills: Some(".codex/skills"),
        homepage: "https://developers.openai.com/codex/cli",
        install: "npm install -g @openai/codex",
    },
    Spec {
        id: "cursor",
        name: "Cursor",
        exes: &["cursor"],
        home_dirs: &[".cursor"],
        appdata_dirs: &["Cursor"],
        format: Format::McpServers,
        config: Base::Home(".cursor/mcp.json"),
        skills: None,
        homepage: "https://cursor.com/download",
        install: "winget install Anysphere.Cursor",
    },
    Spec {
        id: "vscode",
        name: "VS Code (Copilot)",
        exes: &["code"],
        home_dirs: &[".vscode"],
        appdata_dirs: &["Code"],
        format: Format::VsCode,
        config: Base::AppData("Code/User/mcp.json"),
        skills: None,
        homepage: "https://code.visualstudio.com",
        install: "winget install Microsoft.VisualStudioCode",
    },
    Spec {
        id: "opencode",
        name: "OpenCode",
        exes: &["opencode"],
        home_dirs: &[".config/opencode"],
        appdata_dirs: &[],
        format: Format::OpenCode,
        config: Base::Home(".config/opencode/opencode.json"),
        skills: None,
        homepage: "https://opencode.ai",
        install: "npm install -g opencode-ai",
    },
    Spec {
        id: "gemini",
        name: "Gemini CLI",
        exes: &["gemini"],
        home_dirs: &[".gemini"],
        appdata_dirs: &[],
        format: Format::McpServers,
        config: Base::Home(".gemini/settings.json"),
        skills: None,
        homepage: "https://github.com/google-gemini/gemini-cli",
        install: "npm install -g @google/gemini-cli",
    },
];

/// An agent and how Loom is wired into it.
#[derive(Debug, Clone, Serialize)]
pub struct AgentStatus {
    pub id: &'static str,
    pub name: &'static str,
    /// The agent is installed on this machine.
    pub installed: bool,
    /// Loom's MCP server is registered in the agent's config.
    pub mcp: bool,
    /// The Loom skill is installed (`None`: the agent doesn't read skills).
    pub skill: Option<bool>,
    pub config_path: PathBuf,
    pub homepage: &'static str,
    /// Command that installs the agent.
    pub install: &'static str,
}

/// What [`setup`] did for one agent.
#[derive(Debug, Clone, Serialize)]
pub struct SetupReport {
    pub id: &'static str,
    pub config_path: PathBuf,
    pub skill_path: Option<PathBuf>,
    /// The agent's own CLI did the registration.
    pub via_cli: bool,
    /// Original config saved here before Loom changed it (first time only).
    pub backup: Option<PathBuf>,
}

fn spec(id: &str) -> Result<&'static Spec> {
    AGENTS.iter().find(|a| a.id == id).ok_or_else(|| {
        Error::invalid(format!(
            "unknown agent {id:?} (one of: {})",
            AGENTS.iter().map(|a| a.id).collect::<Vec<_>>().join(", ")
        ))
    })
}

fn config_path(env: &AgentEnv, s: &Spec) -> PathBuf {
    match s.config {
        Base::Home(rel) => crate::paths::join_rel(&env.home, rel),
        Base::AppData(rel) => crate::paths::join_rel(&env.appdata, rel),
    }
}

fn skill_path(env: &AgentEnv, s: &Spec) -> Option<PathBuf> {
    s.skills.map(|rel| crate::paths::join_rel(&env.home, rel).join(SERVER).join("SKILL.md"))
}

fn installed(env: &AgentEnv, s: &Spec) -> bool {
    s.exes.iter().any(|e| env.which(e).is_some())
        || s.home_dirs.iter().any(|d| crate::paths::join_rel(&env.home, d).is_dir())
        || s.appdata_dirs.iter().any(|d| crate::paths::join_rel(&env.appdata, d).is_dir())
}

/// Every known agent, installed or not.
pub fn list(env: &AgentEnv) -> Vec<AgentStatus> {
    AGENTS
        .iter()
        .map(|s| {
            let config = config_path(env, s);
            AgentStatus {
                id: s.id,
                name: s.name,
                installed: installed(env, s),
                mcp: is_registered(&config, s.format),
                skill: skill_path(env, s).map(|p| p.is_file()),
                config_path: config,
                homepage: s.homepage,
                install: s.install,
            }
        })
        .collect()
}

/// Registers Loom's MCP server (and skill) in one agent.
pub fn setup(env: &AgentEnv, id: &str, mcp: &McpCommand) -> Result<SetupReport> {
    let s = spec(id)?;
    let config = config_path(env, s);
    let mut via_cli = false;
    let mut backup = None;
    if s.id == "claude-code"
        && env.use_cli
        && let Some(claude) = env.which("claude")
    {
        claude_cli(&claude, mcp)?;
        via_cli = true;
    }
    if !via_cli {
        backup = backup_once(&config)?;
        match s.format {
            Format::CodexToml => edit_toml(&config, |doc| set_codex(doc, mcp))?,
            f => edit_json(&config, |root| set_json(root, f, mcp))?,
        }
    }
    let skill = skill_path(env, s);
    if let Some(p) = &skill {
        fsx::write_atomic(p, SKILL.as_bytes()).ctx(|| format!("writing {}", p.display()))?;
    }
    Ok(SetupReport { id: s.id, config_path: config, skill_path: skill, via_cli, backup })
}

/// Removes Loom's MCP server and skill from one agent.
pub fn remove(env: &AgentEnv, id: &str) -> Result<()> {
    let s = spec(id)?;
    let config = config_path(env, s);
    if s.id == "claude-code"
        && env.use_cli
        && let Some(claude) = env.which("claude")
    {
        let _ = run_cli(&claude, &["mcp", "remove", SERVER, "-s", "user"]);
    }
    if config.is_file() && is_registered(&config, s.format) {
        match s.format {
            Format::CodexToml => edit_toml(&config, |doc| {
                if let Some(t) = doc.get_mut("mcp_servers").and_then(|t| t.as_table_like_mut()) {
                    t.remove(SERVER);
                }
            })?,
            f => edit_json(&config, |root| {
                if let Some(m) = root.get_mut(json_key(f)).and_then(Value::as_object_mut) {
                    m.remove(SERVER);
                }
            })?,
        }
    }
    if let Some(p) = skill_path(env, s)
        && let Some(dir) = p.parent()
        && dir.is_dir()
    {
        std::fs::remove_dir_all(dir).ctx(|| format!("removing {}", dir.display()))?;
    }
    Ok(())
}

/// Writes the Loom skill folder (`<dir>/loom/SKILL.md`) for agents Loom
/// doesn't configure itself (e.g. to upload to Claude Desktop).
pub fn export_skill(dir: &Path) -> Result<PathBuf> {
    let p = dir.join(SERVER).join("SKILL.md");
    fsx::write_atomic(&p, SKILL.as_bytes()).ctx(|| format!("writing {}", p.display()))?;
    Ok(p)
}

// ---------------------------------------------------------------- formats

fn json_key(f: Format) -> &'static str {
    match f {
        Format::McpServers => "mcpServers",
        Format::VsCode => "servers",
        Format::OpenCode => "mcp",
        Format::CodexToml => unreachable!("toml"),
    }
}

fn set_json(root: &mut Map<String, Value>, f: Format, m: &McpCommand) {
    let cmd = m.command.display().to_string();
    let mut entry = match f {
        Format::McpServers => json!({ "command": cmd, "args": m.args }),
        Format::VsCode => json!({ "type": "stdio", "command": cmd, "args": m.args }),
        Format::OpenCode => {
            let mut command = vec![json!(cmd)];
            command.extend(m.args.iter().map(|a| json!(a)));
            json!({ "type": "local", "command": command, "enabled": true })
        }
        Format::CodexToml => unreachable!("toml"),
    };
    if let Some(env) = m.env_json() {
        entry[if f == Format::OpenCode { "environment" } else { "env" }] = env;
    }
    let servers = root.entry(json_key(f)).or_insert_with(|| json!({}));
    if !servers.is_object() {
        *servers = json!({});
    }
    servers.as_object_mut().expect("object").insert(SERVER.into(), entry);
}

fn set_codex(doc: &mut toml_edit::DocumentMut, m: &McpCommand) {
    use toml_edit::{Array, Item, Table, value};
    if !doc.contains_table("mcp_servers") {
        let mut t = Table::new();
        t.set_implicit(true);
        doc["mcp_servers"] = Item::Table(t);
    }
    let mut server = Table::new();
    server["command"] = value(m.command.display().to_string());
    server["args"] = value(m.args.iter().map(String::as_str).collect::<Array>());
    if !m.env.is_empty() {
        let mut env = Table::new();
        for (k, v) in &m.env {
            env[k.as_str()] = value(v.as_str());
        }
        server["env"] = Item::Table(env);
    }
    doc["mcp_servers"][SERVER] = Item::Table(server);
}

fn is_registered(config: &Path, f: Format) -> bool {
    let Ok(text) = std::fs::read_to_string(config) else { return false };
    match f {
        Format::CodexToml => text
            .parse::<toml_edit::DocumentMut>()
            .is_ok_and(|d| d.get("mcp_servers").and_then(|t| t.get(SERVER)).is_some()),
        f => {
            serde_json::from_str::<Value>(&text).is_ok_and(|v| v.get(json_key(f)).and_then(|m| m.get(SERVER)).is_some())
        }
    }
}

/// Loads (or starts) a JSON config, applies `f`, writes it back atomically.
/// Refuses files that aren't plain JSON (comments) rather than lose content.
fn edit_json(path: &Path, f: impl FnOnce(&mut Map<String, Value>)) -> Result<()> {
    let mut root = match std::fs::read_to_string(path) {
        Ok(t) if t.trim().is_empty() => Map::new(),
        Ok(t) => match serde_json::from_str::<Value>(&t) {
            Ok(Value::Object(m)) => m,
            Ok(_) => return Err(Error::invalid(format!("{} is not a JSON object; add Loom by hand", path.display()))),
            Err(e) => {
                return Err(Error::invalid(format!(
                    "{} isn't plain JSON ({e}); Loom won't rewrite it — add the \"{SERVER}\" server by hand",
                    path.display()
                )));
            }
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Map::new(),
        Err(e) => return Err(Error::Io { context: format!("reading {}", path.display()), source: e }),
    };
    f(&mut root);
    let text = serde_json::to_string_pretty(&Value::Object(root)).expect("json") + "\n";
    fsx::write_atomic(path, text.as_bytes()).ctx(|| format!("writing {}", path.display()))
}

fn edit_toml(path: &Path, f: impl FnOnce(&mut toml_edit::DocumentMut)) -> Result<()> {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(Error::Io { context: format!("reading {}", path.display()), source: e }),
    };
    let mut doc: toml_edit::DocumentMut = text
        .parse()
        .map_err(|e| Error::invalid(format!("{} isn't valid TOML ({e}); Loom won't rewrite it", path.display())))?;
    f(&mut doc);
    fsx::write_atomic(path, doc.to_string().as_bytes()).ctx(|| format!("writing {}", path.display()))
}

/// Copies `path` to `<path>.loom-backup` the first time Loom touches it.
fn backup_once(path: &Path) -> Result<Option<PathBuf>> {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".loom-backup");
    let backup = path.with_file_name(name);
    if path.is_file() && !backup.exists() {
        std::fs::copy(path, &backup).ctx(|| format!("backing up {}", path.display()))?;
        return Ok(Some(backup));
    }
    Ok(None)
}

// ---------------------------------------------------------------- Claude Code CLI

/// `claude mcp add -s user loom -- <cmd> mcp`: the supported way to change
/// Claude Code's config (its `~/.claude.json` is rewritten while it runs).
fn claude_cli(claude: &Path, m: &McpCommand) -> Result<()> {
    let _ = run_cli(claude, &["mcp", "remove", SERVER, "-s", "user"]);
    let mut args: Vec<String> = vec!["mcp".into(), "add".into(), "-s".into(), "user".into()];
    for (k, v) in &m.env {
        args.push("-e".into());
        args.push(format!("{k}={v}"));
    }
    args.push(SERVER.into());
    args.push("--".into());
    args.push(m.command.display().to_string());
    args.extend(m.args.iter().cloned());
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    run_cli(claude, &refs)
}

fn run_cli(exe: &Path, args: &[&str]) -> Result<()> {
    let is_script = exe.extension().is_some_and(|e| e.eq_ignore_ascii_case("cmd") || e.eq_ignore_ascii_case("bat"));
    let mut cmd = if is_script {
        let mut c = Command::new("cmd");
        c.arg("/C").arg(exe);
        c
    } else {
        Command::new(exe)
    };
    let out = cmd.args(args).output().ctx(|| format!("running {}", exe.display()))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(Error::invalid(format!(
            "{} {}: {}",
            exe.display(),
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cmd() -> McpCommand {
        McpCommand {
            command: PathBuf::from(r"C:\Program Files\Loom\loom.exe"),
            args: vec!["mcp".into()],
            env: vec![("LOOM_HOME".into(), r"D:\Mods".into())],
        }
    }

    fn installed_ids(env: &AgentEnv) -> Vec<&'static str> {
        list(env).into_iter().filter(|a| a.installed).map(|a| a.id).collect()
    }

    #[test]
    fn detection() {
        let t = tempfile::tempdir().unwrap();
        let env = AgentEnv::sandbox(t.path());
        assert_eq!(installed_ids(&env), Vec::<&str>::new());
        std::fs::create_dir_all(env.path[0].clone()).unwrap();
        std::fs::write(env.path[0].join("codex.cmd"), "").unwrap();
        std::fs::create_dir_all(env.appdata.join("Claude")).unwrap();
        std::fs::create_dir_all(env.home.join(".cursor")).unwrap();
        assert_eq!(installed_ids(&env), ["claude-desktop", "codex", "cursor"]);
    }

    #[test]
    fn json_agents_merge_and_remove() {
        let t = tempfile::tempdir().unwrap();
        let env = AgentEnv::sandbox(t.path());
        // An existing config with another server and other settings.
        let cfg = env.appdata.join("Claude").join("claude_desktop_config.json");
        std::fs::create_dir_all(cfg.parent().unwrap()).unwrap();
        std::fs::write(&cfg, r#"{"theme":"dark","mcpServers":{"other":{"command":"x"}}}"#).unwrap();

        let r = setup(&env, "claude-desktop", &cmd()).unwrap();
        assert!(r.backup.is_some(), "original kept");
        let v: Value = serde_json::from_str(&std::fs::read_to_string(&cfg).unwrap()).unwrap();
        assert_eq!(v["theme"], "dark");
        assert_eq!(v["mcpServers"]["other"]["command"], "x");
        assert_eq!(v["mcpServers"]["loom"]["args"], json!(["mcp"]));
        assert_eq!(v["mcpServers"]["loom"]["env"]["LOOM_HOME"], r"D:\Mods");
        assert!(list(&env).iter().find(|a| a.id == "claude-desktop").unwrap().mcp);

        // Setting up twice is idempotent; the backup is only made once.
        assert!(setup(&env, "claude-desktop", &cmd()).unwrap().backup.is_none());

        remove(&env, "claude-desktop").unwrap();
        let v: Value = serde_json::from_str(&std::fs::read_to_string(&cfg).unwrap()).unwrap();
        assert!(v["mcpServers"].get("loom").is_none());
        assert_eq!(v["mcpServers"]["other"]["command"], "x");
    }

    #[test]
    fn per_agent_shapes_and_skills() {
        let t = tempfile::tempdir().unwrap();
        let env = AgentEnv::sandbox(t.path());
        for id in ["claude-code", "codex", "cursor", "vscode", "opencode", "gemini"] {
            setup(&env, id, &cmd()).unwrap();
        }
        let read = |p: PathBuf| std::fs::read_to_string(p).unwrap();
        let vs: Value = serde_json::from_str(&read(env.appdata.join("Code/User/mcp.json"))).unwrap();
        assert_eq!(vs["servers"]["loom"]["type"], "stdio");
        let oc: Value = serde_json::from_str(&read(env.home.join(".config/opencode/opencode.json"))).unwrap();
        assert_eq!(oc["mcp"]["loom"]["command"][1], "mcp");
        assert_eq!(oc["mcp"]["loom"]["environment"]["LOOM_HOME"], r"D:\Mods");
        let codex = read(env.home.join(".codex/config.toml"));
        assert!(codex.contains("[mcp_servers.loom]"), "{codex}");
        assert!(codex.contains("[mcp_servers.loom.env]"), "{codex}");
        assert!(read(env.home.join(".claude/skills/loom/SKILL.md")).starts_with("---\nname: loom"));
        assert!(env.home.join(".codex/skills/loom/SKILL.md").is_file());
        let all = list(&env);
        assert!(all.iter().filter(|a| a.id != "claude-desktop").all(|a| a.mcp), "{all:#?}");
        remove(&env, "codex").unwrap();
        assert!(!read(env.home.join(".codex/config.toml")).contains("[mcp_servers.loom]"));
        assert!(!env.home.join(".codex/skills/loom").exists());
    }

    #[test]
    fn codex_keeps_existing_settings() {
        let t = tempfile::tempdir().unwrap();
        let env = AgentEnv::sandbox(t.path());
        let cfg = env.home.join(".codex/config.toml");
        std::fs::create_dir_all(cfg.parent().unwrap()).unwrap();
        std::fs::write(&cfg, "model = \"gpt-5\"  # my model\n\n[mcp_servers.other]\ncommand = \"x\"\n").unwrap();
        setup(&env, "codex", &cmd()).unwrap();
        let text = std::fs::read_to_string(&cfg).unwrap();
        assert!(text.contains("model = \"gpt-5\"  # my model"), "{text}");
        assert!(text.contains("[mcp_servers.other]"), "{text}");
        assert!(text.contains("[mcp_servers.loom]"), "{text}");
    }

    #[test]
    fn json_with_comments_is_not_rewritten() {
        let t = tempfile::tempdir().unwrap();
        let env = AgentEnv::sandbox(t.path());
        let cfg = env.appdata.join("Code/User/mcp.json");
        std::fs::create_dir_all(cfg.parent().unwrap()).unwrap();
        let original = "{\n  // my servers\n  \"servers\": {}\n}\n";
        std::fs::write(&cfg, original).unwrap();
        let err = setup(&env, "vscode", &cmd()).unwrap_err();
        assert!(err.to_string().contains("by hand"), "{err}");
        assert_eq!(std::fs::read_to_string(&cfg).unwrap(), original);
    }

    #[test]
    fn skill_is_valid_agent_skill() {
        let (front, body) = SKILL.trim_start_matches("---\n").split_once("\n---\n").unwrap();
        assert!(front.contains("name: loom") && front.contains("description: "));
        assert!(body.contains("loom_doctor"));
    }
}
