//! `loom` — the Loom command line.
//!
//! Every command prints human-readable text, or JSON with `--json` (the
//! format agents and scripts should use). Errors exit with status 1; with
//! `--json` they print `{"error": "..."}`.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use loom_core::doctor::Severity;
use loom_core::launch::{LaunchOptions, Mode};
use loom_core::mods::InstallOptions;
use loom_core::{Error, Loom};
use serde::Serialize;
use serde_json::{Value, json};

#[derive(Parser)]
#[command(name = "loom", version, about = "Loom — a mod manager that never touches your game folder.")]
struct Cli {
    /// Print JSON instead of text (for scripts and agents).
    #[arg(long, global = true)]
    json: bool,
    /// Library folder (default: LOOM_HOME or %LOCALAPPDATA%\Loom).
    #[arg(long, global = true, value_name = "DIR")]
    home: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Install, list and remove plugins (game support and tools).
    #[command(subcommand)]
    Plugin(PluginCmd),
    /// Browse and install reviewed plugins from the marketplace.
    #[command(subcommand)]
    Market(MarketCmd),
    /// Set up and manage games.
    #[command(subcommand)]
    Game(GameCmd),
    /// Profiles: independent mod lists per game.
    #[command(subcommand)]
    Profile(ProfileCmd),
    /// Install, order, enable and remove mods.
    #[command(subcommand, name = "mod")]
    Mod(ModCmd),
    /// The plugin-file load order (games that have one).
    #[command(subcommand)]
    Order(OrderCmd),
    /// Find problems in a game's setup, each with a suggested fix.
    Doctor { game: String },
    /// Start the game with the active profile's mods.
    Launch {
        game: String,
        /// Executable id from the game plugin (see `loom game info`).
        #[arg(long)]
        exe: Option<String>,
        /// mount (WinFsp) or hook (openvfs injection).
        #[arg(long)]
        mode: Option<String>,
        /// Arguments passed to the game.
        #[arg(last = true)]
        args: Vec<String>,
    },
    /// Companion tools (translators, editors, patchers).
    #[command(subcommand)]
    Tool(ToolCmd),
    /// Files the game or tools wrote.
    #[command(subcommand)]
    Overwrite(OverwriteCmd),
    /// Download (and install) a Nexus Mods nxm:// link.
    Nxm {
        url: String,
        /// Only download into the game's downloads folder.
        #[arg(long)]
        no_install: bool,
    },
    /// Read or change settings (nexus_api_key, openvfs_dir, launch_mode, marketplace, tool_paths).
    #[command(subcommand)]
    Config(ConfigCmd),
    /// Run the MCP server on stdio (for AI agents).
    Mcp,
}

#[derive(Subcommand)]
enum PluginCmd {
    /// Install from a folder or an import path (`github.com/owner/repo[/dir][@version]`).
    Add {
        source: String,
    },
    Remove {
        id: String,
    },
    List,
    /// Check installed plugins against their recorded hashes.
    Verify,
    /// Validate a plugin folder (manifest + data-only rule) and print its hash.
    Check {
        dir: PathBuf,
    },
}

#[derive(Subcommand)]
enum MarketCmd {
    Search {
        query: Option<String>,
    },
    /// Install a plugin by import path or game id, at a reviewed version.
    Install {
        plugin: String,
        #[arg(long)]
        version: Option<String>,
    },
    /// Validate a marketplace folder (used by its CI).
    Validate {
        dir: PathBuf,
    },
}

#[derive(Subcommand)]
enum GameCmd {
    /// Games supported by installed plugins, and where they were found.
    List,
    /// Set up a game (detected, or at --path).
    Add {
        game: String,
        #[arg(long)]
        path: Option<PathBuf>,
    },
    Remove {
        game: String,
        /// Also delete its mods, profiles and downloads.
        #[arg(long)]
        purge: bool,
    },
    SetPath {
        game: String,
        path: PathBuf,
    },
    Info {
        game: String,
    },
}

