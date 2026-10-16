//! Loom UI shell prototype — egui. Same screens as the web shells, drawn
//! natively; no embedded browser or video (egui has neither).

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod data;
mod theme;
mod widgets;

use std::io::{BufRead, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex, OnceLock};

use eframe::egui::{self, FontData, FontDefinitions, FontFamily};
use loom_proto_host as host;

/// Single-instance port: a second launch (e.g. Windows handing us an
/// nxm:// link) forwards its links here and exits.
const INSTANCE_PORT: u16 = 47_391;

fn main() -> eframe::Result {
    let inbox: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(host::nxm_args(std::env::args())));
    let listener = match TcpListener::bind(("127.0.0.1", INSTANCE_PORT)) {
        Ok(l) => l,
        Err(_) => {
            if let Ok(mut s) = TcpStream::connect(("127.0.0.1", INSTANCE_PORT)) {
                for url in inbox.lock().unwrap().iter() {
                    let _ = writeln!(s, "{url}");
                }
            }
            return Ok(());
        }
    };
    let ctx_slot: Arc<OnceLock<egui::Context>> = Arc::new(OnceLock::new());
    {
        let (inbox, ctx_slot) = (Arc::clone(&inbox), Arc::clone(&ctx_slot));
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let urls: Vec<String> = std::io::BufReader::new(stream).lines().map_while(Result::ok).collect();
                inbox.lock().unwrap().extend(host::nxm_args(urls));
                if let Some(ctx) = ctx_slot.get() {
                    ctx.request_repaint();
                }
            }
        });
    }

    let live = Arc::new(Mutex::new(app::Live::default()));
    {
        let (live, ctx_slot) = (Arc::clone(&live), Arc::clone(&ctx_slot));
        std::thread::spawn(move || {
            let mut sampler = host::Sampler::new();
            loop {
                let m = sampler.sample();
                {
                    let mut l = live.lock().unwrap();
                    l.now = m;
                    l.mem.push(m.working_set_mb);
                    l.cpu.push(m.cpu_percent);
                    if l.mem.len() > 60 {
                        l.mem.remove(0);
                        l.cpu.remove(0);
                    }
                }
                if let Some(ctx) = ctx_slot.get() {
                    ctx.request_repaint();
                }
                std::thread::sleep(std::time::Duration::from_secs(1));
            }
        });
    }

    let icon = eframe::icon_data::from_png_bytes(include_bytes!("../../assets/icon.png")).ok();
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Loom")
        .with_inner_size([1280.0, 800.0])
        .with_min_inner_size([1024.0, 640.0]);
    if let Some(icon) = icon {
        viewport = viewport.with_icon(icon);
    }
    if let Some((x, y)) = host::bench_window() {
        viewport = viewport.with_position([x as f32, y as f32]).with_active(false);
    }
    let mut options = eframe::NativeOptions { viewport, ..Default::default() };
    // eframe defaults to the high-performance (discrete) GPU. Chromium and
    // WebView2 render on the default/integrated one; on a hybrid laptop the
    // dGPU path adds cross-adapter copies, CPU and memory. Match them.
    #[cfg(feature = "wgpu")]
    if let eframe::egui_wgpu::WgpuSetup::CreateNew(setup) = &mut options.wgpu_options.wgpu_setup {
        setup.power_preference = eframe::wgpu::PowerPreference::LowPower;
    }
    #[cfg(all(feature = "glow", not(feature = "wgpu")))]
    {
        options.renderer = eframe::Renderer::Glow;
    }
    eframe::run_native(
        "Loom",
        options,
        Box::new(move |cc| {
            let _ = ctx_slot.set(cc.egui_ctx.clone());
            setup_style(&cc.egui_ctx);
            Ok(Box::new(app::LoomApp::new(live, inbox)))
        }),
    )
}

/// Segoe UI (+ Semibold/Bold), Segoe Fluent Icons and Cascadia Mono, loaded
/// from the system like the web shells do; dark visuals from the tokens.
fn setup_style(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let fonts_dir = std::path::Path::new(&std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into())).join("Fonts");
    let mut load = |key: &str, file: &str| -> bool {
        match std::fs::read(fonts_dir.join(file)) {
            Ok(bytes) => {
                fonts.font_data.insert(key.into(), Arc::new(FontData::from_owned(bytes)));
                true
            }
            Err(_) => false,
        }
    };
    let have_ui = load("segoe", "segoeui.ttf");
    let have_sb = load("segoe-sb", "seguisb.ttf");
    let have_b = load("segoe-b", "segoeuib.ttf");
    let have_icons = load("icons", "SegoeIcons.ttf") || load("icons", "segmdl2.ttf");
    let have_mono = load("mono", "CascadiaMono.ttf");
    let fallback: Vec<String> = fonts.families[&FontFamily::Proportional].clone();
    let family = |primary: &str, have: bool| -> Vec<String> {
        let mut v = if have { vec![primary.to_owned()] } else { vec![] };
        v.extend(fallback.iter().cloned());
        v
    };
    fonts.families.insert(FontFamily::Proportional, family("segoe", have_ui));
    fonts.families.insert(FontFamily::Name("semibold".into()), family("segoe-sb", have_sb));
    fonts.families.insert(FontFamily::Name("bold".into()), family("segoe-b", have_b));
    fonts.families.insert(FontFamily::Name("icons".into()), family("icons", have_icons));
    if have_mono {
        fonts.families.get_mut(&FontFamily::Monospace).unwrap().insert(0, "mono".into());
    }
    ctx.set_fonts(fonts);

    ctx.all_styles_mut(|style| {
        let v = &mut style.visuals;
        *v = egui::Visuals::dark();
        v.panel_fill = theme::BG;
        v.window_fill = theme::PANEL;
        v.extreme_bg_color = theme::PANEL;
        v.override_text_color = Some(theme::TEXT);
        v.selection.bg_fill = theme::ACCENT_SOFT;
        v.selection.stroke = egui::Stroke::new(1.0, theme::ACCENT);
        v.text_cursor.stroke = egui::Stroke::new(1.5, theme::TEXT);
        v.widgets.inactive.bg_fill = theme::SCROLL_THUMB;
        v.widgets.hovered.bg_fill = egui::Color32::from_rgb(0x3b, 0x43, 0x52);
        v.widgets.active.bg_fill = egui::Color32::from_rgb(0x3b, 0x43, 0x52);
        style.spacing.scroll = egui::style::ScrollStyle {
            floating: false,
            bar_width: 8.0,
            bar_inner_margin: 0.0,
            bar_outer_margin: 0.0,
            ..egui::style::ScrollStyle::solid()
        };
    });
}
