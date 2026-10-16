//! # loom-mcp — Loom for AI agents
//!
//! A Model Context Protocol server over stdio (JSON-RPC 2.0, one message per
//! line). Every tool is a thin call into `loom-core` — the same functions the
//! CLI and the desktop app use — and returns JSON.
//!
//! Repair loop an agent should follow: `loom_doctor` → apply each problem's
//! `fix` (or the equivalent tool) → `loom_doctor` again until it is clean.

use std::io::{BufRead, Write};
use std::path::PathBuf;

use loom_core::launch::{LaunchOptions, Mode};
use loom_core::mods::InstallOptions;
use loom_core::{Error, Loom};
use serde_json::{Value, json};

const PROTOCOL: &str = "2025-06-18";

const INSTRUCTIONS: &str = "Loom manages game mods without touching the game folder. \
Game support comes from plugins: if a game is missing, search the marketplace (loom_market_search) and install its plugin. \
Mods are ordered: index 0 has the highest priority and wins file conflicts. \
For games with a plugin load order, masters must load before the plugins that need them. \
To fix a setup, call loom_doctor, apply the suggested fixes, and call loom_doctor again until it reports no errors. \
Tools (translators, editors) run inside the game's virtual view and write into a 'Generated - <tool>' mod.";

type Handler = fn(&Loom, &Value) -> loom_core::Result<Value>;

struct Tool {
    name: &'static str,
    description: &'static str,
    /// `(name, type, description, required)`.
    params: &'static [(&'static str, &'static str, &'static str, bool)],
    handler: Handler,
}

const GAME: (&str, &str, &str, bool) = ("game", "string", "Game id, e.g. skyrimse", true);

fn str_arg<'a>(args: &'a Value, key: &str) -> loom_core::Result<&'a str> {
    args.get(key).and_then(Value::as_str).ok_or_else(|| Error::invalid(format!("missing argument {key:?}")))
}

fn opt_str<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key).and_then(Value::as_str)
}

fn usize_arg(args: &Value, key: &str) -> loom_core::Result<usize> {
    args.get(key)
        .and_then(Value::as_u64)
        .and_then(|n| usize::try_from(n).ok())
        .ok_or_else(|| Error::invalid(format!("missing integer argument {key:?}")))
}

fn bool_arg(args: &Value, key: &str) -> loom_core::Result<bool> {
    args.get(key).and_then(Value::as_bool).ok_or_else(|| Error::invalid(format!("missing boolean argument {key:?}")))
}

fn strings(args: &Value, key: &str) -> Vec<String> {
    args.get(key)
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default()
}

#[allow(clippy::unnecessary_wraps)] // handlers all return Result
fn to_json(v: impl serde::Serialize) -> loom_core::Result<Value> {
    Ok(serde_json::to_value(v).expect("serializable"))
}

fn mode(args: &Value) -> loom_core::Result<Option<Mode>> {
    opt_str(args, "mode").map(str::parse).transpose()
}

