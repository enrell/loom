//! The Loom prototype screen, redrawn natively. Layout numbers come from
//! ui/style.css; behavior mirrors ui/app.js.

use std::sync::{Arc, Mutex};
use std::time::Instant;

use eframe::egui::{
    self, Align, Align2, CentralPanel, Color32, Frame, Layout, Margin, Mesh, Panel, Pos2, Rect, ScrollArea, Sense,
    Shape, Stroke, StrokeKind, Ui, UiBuilder, Vec2,
};
use loom_proto_host as host;

use crate::data::{self, Mod, NxmLink, Tone, fmt_int, fmt_size, status_of};
use crate::theme::*;
use crate::widgets::{self, Kind, button, card, chip, icon_button, input, paint_badge, paint_toggle};

pub const EGUI_VERSION: &str = "0.37.0";
#[cfg(feature = "wgpu")]
pub const RENDERER: &str = "wgpu (DirectX 12)";
#[cfg(not(feature = "wgpu"))]
pub const RENDERER: &str = "glow (OpenGL)";

const I_MODS: char = '\u{E8F1}';
const I_DOWNLOAD: char = '\u{E896}';
const I_GLOBE: char = '\u{E774}';
const I_VIDEO: char = '\u{E714}';
const I_PERF: char = '\u{E9D9}';
const I_PLAY: char = '\u{E768}';
const I_BACK: char = '\u{E72B}';
const I_FWD: char = '\u{E72A}';
const I_RELOAD: char = '\u{E72C}';
const I_OPEN: char = '\u{E8A7}';

#[derive(Clone, Copy, PartialEq, Eq)]
enum Page {
    Mods,
    Downloads,
    Nexus,
    Media,
    Perf,
}

