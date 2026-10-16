//! Hand-painted widgets mirroring the CSS classes in ui/style.css
//! (`.btn`, `.chip`, `.iconbtn`, `.badge`, `.toggle`, `.card`, `.input`).

use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontId, Frame, Margin, Pos2, Rect, Response, Sense, Stroke, StrokeKind, Ui,
    Vec2,
};

use crate::data::Tone;
use crate::theme::*;

pub fn text_width(ui: &Ui, text: &str, font: FontId) -> f32 {
    ui.painter().layout_no_wrap(text.to_owned(), font, TEXT).size().x
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Normal,
    Primary,
}

/// `.btn` / `.btn.primary`: 32 px tall, 14 px side padding, optional icon.
pub fn button(ui: &mut Ui, icon_glyph: Option<char>, label: &str, kind: Kind) -> Response {
    let font = if kind == Kind::Primary { semibold(13.0) } else { regular(13.0) };
    let icon_w = icon_glyph.map_or(0.0, |_| 13.0 + 8.0);
    let w = 14.0 + icon_w + text_width(ui, label, font.clone()) + 14.0;
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, 32.0), Sense::click());
    let p = ui.painter();
    let (fill, stroke, color) = match kind {
        Kind::Primary => {
            let fill = if resp.hovered() { Color32::from_rgb(0x96, 0x75, 0xff) } else { ACCENT };
            (fill, ACCENT, Color32::WHITE)
        }
        Kind::Normal => (if resp.hovered() { HOVER } else { PANEL_2 }, BORDER, TEXT),
    };
    p.rect_filled(rect, RADIUS, fill);
    p.rect_stroke(rect, RADIUS, Stroke::new(1.0, stroke), StrokeKind::Inside);
    let mut x = rect.left() + 14.0;
    if let Some(g) = icon_glyph {
        p.text(Pos2::new(x, rect.center().y), Align2::LEFT_CENTER, g, icon(13.0), color);
        x += icon_w;
    }
    p.text(Pos2::new(x, rect.center().y), Align2::LEFT_CENTER, label, font, color);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// `.chip`: 26 px pill, accent border + soft fill when active.
pub fn chip(ui: &mut Ui, label: &str, active: bool) -> Response {
    let w = 10.0 + text_width(ui, label, regular(13.0)) + 10.0;
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, 26.0), Sense::click());
    let p = ui.painter();
    if active {
        p.rect_filled(rect, 13, ACCENT_SOFT);
    }
    p.rect_stroke(rect, 13, Stroke::new(1.0, if active { ACCENT } else { BORDER }), StrokeKind::Inside);
    p.text(rect.center(), Align2::CENTER_CENTER, label, regular(13.0), if active { TEXT } else { MUTED });
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// `.iconbtn`: 32×32 square with a Fluent icon.
pub fn icon_button(ui: &mut Ui, glyph: char) -> Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(32.0), Sense::click());
    let p = ui.painter();
    p.rect_filled(rect, RADIUS, if resp.hovered() { HOVER } else { PANEL_2 });
    p.rect_stroke(rect, RADIUS, Stroke::new(1.0, BORDER), StrokeKind::Inside);
    p.text(rect.center(), Align2::CENTER_CENTER, glyph, icon(13.0), TEXT);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

pub fn tone_color(t: Tone) -> Color32 {
    match t {
        Tone::Ok => OK,
        Tone::Warn => WARN,
        Tone::Danger => DANGER,
    }
}

/// `.badge`: 20 px pill, semibold 11 px, tone text over the tone at 14 %.
pub fn paint_badge(ui: &Ui, left_center: Pos2, tone: Tone, label: &str) {
    let font = semibold(11.0);
    let w = 8.0 + text_width(ui, label, font.clone()) + 8.0;
    let rect = Rect::from_min_size(Pos2::new(left_center.x, left_center.y - 10.0), Vec2::new(w, 20.0));
    let c = tone_color(tone);
    ui.painter().rect_filled(rect, 10, soft(c));
    ui.painter().text(rect.center(), Align2::CENTER_CENTER, label, font, c);
}

/// `.toggle`: 28×16 track with a 12 px knob. Returns true when clicked.
pub fn paint_toggle(ui: &Ui, left_center: Pos2, on: bool, id: egui::Id) -> bool {
    let rect = Rect::from_min_size(Pos2::new(left_center.x, left_center.y - 8.0), Vec2::new(28.0, 16.0));
    let resp = ui.interact(rect, id, Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
    let p = ui.painter();
    p.rect_filled(rect, 8, if on { ACCENT } else { BORDER });
    let knob_x = if on { rect.left() + 14.0 + 6.0 } else { rect.left() + 2.0 + 6.0 };
    p.circle_filled(Pos2::new(knob_x, rect.center().y), 6.0, if on { Color32::WHITE } else { MUTED });
    resp.clicked()
}

/// `.card` with an optional `h3` title (11–12 px uppercase muted).
pub fn card<R>(ui: &mut Ui, title: Option<&str>, add: impl FnOnce(&mut Ui) -> R) -> R {
    Frame::new()
        .fill(PANEL)
        .stroke(Stroke::new(1.0, BORDER))
        .corner_radius(CornerRadius::same(RADIUS))
        .inner_margin(Margin { left: 16, right: 16, top: 14, bottom: 14 })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            if let Some(t) = title {
                ui.label(egui::RichText::new(t.to_uppercase()).font(semibold(12.0)).color(MUTED));
                ui.add_space(10.0);
            }
            add(ui)
        })
        .inner
}

/// `.input`: 32 px single-line field with the accent border on focus.
pub fn input(ui: &mut Ui, value: &mut String, hint: &str, width: f32) -> Response {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 32.0), Sense::hover());
    let id = ui.scope_id().with(hint);
    let focused = ui.memory(|m| m.has_focus(id));
    ui.painter().rect_filled(rect, RADIUS, PANEL);
    ui.painter().rect_stroke(rect, RADIUS, Stroke::new(1.0, if focused { ACCENT } else { BORDER }), StrokeKind::Inside);
    let inner = rect.shrink2(Vec2::new(12.0, 0.0));
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(inner).layout(egui::Layout::left_to_right(egui::Align::Center)));
    child.add(
        egui::TextEdit::singleline(value)
            .id(id)
            .hint_text(egui::RichText::new(hint).color(FAINT))
            .font(regular(13.0))
            .text_color(TEXT)
            .frame(Frame::NONE)
            .desired_width(inner.width()),
    )
}