#[allow(clippy::too_many_lines)]
fn tools() -> Vec<Tool> {
    vec![
        Tool {
            name: "loom_games",
            description: "Games supported by installed plugins: whether each is set up and where it was found.",
            params: &[],
            handler: |l, _| to_json(l.available_games()?),
        },
        Tool {
            name: "loom_game_add",
            description: "Set up a game from its installed plugin (detected automatically, or at path).",
            params: &[GAME, ("path", "string", "Game install folder, if detection fails", false)],
            handler: |l, a| {
                to_json(&l.add_game(str_arg(a, "game")?, opt_str(a, "path").map(std::path::Path::new))?.instance)
            },
        },
        Tool {
            name: "loom_game_info",
            description: "A set-up game: folder, active profile, executables, plugin, launch mode.",
            params: &[GAME],
            handler: |l, a| {
                let g = l.game(str_arg(a, "game")?)?;
                Ok(json!({ "id": g.id(), "name": g.spec().name, "path": g.root(), "profile": g.active_profile(),
                    "plugin": g.plugin.lock, "executables": g.spec().executables, "has_load_order": g.has_load_order(),
                    "launch_mode": g.launch_mode(None) }))
            },
        },
        Tool {
            name: "loom_plugins",
            description: "Installed plugins with version, source and content hash.",
            params: &[],
            handler: |l, _| to_json(l.plugins.list()?.iter().map(|p| &p.lock).collect::<Vec<_>>()),
        },
        Tool {
            name: "loom_plugin_add",
            description: "Install a plugin from a local folder or an import path like github.com/owner/repo/dir@v1.0.0. Prefer loom_market_install for reviewed plugins.",
            params: &[("source", "string", "Folder or import path[@version]", true)],
            handler: |l, a| {
                let s = str_arg(a, "source")?;
                let p = if std::path::Path::new(s).is_dir() {
                    l.plugins.install_dir(std::path::Path::new(s))?
                } else {
                    l.plugins.install_remote(s, None)?
                };
                to_json(p.lock)
            },
        },
        Tool {
            name: "loom_plugin_remove",
            description: "Uninstall a plugin. The game's mods and profiles are kept and come back if the plugin is re-added.",
            params: &[("id", "string", "Plugin import path", true)],
            handler: |l, a| to_json(json!({ "removed": l.plugins.remove(str_arg(a, "id")?)? })),
        },
        Tool {
            name: "loom_market_search",
            description: "Search the marketplace of reviewed plugins by name, game id or tag (empty query lists all).",
            params: &[("query", "string", "Search text", false)],
            handler: |l, a| to_json(l.search_marketplace(opt_str(a, "query").unwrap_or(""))?),
        },
        Tool {
            name: "loom_market_install",
            description: "Install a marketplace plugin by import path or game id at a reviewed version (hash-verified).",
            params: &[
                ("plugin", "string", "Import path or game id", true),
                ("version", "string", "Reviewed version (default: latest)", false),
            ],
            handler: |l, a| to_json(l.install_from_marketplace(str_arg(a, "plugin")?, opt_str(a, "version"))?.lock),
        },
        Tool {
            name: "loom_mods",
            description: "Mods of the active profile in priority order (index 0 wins conflicts), with enabled state, size and source.",
            params: &[GAME],
            handler: |l, a| to_json(l.game(str_arg(a, "game")?)?.mods()?),
        },
        Tool {
            name: "loom_mod_install",
            description: "Install a mod from an archive (.zip/.7z/.rar) or folder. Placed at the top of the active profile, enabled.",
            params: &[
                GAME,
                ("source", "string", "Archive or folder path", true),
                ("name", "string", "Mod name (default: archive name)", false),
                ("replace", "boolean", "Replace an existing mod of that name", false),
            ],
            handler: |l, a| {
                let opts = InstallOptions {
                    name: opt_str(a, "name").map(String::from),
                    replace: a.get("replace").and_then(Value::as_bool).unwrap_or(false),
                    ..InstallOptions::default()
                };
                to_json(l.game(str_arg(a, "game")?)?.install_mod(std::path::Path::new(str_arg(a, "source")?), &opts)?)
            },
        },
        Tool {
            name: "loom_mod_set_enabled",
            description: "Enable or disable a mod in the active profile.",
            params: &[GAME, ("name", "string", "Mod name", true), ("enabled", "boolean", "true to enable", true)],
            handler: |l, a| {
                l.game(str_arg(a, "game")?)?.set_mod_enabled(str_arg(a, "name")?, bool_arg(a, "enabled")?)?;
                Ok(json!({ "ok": true }))
            },
        },
        Tool {
            name: "loom_mod_move",
            description: "Move a mod to a priority index (0 = highest priority, wins conflicts).",
            params: &[GAME, ("name", "string", "Mod name", true), ("index", "integer", "New index", true)],
            handler: |l, a| {
                l.game(str_arg(a, "game")?)?.move_mod(str_arg(a, "name")?, usize_arg(a, "index")?)?;
                Ok(json!({ "ok": true }))
            },
        },
        Tool {
            name: "loom_mod_remove",
            description: "Delete a mod (its folder and every profile entry). Irreversible.",
            params: &[GAME, ("name", "string", "Mod name", true)],
            handler: |l, a| {
                l.game(str_arg(a, "game")?)?.remove_mod(str_arg(a, "name")?)?;
                Ok(json!({ "ok": true }))
            },
        },
        Tool {
            name: "loom_conflicts",
            description: "Files provided by more than one enabled mod: winner and losers.",
            params: &[GAME],
            handler: |l, a| to_json(l.game(str_arg(a, "game")?)?.conflicts()?),
        },
        Tool {
            name: "loom_load_order",
            description: "Plugin-file load order with active state, masters, master/light flags and provider.",
            params: &[GAME],
            handler: |l, a| to_json(l.game(str_arg(a, "game")?)?.load_order()?),
        },
        Tool {
            name: "loom_load_order_set_active",
            description: "Activate or deactivate a plugin file (e.g. MyMod.esp).",
            params: &[
                GAME,
                ("plugin", "string", "Plugin file name", true),
                ("active", "boolean", "true to activate", true),
            ],
            handler: |l, a| {
                l.game(str_arg(a, "game")?)?.set_plugin_active(str_arg(a, "plugin")?, bool_arg(a, "active")?)?;
                Ok(json!({ "ok": true }))
            },
        },
        Tool {
            name: "loom_load_order_move",
            description: "Move a plugin file to a position (0 = loads first; game masters stay on top).",
            params: &[GAME, ("plugin", "string", "Plugin file name", true), ("index", "integer", "New position", true)],
            handler: |l, a| {
                l.game(str_arg(a, "game")?)?.move_plugin(str_arg(a, "plugin")?, usize_arg(a, "index")?)?;
                Ok(json!({ "ok": true }))
            },
        },
        Tool {
            name: "loom_load_order_sort",
            description: "Sort the load order: masters first and every plugin after its masters, otherwise keeping the current order.",
            params: &[GAME],
            handler: |l, a| to_json(l.game(str_arg(a, "game")?)?.sort_load_order()?),
        },
        Tool {
            name: "loom_doctor",
            description: "Every problem with the game setup, each with a code and a suggested fix. Run after changes until no errors remain.",
            params: &[GAME],
            handler: |l, a| to_json(l.game(str_arg(a, "game")?)?.doctor()),
        },
        Tool {
            name: "loom_profiles",
            description: "Profiles of a game and which one is active.",
            params: &[GAME],
            handler: |l, a| {
                let g = l.game(str_arg(a, "game")?)?;
                Ok(json!({ "active": g.active_profile(), "profiles": g.profiles()? }))
            },
        },
        Tool {
            name: "loom_profile_create",
            description: "Create a profile, empty (all mods disabled) or copied from another.",
            params: &[GAME, ("name", "string", "New profile name", true), ("from", "string", "Profile to copy", false)],
            handler: |l, a| {
                l.game(str_arg(a, "game")?)?.create_profile(str_arg(a, "name")?, opt_str(a, "from"))?;
                Ok(json!({ "ok": true }))
            },
        },
        Tool {
            name: "loom_profile_switch",
            description: "Make a profile the active one.",
            params: &[GAME, ("name", "string", "Profile name", true)],
            handler: |l, a| {
                l.game(str_arg(a, "game")?)?.switch_profile(str_arg(a, "name")?)?;
                Ok(json!({ "ok": true }))
            },
        },
        Tool {
            name: "loom_tools",
            description: "Companion tools for the game (translators, editors) and whether each is installed.",
            params: &[GAME],
            handler: |l, a| to_json(l.game(str_arg(a, "game")?)?.tools()?),
        },
        Tool {
            name: "loom_tool_run",
            description: "Run a tool inside the game's virtual view and wait for it; its output becomes the 'Generated - <tool>' mod. Interactive tools need the user at the screen.",
            params: &[
                GAME,
                ("tool", "string", "Tool id", true),
                ("args", "array", "Extra arguments; {game} expands to the game folder as the tool sees it", false),
                ("mode", "string", "mount or hook", false),
            ],
            handler: |l, a| {
                to_json(l.game(str_arg(a, "game")?)?.run_tool(str_arg(a, "tool")?, &strings(a, "args"), mode(a)?)?)
            },
        },
        Tool {
            name: "loom_overwrite_to_mod",
            description: "Move files the game wrote (the overwrite folder) into a mod.",
            params: &[GAME, ("name", "string", "Mod to create or extend", true)],
            handler: |l, a| {
                to_json(json!({ "moved": l.game(str_arg(a, "game")?)?.overwrite_to_mod(str_arg(a, "name")?)? }))
            },
        },
        Tool {
            name: "loom_launch",
            description: "Start the game with the active profile and wait until it exits. Only when the user asked to play.",
            params: &[
                GAME,
                ("exe", "string", "Executable id (default: the game's default)", false),
                ("args", "array", "Arguments", false),
                ("mode", "string", "mount or hook", false),
            ],
            handler: |l, a| {
                let opts = LaunchOptions {
                    exe: opt_str(a, "exe").map(String::from),
                    args: strings(a, "args"),
                    mode: mode(a)?,
                };
                to_json(l.game(str_arg(a, "game")?)?.launch(&opts)?)
            },
        },
        Tool {
            name: "loom_nxm_download",
            description: "Download a Nexus Mods nxm:// link (from the site's Mod Manager Download button) and install it. Needs a Nexus API key in the user's config.",
            params: &[
                ("url", "string", "nxm:// link", true),
                ("install", "boolean", "Install after downloading (default true)", false),
            ],
            handler: |l, a| {
                to_json(l.handle_nxm(
                    str_arg(a, "url")?,
                    a.get("install").and_then(Value::as_bool).unwrap_or(true),
                    &mut |_, _| {},
                )?)
            },
        },
    ]
}