#[derive(Subcommand)]
enum ProfileCmd {
    List {
        game: String,
    },
    New {
        game: String,
        name: String,
        /// Copy this profile instead of starting empty.
        #[arg(long)]
        from: Option<String>,
    },
    Switch {
        game: String,
        name: String,
    },
    Remove {
        game: String,
        name: String,
    },
}

#[derive(Subcommand)]
enum ModCmd {
    /// Mods in priority order (first wins).
    List {
        game: String,
    },
    /// Install an archive (.zip, .7z, .rar) or a folder.
    Install {
        game: String,
        source: PathBuf,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        replace: bool,
    },
    Remove {
        game: String,
        name: String,
    },
    Enable {
        game: String,
        name: String,
    },
    Disable {
        game: String,
        name: String,
    },
    /// Move to a priority (0 = highest).
    Move {
        game: String,
        name: String,
        index: usize,
    },
    Rename {
        game: String,
        from: String,
        to: String,
    },
    /// Files more than one enabled mod provides.
    Conflicts {
        game: String,
    },
}

#[derive(Subcommand)]
enum OrderCmd {
    List {
        game: String,
    },
    Enable {
        game: String,
        plugin: String,
    },
    Disable {
        game: String,
        plugin: String,
    },
    /// Move to a position (0 = loads first).
    Move {
        game: String,
        plugin: String,
        index: usize,
    },
    /// Masters first, every plugin after its masters.
    Sort {
        game: String,
    },
    /// Write the order where the game reads it (done on launch too).
    Deploy {
        game: String,
    },
}

#[derive(Subcommand)]
enum ToolCmd {
    List {
        game: String,
    },
    Run {
        game: String,
        tool: String,
        #[arg(long)]
        mode: Option<String>,
        #[arg(last = true)]
        args: Vec<String>,
    },
}

#[derive(Subcommand)]
enum OverwriteCmd {
    /// Move the overwrite folder's files into a mod.
    ToMod { game: String, name: String },
}

#[derive(Subcommand)]
enum ConfigCmd {
    Get { key: Option<String> },
    Set { key: String, value: String },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    if matches!(cli.command, Command::Mcp) {
        let loom = match open(cli.home.as_ref()) {
            Ok(l) => l,
            Err(e) => {
                eprintln!("error: {e}");
                return ExitCode::FAILURE;
            }
        };
        return match loom_mcp::serve_stdio(&loom) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("error: {e}");
                ExitCode::FAILURE
            }
        };
    }
    let json = cli.json;
    match run(cli) {
        Ok(out) => {
            if json {
                println!("{}", serde_json::to_string_pretty(&out.json).expect("json"));
            } else if !out.text.is_empty() {
                println!("{}", out.text.trim_end());
            }
            if out.failed { ExitCode::FAILURE } else { ExitCode::SUCCESS }
        }
        Err(e) => {
            if json {
                println!("{}", json!({ "error": e.to_string() }));
            } else {
                eprintln!("error: {e}");
            }
            ExitCode::FAILURE
        }
    }
}

fn open(home: Option<&PathBuf>) -> loom_core::Result<Loom> {
    match home {
        Some(h) => Loom::open(h),
        None => Loom::open_default(),
    }
}

/// A command's result: JSON for `--json`, text otherwise.
struct Out {
    json: Value,
    text: String,
    failed: bool,
}

impl Out {
    fn new(value: impl Serialize, text: impl Into<String>) -> Self {
        Self { json: serde_json::to_value(value).expect("serializable"), text: text.into(), failed: false }
    }
    fn ok(text: impl Into<String>) -> Self {
        let text = text.into();
        Self { json: json!({ "ok": true, "message": text }), text, failed: false }
    }
}

fn mode(s: Option<&String>) -> loom_core::Result<Option<Mode>> {
    s.map(|m| m.parse()).transpose()
}

