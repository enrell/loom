//! Loom desktop: a Tauri shell over `loom-core`.
//!
//! Every command opens the library fresh (so changes made from the CLI or by
//! an agent over MCP show up immediately) and calls the same core function the
//! CLI does. Long operations (installs, downloads, launches) run on worker
//! threads and report through events.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;
use std::sync::Mutex;

use loom_core::launch::{LaunchOptions, Mode};
use loom_core::mods::InstallOptions;
use loom_core::{Game, Loom};
use serde_json::{Value, json};
use tauri::webview::{PageLoadEvent, WebviewBuilder};
use tauri::{AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, State, Url, Webview, WebviewUrl, Window};

type Res<T> = Result<T, String>;

const NEXUS_HOME: &str = "https://www.nexusmods.com/games";

struct App {
    home: PathBuf,
    pending_nxm: Mutex<Vec<String>>,
}

fn s(e: impl ToString) -> String {
    e.to_string()
}

fn loom(app: &App) -> Res<Loom> {
    Loom::open(&app.home).map_err(s)
}

fn game(app: &App, id: &str) -> Res<Game> {
    loom(app)?.game(id).map_err(s)
}

fn json(v: impl serde::Serialize) -> Res<Value> {
    serde_json::to_value(v).map_err(s)
}

/// Runs blocking core work off the async runtime's reactor threads.
async fn blocking<T: Send + 'static>(f: impl FnOnce() -> Res<T> + Send + 'static) -> Res<T> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(s)?
}

// ---------------------------------------------------------------- library

#[tauri::command]
async fn library(app: State<'_, App>) -> Res<Value> {
    let l = loom(&app)?;
    let games: Vec<Value> = l
        .game_ids()
        .map_err(s)?
        .into_iter()
        .map(|id| match l.game(&id) {
            Ok(g) => json!({ "id": id, "name": g.spec().name, "ok": true, "profile": g.active_profile() }),
            Err(e) => json!({ "id": id, "name": id, "ok": false, "error": e.to_string() }),
        })
        .collect();
    Ok(json!({ "home": l.home(), "games": games, "plugins": l.plugins.list().map_err(s)?.len() }))
}

#[tauri::command]
async fn available_games(app: State<'_, App>) -> Res<Value> {
    let home = app.home.clone();
    blocking(move || json(Loom::open(&home).map_err(s)?.available_games().map_err(s)?)).await
}

#[tauri::command]
async fn game_add(app: State<'_, App>, id: String, path: Option<String>) -> Res<Value> {
    let l = loom(&app)?;
    let g = l.add_game(&id, path.as_deref().map(std::path::Path::new)).map_err(s)?;
    json(&g.instance)
}

#[tauri::command]
async fn game_overview(app: State<'_, App>, id: String) -> Res<Value> {
    let g = game(&app, &id)?;
    let spec = g.spec();
    Ok(json!({
        "id": g.id(), "name": spec.name, "path": g.root(), "profile": g.active_profile(),
        "profiles": g.profiles().map_err(s)?, "executables": spec.executables,
        "nexus_domain": spec.nexus_domain, "has_load_order": g.has_load_order(),
        "launch_mode": g.launch_mode(None), "plugin": g.plugin.lock,
    }))
}

// ---------------------------------------------------------------- mods

#[tauri::command]
async fn mods(app: State<'_, App>, game: String) -> Res<Value> {
    json(self::game(&app, &game)?.mods().map_err(s)?)
}

#[tauri::command]
async fn mod_install(app: State<'_, App>, game: String, source: String) -> Res<Value> {
    let home = app.home.clone();
    blocking(move || {
        let g = Loom::open(&home).map_err(s)?.game(&game).map_err(s)?;
        json(g.install_mod(std::path::Path::new(&source), &InstallOptions::default()).map_err(s)?)
    })
    .await
}

#[tauri::command]
async fn mod_set_enabled(app: State<'_, App>, game: String, name: String, enabled: bool) -> Res<()> {
    self::game(&app, &game)?.set_mod_enabled(&name, enabled).map_err(s)
}

#[tauri::command]
async fn mod_move(app: State<'_, App>, game: String, name: String, index: usize) -> Res<()> {
    self::game(&app, &game)?.move_mod(&name, index).map_err(s)
}