impl Page {
    fn title(self) -> &'static str {
        match self {
            Page::Mods => "Mods",
            Page::Downloads => "Downloads",
            Page::Nexus => "Nexus Mods",
            Page::Media => "Media",
            Page::Perf => "Performance",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Filter {
    All,
    Enabled,
    Disabled,
    Conflicts,
}

/// Live metrics shared with the sampler thread.
#[derive(Default)]
pub struct Live {
    pub now: host::Metrics,
    pub mem: Vec<f64>,
    pub cpu: Vec<f64>,
}

pub struct LoomApp {
    mods: Vec<Mod>,
    visible: Vec<usize>,
    filter: Filter,
    query: String,
    last_query: String,
    selected: usize,
    page: Page,
    links: Vec<NxmLink>,
    nxm_input: String,
    handler_status: String,
    url: String,
    live: Arc<Mutex<Live>>,
    nxm_inbox: Arc<Mutex<Vec<String>>>,
    frames: u64,
    startup_ms: Option<f64>,
    stress: Option<Instant>,
    scroll_offset: f32,
    scroll_dir: f32,
    scroll_max: f32,
}

impl LoomApp {
    pub fn new(live: Arc<Mutex<Live>>, nxm_inbox: Arc<Mutex<Vec<String>>>) -> Self {
        let mods = data::generate();
        let visible = (0..mods.len()).collect();
        Self {
            mods,
            visible,
            filter: Filter::All,
            query: String::new(),
            last_query: String::new(),
            selected: 0,
            page: Page::Mods,
            links: Vec::new(),
            nxm_input: String::new(),
            handler_status: "Not registered by this prototype.".into(),
            url: "https://www.nexusmods.com/games/skyrimspecialedition".into(),
            live,
            nxm_inbox,
            frames: 0,
            startup_ms: None,
            stress: None,
            scroll_offset: 0.0,
            scroll_dir: 1.0,
            scroll_max: 0.0,
        }
    }

    fn apply_filter(&mut self) {
        let q = self.query.to_lowercase();
        self.visible = (0..self.mods.len())
            .filter(|&i| {
                let m = &self.mods[i];
                let f = match self.filter {
                    Filter::All => true,
                    Filter::Enabled => m.enabled,
                    Filter::Disabled => !m.enabled,
                    Filter::Conflicts => m.conflicts > 0,
                };
                f && (q.is_empty() || m.name.to_lowercase().contains(&q))
            })
            .collect();
    }

    fn add_link(&mut self, url: &str, source: &'static str) {
        self.links.insert(0, data::parse_nxm(url, source));
    }

    fn enabled_count(&self) -> usize {
        self.mods.iter().filter(|m| m.enabled).count()
    }

    fn start_scenario(&mut self, scenario: Option<String>) {
        match scenario.as_deref() {
            Some("scroll") => self.stress = Some(Instant::now() + std::time::Duration::from_secs(1_000_000)),
            Some("video") => self.page = Page::Media,
            Some(s) if s.starts_with("page:") => {
                self.page = match &s[5..] {
                    "downloads" => Page::Downloads,
                    "nexus" => Page::Nexus,
                    "media" => Page::Media,
                    "perf" => Page::Perf,
                    _ => Page::Mods,
                }
            }
            _ => {}
        }
    }
}

impl eframe::App for LoomApp {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        self.frames += 1;
        if self.frames == 2 {
            // The first frame has been presented: same point as the web
            // shells' double requestAnimationFrame.
            self.startup_ms = Some(host::mark_ready());
            self.start_scenario(host::bench_scenario());
        }
        let incoming: Vec<String> = std::mem::take(&mut *self.nxm_inbox.lock().unwrap());
        if !incoming.is_empty() {
            for url in incoming {
                self.add_link(&url, "nxm handler");
            }
            self.page = Page::Downloads;
        }

        Panel::left("sidebar")
            .exact_size(232.0)
            .resizable(false)
            .show_separator_line(false)
            .frame(Frame::new().fill(SIDEBAR).inner_margin(Margin { left: 12, right: 12, top: 16, bottom: 16 }))
            .show(ui, |ui| self.sidebar(ui));
        Panel::bottom("status")
            .exact_size(28.0)
            .resizable(false)
            .show_separator_line(false)
            .frame(Frame::new().fill(SIDEBAR))
            .show(ui, |ui| self.status(ui));
        CentralPanel::default().frame(Frame::new().fill(BG)).show(ui, |ui| {
            let full = ui.max_rect();
            let top = Rect::from_min_size(full.min, Vec2::new(full.width(), 56.0));
            self.topbar(ui, top);
            ui.painter().hline(full.x_range(), top.bottom() - 0.5, Stroke::new(1.0, BORDER));
            let body = Rect::from_min_max(Pos2::new(full.left(), top.bottom()), full.max);
            let mut child = ui.new_child(UiBuilder::new().max_rect(body).layout(Layout::top_down(Align::Min)));
            match self.page {
                Page::Mods => self.mods_page(&mut child, body),
                Page::Downloads => self.downloads_page(&mut child, body),
                Page::Nexus => self.nexus_page(&mut child, body),
                Page::Media => self.media_page(&mut child, body),
                Page::Perf => self.perf_page(&mut child, body),
            }
        });
        if self.stress.is_some_and(|until| Instant::now() < until) {
            ui.ctx().request_repaint();
        } else {
            self.stress = None;
        }
    }
}

// ------------------------------------------------------------------ chrome
impl LoomApp {
    fn sidebar(&mut self, ui: &mut Ui) {
        let r = ui.max_rect();
        // border-right of the sidebar (outside the inner margin)
        ui.painter().vline(r.right() + 12.0 - 0.5, (r.top() - 16.0)..=(r.bottom() + 16.0), Stroke::new(1.0, BORDER));
        let p = ui.painter().clone();

        // brand
        // `.brand`: 4 px top padding, a 36 px text block (16 px name + 11 px
        // sub at normal line height) with the 28 px mark centered on it.
        let block_top = r.top() + 4.0;
        let mark = Rect::from_min_size(Pos2::new(r.left() + 8.0, block_top + 4.0), Vec2::splat(28.0));
        gradient_rect(&p, mark, 8.0, ACCENT, TEAL);
        p.text(mark.center(), Align2::CENTER_CENTER, "L", bold(13.0), BG);
        let tx = mark.right() + 10.0;
        p.text(Pos2::new(tx, block_top + 10.5), Align2::LEFT_CENTER, "Loom", semibold(16.0), TEXT);
        p.text(Pos2::new(tx, block_top + 28.5), Align2::LEFT_CENTER, "egui prototype", regular(11.0), MUTED);
        ui.add_space(4.0 + 36.0 + 20.0);

        let nav_label = |ui: &mut Ui, text: &str| {
            let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 14.0 + 15.0 + 6.0), Sense::hover());
            ui.painter().text(
                Pos2::new(rect.left() + 10.0, rect.top() + 14.0 + 7.5),
                Align2::LEFT_CENTER,
                text.to_uppercase(),
                regular(11.0),
                FAINT,
            );
        };
        ui.spacing_mut().item_spacing = Vec2::new(0.0, 2.0);
        nav_label(ui, "Library");
        let mods_count = fmt_int(self.mods.len());
        let dl_count = self.links.len().to_string();
        self.nav_button(ui, Page::Mods, I_MODS, "Mods", Some(&mods_count));
        self.nav_button(ui, Page::Downloads, I_DOWNLOAD, "Downloads", Some(&dl_count));
        nav_label(ui, "Discover");
        self.nav_button(ui, Page::Nexus, I_GLOBE, "Nexus Mods", None);
        self.nav_button(ui, Page::Media, I_VIDEO, "Media", None);
        nav_label(ui, "Diagnostics");
        self.nav_button(ui, Page::Perf, I_PERF, "Performance", None);

