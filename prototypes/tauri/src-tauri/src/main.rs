//! Loom UI shell prototype — Tauri. Serves the shared ui/ in one webview and
//! the embedded Nexus browser as a second (child) webview placed over the UI's
//! browser panel. Implements the same host API as the Electron preload.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::Mutex;

use loom_proto_host as host;
use serde::{Deserialize, Serialize};
use tauri::webview::{PageLoadEvent, WebviewBuilder};
use tauri::{Emitter, LogicalPosition, LogicalSize, Manager, State, Url, Webview, WebviewUrl, Window};

const HOME: &str = "https://www.nexusmods.com/games/skyrimspecialedition";

struct AppState {
    sampler: Mutex<host::Sampler>,
    pending_nxm: Mutex<Vec<String>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Info {
    shell: &'static str,
    version: String,
    engine: String,
    renderer: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MetricsDto {
    working_set_mb: f64,
    cpu_percent: f64,
    processes: usize,
}

#[derive(Serialize)]
struct RegisterResult {
    ok: bool,
    message: String,
}

#[derive(Deserialize)]
struct Rect {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

#[tauri::command]
fn info() -> Info {
    let engine = tauri::webview_version().map_or_else(|_| "WebView2".into(), |v| format!("WebView2 {v}"));
    Info { shell: "Tauri", version: tauri::VERSION.into(), engine, renderer: "WebView2 (Chromium, GPU compositing)" }
}

#[tauri::command]
fn metrics(state: State<'_, AppState>) -> MetricsDto {
    let m = state.sampler.lock().unwrap().sample();
    MetricsDto { working_set_mb: m.working_set_mb, cpu_percent: m.cpu_percent, processes: m.processes }
}

#[tauri::command]
fn startup_ms() -> f64 {
    host::startup_ms()
}

#[tauri::command]
fn ready() {
    host::mark_ready();
}

#[tauri::command]
fn bench_scenario() -> Option<String> {
    host::bench_scenario()
}

#[tauri::command]
fn take_nxm(state: State<'_, AppState>) -> Vec<String> {
    std::mem::take(&mut *state.pending_nxm.lock().unwrap())
}

#[tauri::command]
fn register_nxm() -> RegisterResult {
    let exe = std::env::current_exe().unwrap_or_default();
    match host::register_nxm(&exe) {
        Ok(message) => RegisterResult { ok: true, message },
        Err(e) => RegisterResult { ok: false, message: format!("Registration failed: {e}") },
    }
}

#[tauri::command]
fn open_external(url: String) -> Result<(), String> {
    tauri_plugin_opener::open_url(url, None::<&str>).map_err(|e| e.to_string())
}

/// The Nexus webview, created on first use as a child of the main window.
/// Only called from async commands: creating a webview from a sync command
/// deadlocks the main thread on Windows.
fn nexus(window: &Window) -> tauri::Result<Webview> {
    // `browser_show` and `browser_navigate` arrive concurrently; without the
    // lock both would try to create the "nexus" webview.
    static CREATE: Mutex<()> = Mutex::new(());
    let _guard = CREATE.lock().unwrap();
    if let Some(w) = window.get_webview("nexus") {
        return Ok(w);
    }
    let app = window.app_handle().clone();
    let nav_app = app.clone();
    let builder = WebviewBuilder::new("nexus", WebviewUrl::External(Url::parse(HOME).unwrap()))
        .focused(host::bench_window().is_none())
        .background_color(tauri::window::Color(23, 27, 34, 255))
        // "Mod Manager Download" inside the embedded page: catch the nxm://
        // link here instead of letting Windows route it to a protocol handler.
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
async fn browser_show(window: Window, x: f64, y: f64, width: f64, height: f64) -> Result<(), String> {
    let r = Rect { x, y, width, height };
    let wv = nexus(&window).map_err(|e| e.to_string())?;
    wv.set_position(LogicalPosition::new(r.x, r.y)).map_err(|e| e.to_string())?;
    wv.set_size(LogicalSize::new(r.width, r.height)).map_err(|e| e.to_string())?;
    wv.show().map_err(|e| e.to_string())
}

#[tauri::command]
async fn browser_hide(window: Window) -> Result<(), String> {
    match window.get_webview("nexus") {
        Some(wv) => wv.hide().map_err(|e| e.to_string()),
        None => Ok(()),
    }
}

#[tauri::command]
async fn browser_navigate(window: Window, url: String) -> Result<(), String> {
    let url = Url::parse(&url).map_err(|e| e.to_string())?;
    nexus(&window).and_then(|wv| wv.navigate(url)).map_err(|e| e.to_string())
}

fn eval_in_nexus(window: &Window, js: &str) -> Result<(), String> {
    match window.get_webview("nexus") {
        Some(wv) => wv.eval(js).map_err(|e| e.to_string()),
        None => Ok(()),
    }
}

#[tauri::command]
async fn browser_back(window: Window) -> Result<(), String> {
    eval_in_nexus(&window, "history.back()")
}

#[tauri::command]
async fn browser_forward(window: Window) -> Result<(), String> {
    eval_in_nexus(&window, "history.forward()")
}

#[tauri::command]
async fn browser_reload(window: Window) -> Result<(), String> {
    eval_in_nexus(&window, "location.reload()")
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            for url in host::nxm_args(argv) {
                let _ = app.emit_to("ui", "nxm", url);
            }
            if let Some(w) = app.get_window("main") {
                let _ = w.unminimize();
                let _ = w.set_focus();
            }
        }))
        .plugin(tauri_plugin_opener::init())
        .manage(AppState {
            sampler: Mutex::new(host::Sampler::new()),
            pending_nxm: Mutex::new(host::nxm_args(std::env::args())),
        })
        .setup(|app| {
            let bench_pos = host::bench_window();
            let mut builder = tauri::window::WindowBuilder::new(app, "main")
                .title("Loom")
                .inner_size(1280.0, 800.0)
                .min_inner_size(1024.0, 640.0)
                .background_color(tauri::window::Color(14, 16, 20, 255));
            if let Some((x, y)) = bench_pos {
                builder = builder.position(x, y).focused(false);
            }
            let window = builder.build()?;
            let size = window.inner_size()?.to_logical::<f64>(window.scale_factor()?);
            // WebView2 focuses itself on creation unless told otherwise.
            window.add_child(
                WebviewBuilder::new("ui", WebviewUrl::App("index.html".into())).auto_resize().focused(bench_pos.is_none()),
                LogicalPosition::new(0.0, 0.0),
                size,
            )?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            info,
            metrics,
            startup_ms,
            ready,
            bench_scenario,
            take_nxm,
            register_nxm,
            open_external,
            browser_show,
            browser_hide,
            browser_navigate,
            browser_back,
            browser_forward,
            browser_reload,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