#[tauri::command]
async fn mod_remove(app: State<'_, App>, game: String, name: String) -> Res<()> {
    self::game(&app, &game)?.remove_mod(&name).map_err(s)
}

#[tauri::command]
async fn conflicts(app: State<'_, App>, game: String) -> Res<Value> {
    json(self::game(&app, &game)?.conflicts().map_err(s)?)
}

// ---------------------------------------------------------------- load order, doctor, profiles

#[tauri::command]
async fn load_order(app: State<'_, App>, game: String) -> Res<Value> {
    json(self::game(&app, &game)?.load_order().map_err(s)?)
}

#[tauri::command]
async fn plugin_set_active(app: State<'_, App>, game: String, name: String, active: bool) -> Res<()> {
    self::game(&app, &game)?.set_plugin_active(&name, active).map_err(s)
}

#[tauri::command]
async fn plugin_move(app: State<'_, App>, game: String, name: String, index: usize) -> Res<()> {
    self::game(&app, &game)?.move_plugin(&name, index).map_err(s)
}

#[tauri::command]
async fn load_order_sort(app: State<'_, App>, game: String) -> Res<Value> {
    json(self::game(&app, &game)?.sort_load_order().map_err(s)?)
}

#[tauri::command]
async fn doctor(app: State<'_, App>, game: String) -> Res<Value> {
    let home = app.home.clone();
    blocking(move || json(Loom::open(&home).map_err(s)?.game(&game).map_err(s)?.doctor())).await
}

#[tauri::command]
async fn profile_switch(app: State<'_, App>, game: String, name: String) -> Res<()> {
    self::game(&app, &game)?.switch_profile(&name).map_err(s)
}

#[tauri::command]
async fn profile_create(app: State<'_, App>, game: String, name: String, from: Option<String>) -> Res<()> {
    self::game(&app, &game)?.create_profile(&name, from.as_deref()).map_err(s)
}

#[tauri::command]
async fn overwrite_to_mod(app: State<'_, App>, game: String, name: String) -> Res<usize> {
    self::game(&app, &game)?.overwrite_to_mod(&name).map_err(s)
}

// ---------------------------------------------------------------- launch & tools

/// Starts the game on a worker thread; `launch-finished` carries the report.
#[tauri::command]
async fn launch(app: State<'_, App>, handle: AppHandle, game: String, exe: Option<String>, mode: Option<String>) -> Res<()> {
    let g = self::game(&app, &game)?;
    let mode = mode.map(|m| m.parse::<Mode>()).transpose().map_err(s)?;
    std::thread::spawn(move || {
        let r = g.launch(&LaunchOptions { exe, args: Vec::new(), mode });
        let _ = handle.emit_to("ui", "launch-finished", match r {
            Ok(rep) => json!({ "game": game, "report": rep }),
            Err(e) => json!({ "game": game, "error": e.to_string() }),
        });
    });
    Ok(())
}

#[tauri::command]
async fn tools(app: State<'_, App>, game: String) -> Res<Value> {
    json(self::game(&app, &game)?.tools().map_err(s)?)
}

#[tauri::command]
async fn tool_run(app: State<'_, App>, handle: AppHandle, game: String, tool: String) -> Res<()> {
    let g = self::game(&app, &game)?;
    std::thread::spawn(move || {
        let r = g.run_tool(&tool, &[], None);
        let _ = handle.emit_to("ui", "tool-finished", match r {
            Ok(rep) => json!({ "game": game, "report": rep }),
            Err(e) => json!({ "game": game, "tool": tool, "error": e.to_string() }),
        });
    });
    Ok(())
}

// ---------------------------------------------------------------- plugins & marketplace

#[tauri::command]
async fn plugins(app: State<'_, App>) -> Res<Value> {
    let list = loom(&app)?.plugins.list().map_err(s)?;
    Ok(Value::Array(
        list.iter()
            .map(|p| json!({ "lock": p.lock, "name": p.manifest.plugin.name, "game": p.manifest.game.as_ref().map(|g| &g.id), "tools": p.manifest.tools.len() }))
            .collect(),
    ))
}

#[tauri::command]
async fn market_search(app: State<'_, App>, query: String) -> Res<Value> {
    let home = app.home.clone();
    blocking(move || json(Loom::open(&home).map_err(s)?.search_marketplace(&query).map_err(s)?)).await
}