        // profile card pinned to the bottom
        let foot = Rect::from_min_max(Pos2::new(r.left(), r.bottom() - 72.0), r.max);
        p.rect_filled(foot, RADIUS, PANEL);
        p.rect_stroke(foot, RADIUS, Stroke::new(1.0, BORDER), StrokeKind::Inside);
        let fx = foot.left() + 11.0;
        p.text(Pos2::new(fx, foot.top() + 18.0), Align2::LEFT_CENTER, "Active profile", regular(11.0), FAINT);
        p.text(Pos2::new(fx, foot.top() + 37.0), Align2::LEFT_CENTER, "Skyrim Special Edition", semibold(13.0), TEXT);
        p.text(Pos2::new(fx, foot.top() + 55.0), Align2::LEFT_CENTER, "default", regular(11.0), FAINT);
    }

    fn nav_button(&mut self, ui: &mut Ui, page: Page, glyph: char, label: &str, count: Option<&str>) {
        let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 34.0), Sense::click());
        let active = self.page == page;
        let p = ui.painter();
        if active {
            p.rect_filled(rect, RADIUS, ACCENT_SOFT);
        } else if resp.hovered() {
            p.rect_filled(rect, RADIUS, HOVER);
        }
        let fg = if active || resp.hovered() { TEXT } else { MUTED };
        let ico = if active { ACCENT } else { fg };
        p.text(Pos2::new(rect.left() + 10.0 + 9.0, rect.center().y), Align2::CENTER_CENTER, glyph, icon(15.0), ico);
        p.text(Pos2::new(rect.left() + 10.0 + 18.0 + 10.0, rect.center().y), Align2::LEFT_CENTER, label, regular(13.0), fg);
        if let Some(c) = count {
            p.text(Pos2::new(rect.right() - 10.0, rect.center().y), Align2::RIGHT_CENTER, c, regular(11.0), FAINT);
        }
        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            self.page = page;
        }
    }

    fn topbar(&mut self, ui: &mut Ui, rect: Rect) {
        let p = ui.painter().clone();
        let cy = rect.center().y;
        let title = p.text(Pos2::new(rect.left() + 20.0, cy), Align2::LEFT_CENTER, self.page.title(), semibold(16.0), TEXT);
        if self.page == Page::Mods {
            let sub = format!("{} of {}", fmt_int(self.visible.len()), fmt_int(self.mods.len()));
            p.text(Pos2::new(title.right() + 12.0, cy), Align2::LEFT_CENTER, sub, regular(13.0), MUTED);
        }
        let right = Rect::from_min_max(Pos2::new(rect.left(), rect.top() + 12.0), Pos2::new(rect.right() - 20.0, rect.bottom() - 12.0));
        let mut row = ui.new_child(UiBuilder::new().max_rect(right).layout(Layout::right_to_left(Align::Center)));
        row.spacing_mut().item_spacing.x = 12.0;
        button(&mut row, Some(I_PLAY), "Launch game", Kind::Primary);
        if self.page == Page::Mods {
            input(&mut row, &mut self.query, "Search mods", 240.0);
            if self.query != self.last_query {
                self.last_query = self.query.clone();
                self.apply_filter();
            }
        }
    }

    fn status(&mut self, ui: &mut Ui) {
        let r = ui.max_rect();
        let p = ui.painter().clone();
        p.hline(r.x_range(), r.top() + 0.5, Stroke::new(1.0, BORDER));
        let cy = r.center().y;
        let mut x = r.left() + 20.0;
        p.circle_filled(Pos2::new(x + 3.5, cy), 3.5, OK);
        x += 13.0;
        let t = p.text(Pos2::new(x, cy), Align2::LEFT_CENTER, format!("egui {EGUI_VERSION}"), semibold(12.0), TEXT);
        x = t.right() + 18.0;
        let t = p.text(Pos2::new(x, cy), Align2::LEFT_CENTER, "Skyrim Special Edition · default", regular(12.0), MUTED);
        x = t.right() + 18.0;
        let enabled = format!("{} of {} enabled", fmt_int(self.enabled_count()), fmt_int(self.mods.len()));
        p.text(Pos2::new(x, cy), Align2::LEFT_CENTER, enabled, regular(12.0), MUTED);

        let now = self.live.lock().unwrap().now;
        let mut x = r.right() - 20.0;
        let t = p.text(Pos2::new(x, cy), Align2::RIGHT_CENTER, format!("{:.1}%", now.cpu_percent), semibold(12.0), TEXT);
        let t = p.text(Pos2::new(t.left() - 4.0, cy), Align2::RIGHT_CENTER, "CPU", regular(12.0), MUTED);
        x = t.left() - 18.0;
        let t = p.text(Pos2::new(x, cy), Align2::RIGHT_CENTER, format!("{:.0} MB", now.working_set_mb), semibold(12.0), TEXT);
        p.text(Pos2::new(t.left() - 4.0, cy), Align2::RIGHT_CENTER, "MEM", regular(12.0), MUTED);
    }
}

