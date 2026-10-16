//! Design tokens — the same values as the `:root` block in ui/style.css.

use eframe::egui::{Color32, FontFamily, FontId};

pub const BG: Color32 = Color32::from_rgb(0x0e, 0x10, 0x14);
pub const SIDEBAR: Color32 = Color32::from_rgb(0x12, 0x15, 0x1b);
pub const PANEL: Color32 = Color32::from_rgb(0x17, 0x1b, 0x22);
pub const PANEL_2: Color32 = Color32::from_rgb(0x1d, 0x22, 0x2b);
pub const HOVER: Color32 = Color32::from_rgb(0x23, 0x29, 0x35);
pub const BORDER: Color32 = Color32::from_rgb(0x27, 0x2e, 0x3a);
/// `rgba(39, 46, 58, 0.5)` row separators, pre-blended over BG.
pub const BORDER_SOFT: Color32 = Color32::from_rgb(0x1b, 0x1f, 0x27);
pub const TEXT: Color32 = Color32::from_rgb(0xe7, 0xea, 0xf0);
pub const MUTED: Color32 = Color32::from_rgb(0x8a, 0x93, 0xa5);
pub const FAINT: Color32 = Color32::from_rgb(0x5d, 0x65, 0x76);
pub const ACCENT: Color32 = Color32::from_rgb(0x8b, 0x6c, 0xff);
pub const ACCENT_SOFT: Color32 = Color32::from_rgba_premultiplied(22, 17, 41, 41); // rgba(139,108,255,.16)
pub const TEAL: Color32 = Color32::from_rgb(0x2d, 0xd4, 0xbf);
pub const OK: Color32 = Color32::from_rgb(0x34, 0xd3, 0x99);
pub const WARN: Color32 = Color32::from_rgb(0xfb, 0xbf, 0x24);
pub const DANGER: Color32 = Color32::from_rgb(0xf8, 0x71, 0x71);
pub const SCROLL_THUMB: Color32 = Color32::from_rgb(0x2f, 0x36, 0x44);

pub const RADIUS: u8 = 8;
pub const ROW: f32 = 36.0;

/// Badge background: the tone at 14 % alpha.
pub fn soft(c: Color32) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), 36)
}

pub fn regular(size: f32) -> FontId {
    FontId::new(size, FontFamily::Proportional)
}
pub fn semibold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("semibold".into()))
}
pub fn bold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("bold".into()))
}
pub fn icon(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("icons".into()))
}
pub fn mono(size: f32) -> FontId {
    FontId::new(size, FontFamily::Monospace)
}