#[tauri::command]
async fn market_install(app: State<'_, App>, id: String) -> Res<Value> {
    let home = app.home.clone();
    blocking(move || json(Loom::open(&home).map_err(s)?.install_from_marketplace(&id, None).map_err(s)?.lock)).await
}

#[tauri::command]
async fn plugin_add_dir(app: State<'_, App>, dir: String) -> Res<Value> {
    json(loom(&app)?.plugins.install_dir(std::path::Path::new(&dir)).map_err(s)?.lock)
}

#[tauri::command]
async fn plugin_remove(app: State<'_, App>, id: String) -> Res<bool> {
    loom(&app)?.plugins.remove(&id).map_err(s)
}

// ---------------------------------------------------------------- settings

#[tauri::command]
async fn settings(app: State<'_, App>) -> Res<Value> {
    let l = loom(&app)?;
    Ok(json!({
        "nexus_api_key_set": l.config.nexus_api_key.is_some(),
        "launch_mode": l.config.launch_mode,
        "marketplace": l.config.marketplace.clone().unwrap_or_else(|| loom_core::marketplace::DEFAULT.into()),
        "mount_available": loom_core::launch::mount_available(),
        "home": l.home(),
    }))
}

#[tauri::command]
async fn settings_set(app: State<'_, App>, key: String, value: Option<String>) -> Res<()> {
    let mut l = loom(&app)?;
    let v = value.filter(|v| !v.trim().is_empty());
    match key.as_str() {
        "nexus_api_key" => l.config.nexus_api_key = v,
        "launch_mode" => l.config.launch_mode = v.filter(|m| m != "auto"),
        "marketplace" => l.config.marketplace = v,
        _ => return Err(format!("unknown setting {key}")),
    }
    l.save_config().map_err(s)
}

// ---------------------------------------------------------------- nexus

/// Downloads and installs an nxm:// link on a worker thread, reporting
/// `download-progress` and `download-finished`.
#[tauri::command]
async fn nxm_download(app: State<'_, App>, handle: AppHandle, url: String) -> Res<()> {
    let l = loom(&app)?;
    std::thread::spawn(move || {
        let mut last = 0u64;
        let h = handle.clone();
        let u = url.clone();
        let r = l.handle_nxm(&url, true, &mut |done, total| {
            if done - last >= 512 * 1024 || Some(done) == total {
                last = done;
                let _ = h.emit_to("ui", "download-progress", json!({ "url": u, "done": done, "total": total }));
            }
        });
        let _ = handle.emit_to("ui", "download-finished", match r {
            Ok(d) => json!({ "url": url, "result": d }),
            Err(e) => json!({ "url": url, "error": e.to_string() }),
        });
    });
    Ok(())
}

/// `LOOM_START_PAGE` (docs screenshots, tests): the page to open first.
#[tauri::command]
fn start_page() -> Option<String> {
    std::env::var("LOOM_START_PAGE").ok().filter(|p| !p.is_empty())
}

#[tauri::command]
fn take_nxm(app: State<'_, App>) -> Vec<String> {
    std::mem::take(&mut *app.pending_nxm.lock().unwrap())
}

/// Makes Loom this user's `nxm://` handler (HKCU, no admin).
#[tauri::command]
fn register_nxm() -> Res<String> {
    let exe = std::env::current_exe().map_err(s)?.display().to_string();
    let reg = |args: &[&str]| -> Res<()> {
        let out = std::process::Command::new("reg").args(args).output().map_err(s)?;
        if out.status.success() { Ok(()) } else { Err(String::from_utf8_lossy(&out.stderr).trim().to_owned()) }
    };
    let base = r"HKCU\Software\Classes\nxm";
    reg(&["add", base, "/ve", "/d", "URL:nxm", "/f"])?;
    reg(&["add", base, "/v", "URL Protocol", "/d", "", "/f"])?;
    reg(&["add", &format!(r"{base}\shell\open\command"), "/ve", "/d", &format!("\"{exe}\" \"%1\""), "/f"])?;
    Ok(format!("Loom ({exe}) now opens nxm:// links."))
}

fn nxm_args(args: impl IntoIterator<Item = String>) -> Vec<String> {
    args.into_iter().filter(|a| a.to_ascii_lowercase().starts_with("nxm://")).collect()
}