fn schema(t: &Tool) -> Value {
    let mut props = serde_json::Map::new();
    let mut required = Vec::new();
    for (name, ty, desc, req) in t.params {
        let mut p = json!({ "type": ty, "description": desc });
        if *ty == "array" {
            p["items"] = json!({ "type": "string" });
        }
        props.insert((*name).into(), p);
        if *req {
            required.push(*name);
        }
    }
    json!({ "type": "object", "properties": props, "required": required, "additionalProperties": false })
}

/// Handles one JSON-RPC message; `None` for notifications.
pub fn handle(home: &PathBuf, msg: &Value) -> Option<Value> {
    let id = msg.get("id").cloned()?;
    let method = msg.get("method").and_then(Value::as_str).unwrap_or("");
    let params = msg.get("params").cloned().unwrap_or(Value::Null);
    let result = match method {
        "initialize" => Ok(json!({
            "protocolVersion": params.get("protocolVersion").and_then(Value::as_str).unwrap_or(PROTOCOL),
            "capabilities": { "tools": { "listChanged": false } },
            "serverInfo": { "name": "loom", "version": env!("CARGO_PKG_VERSION") },
            "instructions": INSTRUCTIONS,
        })),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({
            "tools": tools().iter().map(|t| json!({ "name": t.name, "description": t.description, "inputSchema": schema(t) })).collect::<Vec<_>>()
        })),
        "tools/call" => {
            let name = params.get("name").and_then(Value::as_str).unwrap_or("");
            let args = params.get("arguments").cloned().unwrap_or_else(|| json!({}));
            match tools().into_iter().find(|t| t.name == name) {
                None => Err((-32602, format!("unknown tool {name:?}"))),
                Some(t) => {
                    let out = Loom::open(home).and_then(|l| (t.handler)(&l, &args));
                    Ok(match out {
                        Ok(v) => json!({
                            "content": [{ "type": "text", "text": serde_json::to_string_pretty(&v).expect("json") }],
                            "structuredContent": if v.is_object() { v.clone() } else { json!({ "result": v }) },
                            "isError": false,
                        }),
                        Err(e) => {
                            json!({ "content": [{ "type": "text", "text": e.to_string() }], "isError": true })
                        }
                    })
                }
            }
        }
        _ => Err((-32601, format!("method {method:?} not found"))),
    };
    Some(match result {
        Ok(r) => json!({ "jsonrpc": "2.0", "id": id, "result": r }),
        Err((code, message)) => {
            json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
        }
    })
}