fn mb(bytes: u64) -> String {
    #[allow(clippy::cast_precision_loss)]
    let mb = bytes as f64 / 1_048_576.0;
    if mb >= 1024.0 { format!("{:.1} GB", mb / 1024.0) } else { format!("{mb:.1} MB") }
}

#[allow(clippy::too_many_lines)]
fn run(cli: Cli) -> loom_core::Result<Out> {
    let mut loom = open(cli.home.as_ref())?;
    Ok(match cli.command {
        Command::Mcp => unreachable!("handled in main"),

        Command::Plugin(cmd) => match cmd {
            PluginCmd::Add { source } => {
                let p = if PathBuf::from(&source).is_dir() {
                    loom.plugins.install_dir(&PathBuf::from(&source))?
                } else {
                    loom.plugins.install_remote(&source, None)?
                };
                let text = format!("installed {} {} ({})", p.lock.id, p.manifest.plugin.version, p.lock.sum);
                Out::new(&p.lock, text)
            }
            PluginCmd::Remove { id } => {
                if !loom.plugins.remove(&id)? {
                    return Err(Error::invalid(format!("plugin {id} is not installed")));
                }
                Out::ok(format!("removed {id} (game data is kept; re-adding the plugin restores it)"))
            }
            PluginCmd::List => {
                let list = loom.plugins.list()?;
                let text = if list.is_empty() {
                    "no plugins installed — try `loom market search`".to_owned()
                } else {
                    list.iter()
                        .map(|p| {
                            let what = p.manifest.game.as_ref().map_or_else(
                                || format!("{} tools", p.manifest.tools.len()),
                                |g| format!("game {}", g.id),
                            );
                            format!("{}  {}  ({what})", p.lock.id, p.manifest.plugin.version)
                        })
                        .collect::<Vec<_>>()
                        .join("\n")
                };
                Out::new(list.iter().map(|p| &p.lock).collect::<Vec<_>>(), text)
            }
            PluginCmd::Verify => {
                let changed = loom.plugins.verify()?;
                let text = if changed.is_empty() {
                    "all plugins match their recorded hashes".into()
                } else {
                    format!("modified: {}", changed.join(", "))
                };
                Out { failed: !changed.is_empty(), ..Out::new(json!({ "modified": changed }), text) }
            }
            PluginCmd::Check { dir } => {
                let m = loom_core::plugin::Manifest::load_dir(&dir)?;
                loom_core::plugins::check_data_only(&dir)?;
                let sum = loom_core::plugins::content_sum(&dir)?;
                Out::new(
                    json!({ "id": m.plugin.id, "version": m.plugin.version, "sum": sum }),
                    format!("{} {} is valid\nsum: {sum}", m.plugin.id, m.plugin.version),
                )
            }
        },

        Command::Market(cmd) => match cmd {
            MarketCmd::Search { query } => {
                let found = loom.search_marketplace(query.as_deref().unwrap_or(""))?;
                let text = found
                    .iter()
                    .map(|e| {
                        format!(
                            "{:<28} {}  [{}]  {}",
                            e.games.join(","),
                            e.id,
                            e.latest().map_or("-", |v| v.version.as_str()),
                            e.name
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                Out::new(&found, if text.is_empty() { "nothing found".into() } else { text })
            }
            MarketCmd::Install { plugin, version } => {
                let p = loom.install_from_marketplace(&plugin, version.as_deref())?;
                Out::new(
                    &p.lock,
                    format!("installed {} {} (verified {})", p.lock.id, p.manifest.plugin.version, p.lock.sum),
                )
            }
            MarketCmd::Validate { dir } => {
                let entries = loom_core::marketplace::read_dir(&dir)?;
                Out::new(json!({ "entries": entries.len() }), format!("{} entries, all valid", entries.len()))
            }
        },

        Command::Game(cmd) => match cmd {
            GameCmd::List => {
                let games = loom.available_games()?;
                let text = games
                    .iter()
                    .map(|g| {
                        let state = if g.set_up {
                            "set up".to_owned()
                        } else if let Some(c) = g.found.first() {
                            format!("found ({}) {}", c.source, c.path.display())
                        } else {
                            "not found".into()
                        };
                        format!("{:<22} {:<40} {state}", g.id, g.name)
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                Out::new(
                    &games,
                    if text.is_empty() {
                        "no game plugins installed — try `loom market search`".into()
                    } else {
                        text
                    },
                )
            }
            GameCmd::Add { game, path } => {
                let g = loom.add_game(&game, path.as_deref())?;
                Out::new(&g.instance, format!("set up {} at {}", g.spec().name, g.root().display()))
            }
            GameCmd::Remove { game, purge } => {
                loom.remove_game(&game, purge)?;
                Out::ok(if purge {
                    format!("removed {game} and its data")
                } else {
                    format!("forgot {game} (mods and profiles kept)")
                })
            }
            GameCmd::SetPath { game, path } => {
                loom.set_game_path(&game, &path)?;
                Out::ok(format!("{game} now points at {}", path.display()))
            }
            GameCmd::Info { game } => {
                let g = loom.game(&game)?;
                let spec = g.spec();
                let mode = g.launch_mode(None);
                let info = json!({
                    "id": g.id(), "name": spec.name, "path": g.root(), "profile": g.active_profile(),
                    "plugin": g.plugin.lock.id, "plugin_version": g.plugin.manifest.plugin.version,
                    "nexus_domain": spec.nexus_domain, "executables": spec.executables,
                    "load_order": spec.load_order.is_some(), "launch_mode": mode,
                });
                let exes = spec
                    .executables
                    .iter()
                    .map(|e| format!("  {:<12} {}{}", e.id, e.path, if e.default { " (default)" } else { "" }))
                    .collect::<Vec<_>>()
                    .join("\n");
                let text = format!(
                    "{} ({})\n  folder:  {}\n  profile: {}\n  plugin:  {} {}\n  launch:  {mode:?}\nexecutables:\n{exes}",
                    spec.name,
                    g.id(),
                    g.root().display(),
                    g.active_profile(),
                    g.plugin.lock.id,
                    g.plugin.manifest.plugin.version
                );
                Out::new(info, text)
            }
        },

        Command::Profile(cmd) => match cmd {
            ProfileCmd::List { game } => {
                let g = loom.game(&game)?;
                let list = g.profiles()?;
                let text = list
                    .iter()
                    .map(|p| format!("{} {p}", if p == g.active_profile() { "*" } else { " " }))
                    .collect::<Vec<_>>()
                    .join("\n");
                Out::new(json!({ "active": g.active_profile(), "profiles": list }), text)
            }
            ProfileCmd::New { game, name, from } => {
                loom.game(&game)?.create_profile(&name, from.as_deref())?;
                Out::ok(format!("created profile {name}"))
            }
            ProfileCmd::Switch { game, name } => {
                loom.game(&game)?.switch_profile(&name)?;
                Out::ok(format!("{game} now uses profile {name}"))
            }
            ProfileCmd::Remove { game, name } => {
                loom.game(&game)?.remove_profile(&name)?;
                Out::ok(format!("removed profile {name}"))
            }
        },

        Command::Mod(cmd) => match cmd {
            ModCmd::List { game } => {
                let mods = loom.game(&game)?.mods()?;
                let text = mods
                    .iter()
                    .map(|m| {
                        format!(
                            "{:>3}. [{}] {}  ({} files, {}){}",
                            m.priority,
                            if m.enabled { "x" } else { " " },
                            m.name,
                            m.files,
                            mb(m.bytes),
                            if m.missing { "  MISSING" } else { "" }
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                Out::new(&mods, if text.is_empty() { "no mods installed".into() } else { text })
            }
            ModCmd::Install { game, source, name, replace } => {
                let r = loom
                    .game(&game)?
                    .install_mod(&source, &InstallOptions { name, replace, ..InstallOptions::default() })?;
                let text = format!("installed {} ({} files)\n  {}", r.name, r.files, r.layout.join("\n  "));
                Out::new(&r, text)
            }
            ModCmd::Remove { game, name } => {
                loom.game(&game)?.remove_mod(&name)?;
                Out::ok(format!("removed {name}"))
            }
            ModCmd::Enable { game, name } => {
                loom.game(&game)?.set_mod_enabled(&name, true)?;
                Out::ok(format!("enabled {name}"))
            }
            ModCmd::Disable { game, name } => {
                loom.game(&game)?.set_mod_enabled(&name, false)?;
                Out::ok(format!("disabled {name}"))
            }
            ModCmd::Move { game, name, index } => {
                loom.game(&game)?.move_mod(&name, index)?;
                Out::ok(format!("moved {name} to {index}"))
            }
            ModCmd::Rename { game, from, to } => {
                loom.game(&game)?.rename_mod(&from, &to)?;
                Out::ok(format!("renamed {from} to {to}"))
            }
            ModCmd::Conflicts { game } => {
                let c = loom.game(&game)?.conflicts()?;
                let text = c
                    .iter()
                    .map(|c| format!("{}: {} over {}", c.path, c.winner, c.losers.join(", ")))
                    .collect::<Vec<_>>()
                    .join("\n");
                Out::new(
                    &c,
                    if text.is_empty() {
                        "no conflicts".into()
                    } else {
                        format!("{text}\n{} conflicting file(s)", c.len())
                    },
                )
            }
        },

        Command::Order(cmd) => {
            let fmt = |order: &[loom_core::loadorder::PluginEntry]| {
                order
                    .iter()
                    .enumerate()
                    .map(|(i, p)| {
                        let flags = [(p.is_master, "M"), (p.is_light, "L"), (p.implicit, "I")]
                            .iter()
                            .filter(|f| f.0)
                            .map(|f| f.1)
                            .collect::<String>();
                        format!(
                            "{i:>3}. [{}] {:<44} {:<3} {}",
                            if p.active { "x" } else { " " },
                            p.name,
                            flags,
                            p.provider
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            };
            match cmd {
                OrderCmd::List { game } => {
                    let o = loom.game(&game)?.load_order()?;
                    let t = fmt(&o);
                    Out::new(&o, t)
                }
                OrderCmd::Enable { game, plugin } => {
                    loom.game(&game)?.set_plugin_active(&plugin, true)?;
                    Out::ok(format!("enabled {plugin}"))
                }
                OrderCmd::Disable { game, plugin } => {
                    loom.game(&game)?.set_plugin_active(&plugin, false)?;
                    Out::ok(format!("disabled {plugin}"))
                }
                OrderCmd::Move { game, plugin, index } => {
                    loom.game(&game)?.move_plugin(&plugin, index)?;
                    Out::ok(format!("moved {plugin} to {index}"))
                }
                OrderCmd::Sort { game } => {
                    let o = loom.game(&game)?.sort_load_order()?;
                    let t = fmt(&o);
                    Out::new(&o, t)
                }
                OrderCmd::Deploy { game } => {
                    let files = loom.game(&game)?.deploy_load_order()?;
                    Out::new(
                        &files,
                        files.iter().map(|f| format!("wrote {}", f.display())).collect::<Vec<_>>().join("\n"),
                    )
                }
            }
        }

        Command::Doctor { game } => {
            let problems = loom.game(&game)?.doctor();
            let errors = problems.iter().filter(|p| p.severity == Severity::Error).count();
            let text = if problems.is_empty() {
                "no problems found".to_owned()
            } else {
                problems
                    .iter()
                    .map(|p| {
                        let fix = p.fix.as_ref().map(|f| format!("\n      fix: {f}")).unwrap_or_default();
                        format!("{:<7} [{}] {}{fix}", format!("{:?}", p.severity).to_lowercase(), p.code, p.message)
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            };
            Out { failed: errors > 0, ..Out::new(&problems, text) }
        }

        Command::Launch { game, exe, mode: m, args } => {
            let g = loom.game(&game)?;
            let r = g.launch(&LaunchOptions { exe, args, mode: mode(m.as_ref())? })?;
            let text = format!(
                "{} exited with {} ({:?} mode); {} new file(s) captured in the overwrite folder",
                r.program.display(),
                r.exit_code.map_or("no code".into(), |c| c.to_string()),
                r.mode,
                r.captured
            );
            Out::new(&r, text)
        }

        Command::Tool(cmd) => match cmd {
            ToolCmd::List { game } => {
                let tools = loom.game(&game)?.tools()?;
                let text = tools
                    .iter()
                    .map(|t| {
                        format!(
                            "{:<14} {:<28} {}",
                            t.id,
                            t.name,
                            t.found.as_ref().map_or_else(|| "not found".into(), |p| p.display().to_string())
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                Out::new(&tools, if text.is_empty() { "no tools declared for this game".into() } else { text })
            }
            ToolCmd::Run { game, tool, mode: m, args } => {
                let r = loom.game(&game)?.run_tool(&tool, &args, mode(m.as_ref())?)?;
                let text = format!(
                    "{} finished; output in {}",
                    r.tool,
                    r.output_mod.as_deref().unwrap_or("the overwrite folder")
                );
                Out::new(&r, text)
            }
        },

        Command::Overwrite(OverwriteCmd::ToMod { game, name }) => {
            let n = loom.game(&game)?.overwrite_to_mod(&name)?;
            Out::new(json!({ "moved": n, "mod": name }), format!("moved {n} file(s) into {name}"))
        }

        Command::Nxm { url, no_install } => {
            let json_mode = cli.json;
            let mut last = 0u64;
            let r = loom.handle_nxm(&url, !no_install, &mut |done, total| {
                if !json_mode && done - last > 8 * 1_048_576 {
                    last = done;
                    eprintln!("  {} / {}", mb(done), total.map_or("?".into(), mb));
                }
            })?;
            let text = match &r.installed {
                Some(i) => format!("downloaded {} and installed {}", r.archive.display(), i.name),
                None => format!("downloaded {}", r.archive.display()),
            };
            Out::new(&r, text)
        }

        Command::Config(cmd) => match cmd {
            ConfigCmd::Get { key } => {
                let mut v = serde_json::to_value(&loom.config).expect("json");
                if let Some(k) = v.get_mut("nexus_api_key") {
                    *k = json!("(set)");
                }
                let v = match key {
                    Some(k) => v.get(&k).cloned().unwrap_or(Value::Null),
                    None => v,
                };
                let text = serde_json::to_string_pretty(&v).expect("json");
                Out::new(v, text)
            }
            ConfigCmd::Set { key, value } => {
                let c = &mut loom.config;
                match key.as_str() {
                    "nexus_api_key" => c.nexus_api_key = Some(value),
                    "openvfs_dir" => c.openvfs_dir = Some(value.into()),
                    "marketplace" => c.marketplace = Some(value),
                    "launch_mode" => {
                        value.parse::<Mode>().or_else(|e| if value == "auto" { Ok(Mode::Mount) } else { Err(e) })?;
                        c.launch_mode = (value != "auto").then_some(value);
                    }
                    "tool_paths" => c.tool_paths = std::env::split_paths(&value).collect(),
                    _ => return Err(Error::invalid(format!("unknown setting {key:?}"))),
                }
                loom.save_config()?;
                Out::ok(format!("set {key}"))
            }
        },
    })
}