// ---------------------------------------------------------------- embedded browser

/// The Nexus webview, created on first use; `nxm://` navigations inside it are
/// handed to the UI instead of Windows.
fn nexus(window: &Window) -> tauri::Result<Webview> {
    static CREATE: Mutex<()> = Mutex::new(());
    let _guard = CREATE.lock().unwrap();
    if let Some(w) = window.get_webview("nexus") {
        return Ok(w);
    }
    let app = window.app_handle().clone();
    let nav_app = app.clone();
    let builder = WebviewBuilder::new("nexus", WebviewUrl::External(Url::parse(NEXUS_HOME).expect("url")))
        .background_color(tauri::window::Color(23, 27, 34, 255))
        .on_navigation(move |url| {
            if url.scheme().eq_ignore_ascii_case("nxm") {
                let _ = nav_app.emit_to("ui", "nxm", url.to_string());
                return false;
            }
            true
        })
        .on_page_load(move |_wv, payload| {
            if payload.event() == PageLoadEvent::Finished {
                let _ = app.emit_to("ui", "browser-nav", payload.url().to_string());
            }
        });
    window.add_child(builder, LogicalPosition::new(0.0, 0.0), LogicalSize::new(1.0, 1.0))
}

#[tauri::command]
async fn browser_show(window: Window, x: f64, y: f64, width: f64, height: f64) -> Res<()> {
    let wv = nexus(&window).map_err(s)?;
    wv.set_position(LogicalPosition::new(x, y)).map_err(s)?;
    wv.set_size(LogicalSize::new(width, height)).map_err(s)?;
    wv.show().map_err(s)
}

#[tauri::command]
async fn browser_hide(window: Window) -> Res<()> {
    window.get_webview("nexus").map_or(Ok(()), |w| w.hide().map_err(s))
}

#[tauri::command]
async fn browser_navigate(window: Window, url: String) -> Res<()> {
    let url = Url::parse(&url).map_err(s)?;
    nexus(&window).and_then(|w| w.navigate(url)).map_err(s)
}

#[tauri::command]
async fn browser_eval(window: Window, js: String) -> Res<()> {
    window.get_webview("nexus").map_or(Ok(()), |w| w.eval(&js).map_err(s))
}

fn main() {
    let home = loom_core::paths::default_home();
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            for url in nxm_args(argv) {
                let _ = app.emit_to("ui", "nxm", url);
            }
            if let Some(w) = app.get_window("main") {
                let _ = w.unminimize();
                let _ = w.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(App { home, pending_nxm: Mutex::new(nxm_args(std::env::args())) })
        .setup(|app| {
            // LOOM_WINDOW=x,y (tests, screenshots): open there without taking focus.
            let test_pos = std::env::var("LOOM_WINDOW").ok().and_then(|v| {
                let (x, y) = v.split_once(',')?;
                Some((x.trim().parse::<f64>().ok()?, y.trim().parse::<f64>().ok()?))
            });
            let mut builder = tauri::window::WindowBuilder::new(app, "main")
                .title("Loom")
                .inner_size(1280.0, 800.0)
                .min_inner_size(1024.0, 640.0)
                .background_color(tauri::window::Color(14, 16, 20, 255));
            if let Some((x, y)) = test_pos {
                builder = builder.position(x, y).focused(false);
            }
            let window = builder.build()?;
            let size = window.inner_size()?.to_logical::<f64>(window.scale_factor()?);
            window.add_child(
                WebviewBuilder::new("ui", WebviewUrl::App("index.html".into())).auto_resize().focused(test_pos.is_none()),
                LogicalPosition::new(0.0, 0.0),
                size,
            )?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            library,
            available_games,
            game_add,
            game_overview,
            mods,
            mod_install,
            mod_set_enabled,
            mod_move,
            mod_remove,
            conflicts,
            load_order,
            plugin_set_active,
            plugin_move,
            load_order_sort,
            doctor,
            profile_switch,
            profile_create,
            overwrite_to_mod,
            launch,
            tools,
            tool_run,
            plugins,
            market_search,
            market_install,
            plugin_add_dir,
            plugin_remove,
            settings,
            settings_set,
            nxm_download,
            take_nxm,
            start_page,
            register_nxm,
            browser_show,
            browser_hide,
            browser_navigate,
            browser_eval,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Loom");
}