/// Serves MCP on stdin/stdout until stdin closes. The library is reopened for
/// every call, so changes made in the app or CLI meanwhile are seen.
pub fn serve_stdio(loom: &Loom) -> std::io::Result<()> {
    let home = loom.home().to_path_buf();
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout().lock();
    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<Value>(&line) {
            Ok(msg) => handle(&home, &msg),
            Err(e) => {
                Some(json!({ "jsonrpc": "2.0", "id": null, "error": { "code": -32700, "message": e.to_string() } }))
            }
        };
        if let Some(r) = reply {
            writeln!(stdout, "{r}")?;
            stdout.flush()?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protocol_basics() {
        let t = tempfile::tempdir().unwrap();
        let home = t.path().to_path_buf();
        let init = handle(&home, &json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": { "protocolVersion": "2025-06-18" } })).unwrap();
        assert_eq!(init["result"]["serverInfo"]["name"], "loom");
        assert!(handle(&home, &json!({ "jsonrpc": "2.0", "method": "notifications/initialized" })).is_none());
        let list = handle(&home, &json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" })).unwrap();
        let tools = list["result"]["tools"].as_array().unwrap();
        assert!(tools.len() > 20);
        assert!(tools.iter().all(|t| t["inputSchema"]["type"] == "object"));
        let call = handle(&home, &json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": { "name": "loom_games", "arguments": {} } })).unwrap();
        assert_eq!(call["result"]["isError"], false);
        let bad = handle(&home, &json!({ "jsonrpc": "2.0", "id": 4, "method": "tools/call", "params": { "name": "loom_mods", "arguments": { "game": "nope" } } })).unwrap();
        assert_eq!(bad["result"]["isError"], true);
        let unknown = handle(&home, &json!({ "jsonrpc": "2.0", "id": 5, "method": "nope" })).unwrap();
        assert_eq!(unknown["error"]["code"], -32601);
    }
}