// ------------------------------------------------------------------ pages
impl LoomApp {
    fn mods_page(&mut self, ui: &mut Ui, body: Rect) {
        let details = Rect::from_min_max(Pos2::new(body.right() - 320.0, body.top()), body.max);
        let list = Rect::from_min_max(body.min, Pos2::new(details.left(), body.bottom()));
        ui.painter().vline(details.left() - 0.5, body.y_range(), Stroke::new(1.0, BORDER));

        // chips
        let chips = Rect::from_min_size(list.min, Vec2::new(list.width(), 51.0)); // 12 + 26 + 12 + 1 px border
        let mut row = ui.new_child(UiBuilder::new().max_rect(Rect::from_min_size(chips.min + Vec2::new(20.0, 12.0), Vec2::new(chips.width() - 40.0, 26.0))).layout(Layout::left_to_right(Align::Center)));
        row.spacing_mut().item_spacing.x = 6.0;
        for (f, label) in [(Filter::All, "All"), (Filter::Enabled, "Enabled"), (Filter::Disabled, "Disabled"), (Filter::Conflicts, "Conflicts")] {
            if chip(&mut row, label, self.filter == f).clicked() {
                self.filter = f;
                self.apply_filter();
            }
        }
        ui.painter().hline(list.x_range(), chips.bottom() - 0.5, Stroke::new(1.0, BORDER));

        // header
        let head = Rect::from_min_size(Pos2::new(list.left(), chips.bottom()), Vec2::new(list.width(), 33.0)); // 32 + border
        let cols = columns(head.left() + 20.0, head.right() - 28.0);
        for (i, h) in ["", "#", "Name", "Version", "Category", "Size", "Status"].iter().enumerate() {
            ui.painter().text(Pos2::new(cols[i], head.top() + 16.0), Align2::LEFT_CENTER, h.to_uppercase(), regular(11.0), FAINT);
        }
        ui.painter().hline(list.x_range(), head.bottom() - 0.5, Stroke::new(1.0, BORDER));

        // rows
        let view = Rect::from_min_max(Pos2::new(list.left(), head.bottom()), list.max);
        let mut vui = ui.new_child(UiBuilder::new().max_rect(view));
        vui.spacing_mut().item_spacing = Vec2::ZERO;
        let mut area = ScrollArea::vertical().auto_shrink(false).id_salt("mods");
        if self.stress.is_some() {
            self.scroll_offset += 48.0 * self.scroll_dir;
            if self.scroll_offset >= self.scroll_max {
                self.scroll_dir = -1.0;
            }
            if self.scroll_offset <= 0.0 {
                self.scroll_dir = 1.0;
            }
            self.scroll_offset = self.scroll_offset.clamp(0.0, self.scroll_max.max(0.0));
            area = area.vertical_scroll_offset(self.scroll_offset);
        }
        let mut toggled = None;
        let mut clicked = None;
        let out = area.show_rows(&mut vui, ROW, self.visible.len(), |ui, range| {
            for vi in range {
                let mi = self.visible[vi];
                let m = &self.mods[mi];
                let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), ROW), Sense::click());
                let p = ui.painter();
                if m.priority == self.selected {
                    p.rect_filled(rect, 0, ACCENT_SOFT);
                } else if resp.hovered() {
                    p.rect_filled(rect, 0, PANEL);
                }
                p.hline(rect.x_range(), rect.bottom() - 0.5, Stroke::new(1.0, BORDER_SOFT));
                let c = columns(rect.left() + 20.0, rect.right() - 20.0);
                let cy = rect.center().y;
                if paint_toggle(ui, Pos2::new(c[0], cy), m.enabled, ui.scope_id().with(("toggle", mi))) {
                    toggled = Some(mi);
                }
                let p = ui.painter();
                p.text(Pos2::new(c[1], cy), Align2::LEFT_CENTER, m.priority, regular(13.0), MUTED);
                let name_w = c[3] - c[2] - 12.0;
                let galley = ui.painter().layout(m.name.clone(), regular(13.0), TEXT, f32::INFINITY);
                let name = if galley.size().x > name_w { elide(ui, &m.name, name_w) } else { m.name.clone() };
                p.text(Pos2::new(c[2], cy), Align2::LEFT_CENTER, name, regular(13.0), TEXT);
                p.text(Pos2::new(c[3], cy), Align2::LEFT_CENTER, &m.version, regular(13.0), MUTED);
                p.text(Pos2::new(c[4], cy), Align2::LEFT_CENTER, m.category, regular(13.0), MUTED);
                p.text(Pos2::new(c[5], cy), Align2::LEFT_CENTER, fmt_size(m.size_mb), regular(13.0), MUTED);
                let (tone, label) = status_of(m);
                paint_badge(ui, Pos2::new(c[6], cy), tone, &label);
                if resp.clicked() {
                    clicked = Some(mi);
                }
            }
        });
        self.scroll_max = (out.content_size.y - out.inner_rect.height()).max(0.0);
        if self.stress.is_none() {
            self.scroll_offset = out.state.clamped_offset().y;
        }
        if let Some(mi) = toggled {
            self.mods[mi].enabled = !self.mods[mi].enabled;
        }
        if let Some(mi) = clicked {
            self.selected = mi;
        }

        // details
        let mut dui = ui.new_child(UiBuilder::new().max_rect(details.shrink(20.0)).layout(Layout::top_down(Align::Min)));
        self.details(&mut dui);
    }

    fn details(&mut self, ui: &mut Ui) {
        let m = &self.mods[self.selected];
        ui.spacing_mut().item_spacing = Vec2::ZERO;
        ui.label(egui::RichText::new(&m.name).font(semibold(15.0)).color(TEXT));
        ui.add_space(4.0);
        ui.label(egui::RichText::new(format!("by {} · v{}", m.author, m.version)).font(regular(13.0)).color(MUTED));
        ui.add_space(16.0);
        let (tone, label) = status_of(m);
        let rows: [(&str, String); 6] = [
            ("Priority", m.priority.to_string()),
            ("Category", m.category.to_owned()),
            ("Size", fmt_size(m.size_mb)),
            ("Endorsements", fmt_int(m.endorsements as usize)),
            ("Status", String::new()),
            ("State", if m.enabled { "Enabled".into() } else { "Disabled".into() }),
        ];
        card(ui, None, |ui| {
            for (i, (k, v)) in rows.iter().enumerate() {
                if i > 0 {
                    ui.add_space(10.0);
                }
                // grid row: 17 px text line, 20 px when it holds a badge
                let h = if *k == "Status" { 20.0 } else { 17.0 };
                let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), h), Sense::hover());
                ui.painter().text(r.left_center(), Align2::LEFT_CENTER, *k, regular(13.0), FAINT);
                let vpos = Pos2::new(r.left() + 100.0, r.center().y);
                if *k == "Status" {
                    paint_badge(ui, vpos, tone, &label);
                } else {
                    ui.painter().text(vpos, Align2::LEFT_CENTER, v, regular(13.0), TEXT);
                }
            }
        });
        ui.add_space(12.0);
        let enabled = m.enabled;
        let name = m.name.clone();
        let selected = self.selected;
        let mut go_nexus = false;
        card(ui, Some("Actions"), |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                if button(ui, None, if enabled { "Disable" } else { "Enable" }, Kind::Normal).clicked() {
                    self.mods[selected].enabled = !enabled;
                }
                go_nexus = button(ui, Some(I_GLOBE), "Find on Nexus", Kind::Normal).clicked();
            });
        });
        if go_nexus {
            self.url = format!("https://www.nexusmods.com/games/skyrimspecialedition/mods?keyword={}", name.replace(' ', "%20"));
            self.page = Page::Nexus;
        }
    }

    fn downloads_page(&mut self, ui: &mut Ui, body: Rect) {
        let mut c = content(ui, body);
        c.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            let buttons_w: f32 = ["Add", "Simulate link", "Register as nxm handler"]
                .iter()
                .map(|l| 8.0 + 14.0 + widgets::text_width(ui, l, regular(13.0)) + 14.0)
                .sum();
            let w = ui.available_width() - buttons_w;
            input(ui, &mut self.nxm_input, "Paste an nxm:// link", w);
            if button(ui, None, "Add", Kind::Normal).clicked() && !self.nxm_input.is_empty() {
                let url = std::mem::take(&mut self.nxm_input);
                self.add_link(&url, "pasted");
            }
            if button(ui, None, "Simulate link", Kind::Normal).clicked() {
                let m = 266 + self.links.len();
                let exp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() + 3600;
                let url = format!("nxm://skyrimspecialedition/mods/{m}/files/{}?key=demo&expires={exp}&user_id=1", 100_000 + m);
                self.add_link(&url, "simulated");
            }
            if button(ui, None, "Register as nxm handler", Kind::Normal).clicked() {
                let exe = std::env::current_exe().unwrap_or_default();
                self.handler_status = host::register_nxm(&exe).unwrap_or_else(|e| format!("Registration failed: {e}"));
            }
        });
        c.add_space(12.0);
        let links = &self.links;
        card(&mut c, Some("Received links"), |ui| {
            let rows: Vec<[String; 5]> = links
                .iter()
                .map(|l| [l.game.clone(), l.module.clone(), l.file.clone(), l.expires.clone(), l.source.to_owned()])
                .collect();
            table(ui, &["Game", "Mod", "File", "Expires", "Source"], &rows, |ui, r, col, text| {
                let font = if col == 0 { mono(12.0) } else { regular(13.0) };
                ui.painter().text(r.left_center(), Align2::LEFT_CENTER, text, font, TEXT);
            });
            if links.is_empty() {
                let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 28.0 + 18.0 + 28.0), Sense::hover());
                ui.painter().text(
                    r.center(),
                    Align2::CENTER_CENTER,
                    "No nxm:// links yet. Click “Mod Manager Download” on Nexus Mods, or simulate one.",
                    regular(13.0),
                    MUTED,
                );
            }
        });
        c.add_space(12.0);
        let status = self.handler_status.clone();
        card(&mut c, Some("Handler"), |ui| {
            ui.label(egui::RichText::new(status).font(regular(13.0)).color(MUTED));
        });
    }

    fn nexus_page(&mut self, ui: &mut Ui, body: Rect) {
        let bar = Rect::from_min_size(body.min, Vec2::new(body.width(), 52.0));
        let mut row = ui.new_child(UiBuilder::new().max_rect(bar.shrink2(Vec2::new(20.0, 10.0))).layout(Layout::left_to_right(Align::Center)));
        row.spacing_mut().item_spacing.x = 8.0;
        icon_button(&mut row, I_BACK);
        icon_button(&mut row, I_FWD);
        icon_button(&mut row, I_RELOAD);
        let ext_w = 14.0 + 21.0 + widgets::text_width(&row, "Open in browser", regular(13.0)) + 14.0;
        let w = row.available_width() - ext_w - 8.0;
        input(&mut row, &mut self.url, "url", w);
        if button(&mut row, Some(I_OPEN), "Open in browser", Kind::Normal).clicked() {
            let _ = std::process::Command::new("rundll32").args(["url.dll,FileProtocolHandler", &self.url]).spawn();
        }
        ui.painter().hline(body.x_range(), bar.bottom() - 0.5, Stroke::new(1.0, BORDER));
        let host_rect = Rect::from_min_max(Pos2::new(body.left(), bar.bottom()), body.max);
        ui.painter().rect_filled(host_rect, 0, PANEL);
        placeholder(
            ui,
            host_rect.center(),
            "No embedded browser in this shell",
            "Use “Open in browser”; nxm:// links still arrive in Downloads.",
        );
    }

    fn media_page(&mut self, ui: &mut Ui, body: Rect) {
        let mut c = content(ui, body);
        let w = (c.available_width() - 12.0) / 2.0;
        let video_h = (w - 32.0) * 9.0 / 16.0;
        let card_h = 22.0 + video_h;
        c.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 12.0;
            cell(ui, w, |ui| {
                card(ui, Some("Local video · H.264 / AAC"), |ui| {
                    let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), video_h), Sense::hover());
                    ui.painter().rect_filled(r, RADIUS, Color32::BLACK);
                    placeholder(ui, r.center(), "No video playback in egui", "Needs a native decoder (ffmpeg / GStreamer bindings).");
                });
            });
            cell(ui, w, |ui| {
                card(ui, Some("Codec support"), |ui| {
                    ui.set_min_height(card_h - 22.0);
                    let rows: Vec<[String; 2]> = ["H.264 / AVC", "H.265 / HEVC", "VP9", "AV1", "AAC", "Opus"]
                        .iter()
                        .map(|n| [(*n).to_owned(), "No".to_owned()])
                        .collect();
                    table(ui, &["Codec", "Support"], &rows, |ui, r, col, text| {
                        if col == 1 {
                            paint_badge(ui, r.left_center(), Tone::Danger, text);
                        } else {
                            ui.painter().text(r.left_center(), Align2::LEFT_CENTER, text, regular(13.0), TEXT);
                        }
                    });
                });
            });
        });
        c.add_space(12.0);
        card(&mut c, Some("Streaming"), |ui| {
            if button(ui, None, "Open a mod showcase on YouTube in the Nexus tab", Kind::Normal).clicked() {
                self.url = "https://www.youtube.com/results?search_query=skyrim+mod+showcase".into();
                self.page = Page::Nexus;
            }
        });
    }

    fn perf_page(&mut self, ui: &mut Ui, body: Rect) {
        let mut c = content(ui, body);
        let (now, mem, cpu) = {
            let l = self.live.lock().unwrap();
            (l.now, l.mem.clone(), l.cpu.clone())
        };
        let startup = self.startup_ms.map_or_else(|| "–".into(), |ms| format!("{ms:.0}"));
        let stats = [
            ("Startup", startup, "ms"),
            ("Memory (process tree)", format!("{:.0}", now.working_set_mb), "MB"),
            ("CPU", format!("{:.1}", now.cpu_percent), "%"),
            ("Processes", now.processes.to_string(), ""),
        ];
        let w4 = (c.available_width() - 36.0) / 4.0;
        c.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 12.0;
            for (k, v, u) in &stats {
                cell(ui, w4, |ui| {
                    card(ui, None, |ui| {
                        ui.label(egui::RichText::new(*k).font(regular(13.0)).color(MUTED));
                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 4.0;
                            ui.label(egui::RichText::new(v).font(semibold(26.0)).color(TEXT));
                            ui.label(egui::RichText::new(*u).font(regular(13.0)).color(MUTED));
                        });
                    });
                });
            }
        });
        c.add_space(12.0);
        let w2 = (c.available_width() - 12.0) / 2.0;
        c.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 12.0;
            for (title, data, color, floor) in [("Memory · last 60 s", &mem, ACCENT, 100.0), ("CPU · last 60 s", &cpu, TEAL, 10.0)] {
                cell(ui, w2, |ui| {
                    card(ui, Some(title), |ui| {
                        let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 120.0), Sense::hover());
                        sparkline(ui, r, data, color, floor);
                    });
                });
            }
        });
        c.add_space(12.0);
        c.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 12.0;
            cell(ui, w2, |ui| {
                card(ui, Some("Scenarios"), |ui| {
                    ui.set_min_height(3.0 * 17.0 + 2.0 * 10.0); // the Shell card's three kv rows
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 8.0;
                        if button(ui, None, "Scroll stress · 10 s", Kind::Normal).clicked() {
                            self.page = Page::Mods;
                            self.stress = Some(Instant::now() + std::time::Duration::from_secs(10));
                        }
                        if button(ui, None, "Play video", Kind::Normal).clicked() {
                            self.page = Page::Media;
                        }
                    });
                });
            });
            cell(ui, w2, |ui| {
                card(ui, Some("Shell"), |ui| {
                    let rows = [("Shell", format!("egui {EGUI_VERSION}")), ("Engine", "egui (immediate mode)".into()), ("Renderer", RENDERER.into())];
                    for (i, (k, v)) in rows.iter().enumerate() {
                        if i > 0 {
                            ui.add_space(10.0);
                        }
                        let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 17.0), Sense::hover());
                        ui.painter().text(r.left_center(), Align2::LEFT_CENTER, *k, regular(13.0), FAINT);
                        ui.painter().text(Pos2::new(r.left() + 100.0, r.center().y), Align2::LEFT_CENTER, v, regular(13.0), TEXT);
                    }
                });
            });
        });
        ui.ctx().request_repaint_after(std::time::Duration::from_secs(1));
    }
}

// ------------------------------------------------------------------ helpers

/// Column x positions for `grid-template-columns: 44px 56px 1fr 90px 120px 84px 96px`.
fn columns(left: f32, right: f32) -> [f32; 7] {
    let fixed = [44.0, 56.0, 0.0, 90.0, 120.0, 84.0, 96.0];
    let flex = (right - left) - fixed.iter().sum::<f32>();
    let mut x = left;
    let mut out = [0.0; 7];
    for (i, w) in fixed.iter().enumerate() {
        out[i] = x;
        x += if i == 2 { flex } else { *w };
    }
    out
}

fn elide(ui: &Ui, text: &str, max_w: f32) -> String {
    let mut s: String = text.to_owned();
    while !s.is_empty() && widgets::text_width(ui, &format!("{s}…"), regular(13.0)) > max_w {
        s.pop();
    }
    format!("{}…", s.trim_end())
}

/// One CSS grid cell: a fixed-width, top-down column inside a row.
fn cell<R>(ui: &mut Ui, w: f32, add: impl FnOnce(&mut Ui) -> R) -> R {
    ui.allocate_ui_with_layout(Vec2::new(w, 0.0), Layout::top_down(Align::Min), add).inner
}

/// `.content`: 20 px padding, vertical flow.
fn content(ui: &mut Ui, body: Rect) -> Ui {
    let mut c = ui.new_child(UiBuilder::new().max_rect(body.shrink(20.0)).layout(Layout::top_down(Align::Min)));
    c.spacing_mut().item_spacing = Vec2::ZERO;
    c
}

fn placeholder(ui: &Ui, center: Pos2, title: &str, line: &str) {
    let p = ui.painter();
    p.text(center - Vec2::new(0.0, 10.0), Align2::CENTER_CENTER, title, semibold(15.0), TEXT);
    p.text(center + Vec2::new(0.0, 12.0), Align2::CENTER_CENTER, line, regular(13.0), MUTED);
}

/// `.table` with `table-layout: fixed`: equal columns, 11 px uppercase
/// header, 9 px cell padding, soft row separators.
fn table<const N: usize>(ui: &mut Ui, head: &[&str; N], rows: &[[String; N]], mut cell: impl FnMut(&Ui, Rect, usize, &str)) {
    let w = ui.available_width();
    let cw = w / N as f32;
    let (hr, _) = ui.allocate_exact_size(Vec2::new(w, 32.0), Sense::hover());
    for (i, h) in head.iter().enumerate() {
        let x = hr.left() + cw * i as f32 + 10.0;
        ui.painter().text(Pos2::new(x, hr.center().y), Align2::LEFT_CENTER, h.to_uppercase(), semibold(11.0), FAINT);
    }
    ui.painter().hline(hr.x_range(), hr.bottom() - 0.5, Stroke::new(1.0, BORDER));
    for row in rows {
        let (rr, _) = ui.allocate_exact_size(Vec2::new(w, 39.0), Sense::hover());
        for (i, text) in row.iter().enumerate() {
            let r = Rect::from_min_size(Pos2::new(rr.left() + cw * i as f32 + 10.0, rr.top()), Vec2::new(cw - 20.0, rr.height()));
            cell(ui, r, i, text);
        }
        ui.painter().hline(rr.x_range(), rr.bottom() - 0.5, Stroke::new(1.0, BORDER_SOFT));
    }
}

/// The brand mark's 135° violet→teal gradient.
fn gradient_rect(p: &egui::Painter, r: Rect, _radius: f32, a: Color32, b: Color32) {
    let mid = a.lerp_to_gamma(b, 0.5);
    let mut mesh = Mesh::default();
    mesh.colored_vertex(r.left_top(), a);
    mesh.colored_vertex(r.right_top(), mid);
    mesh.colored_vertex(r.right_bottom(), b);
    mesh.colored_vertex(r.left_bottom(), mid);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    p.add(Shape::mesh(mesh));
    // round the corners by painting the sidebar color over them
    let k = 8.0;
    for (corner, dir) in [
        (r.left_top(), Vec2::new(1.0, 1.0)),
        (r.right_top(), Vec2::new(-1.0, 1.0)),
        (r.right_bottom(), Vec2::new(-1.0, -1.0)),
        (r.left_bottom(), Vec2::new(1.0, -1.0)),
    ] {
        let c = corner + dir * k;
        let mut pts = vec![corner];
        for s in 0..=8 {
            let t = s as f32 / 8.0 * std::f32::consts::FRAC_PI_2;
            pts.push(c - Vec2::new(dir.x * k * t.cos(), dir.y * k * t.sin()));
        }
        p.add(Shape::convex_polygon(pts, SIDEBAR, Stroke::NONE));
    }
}

/// Filled area + 2 px line, newest sample on the right, 3 grid lines.
fn sparkline(ui: &Ui, r: Rect, data: &[f64], color: Color32, floor: f64) {
    let p = ui.painter();
    for i in 1..4 {
        let y = (r.top() + r.height() * i as f32 / 4.0).round() + 0.5;
        p.hline(r.x_range(), y, Stroke::new(1.0, BORDER));
    }
    if data.len() < 2 {
        return;
    }
    let top = data.iter().copied().fold(floor, f64::max) * 1.15;
    let pts: Vec<Pos2> = data
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let x = r.left() + (i as f32 / 59.0) * r.width();
            Pos2::new(x, r.bottom() - (*v / top) as f32 * r.height())
        })
        .collect();
    let off = r.right() - pts.last().unwrap().x;
    let pts: Vec<Pos2> = pts.into_iter().map(|q| q + Vec2::new(off, 0.0)).collect();
    let fill = Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 0x22);
    let mut mesh = Mesh::default();
    for q in &pts {
        mesh.colored_vertex(*q, fill);
        mesh.colored_vertex(Pos2::new(q.x, r.bottom()), fill);
    }
    for i in 0..pts.len() as u32 - 1 {
        let (a, b, c, d) = (2 * i, 2 * i + 1, 2 * i + 2, 2 * i + 3);
        mesh.add_triangle(a, b, c);
        mesh.add_triangle(b, d, c);
    }
    p.add(Shape::mesh(mesh));
    p.add(Shape::line(pts, Stroke::new(2.0, color)));
}
