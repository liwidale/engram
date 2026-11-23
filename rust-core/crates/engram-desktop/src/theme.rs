use eframe::egui::{self, Color32, CornerRadius, Stroke, Vec2};
use engram_core::health::Severity;
use engram_core::{MemoryCategory, NodeKind};

pub const ACCENT: Color32 = Color32::from_rgb(97, 102, 242);
pub const ERROR: Color32 = Color32::from_rgb(235, 77, 77);
pub const WARNING: Color32 = Color32::from_rgb(242, 158, 38);
pub const INFO: Color32 = Color32::from_rgb(64, 148, 245);
pub const SUCCESS: Color32 = Color32::from_rgb(51, 184, 115);
pub const MUTED: Color32 = Color32::from_rgb(140, 140, 150);

pub fn category(c: MemoryCategory) -> Color32 {
    match c {
        MemoryCategory::Invariant => Color32::from_rgb(158, 97, 242),
        MemoryCategory::Rule => Color32::from_rgb(237, 107, 84),
        MemoryCategory::TechStack => INFO,
        MemoryCategory::Convention => Color32::from_rgb(41, 179, 158),
        MemoryCategory::Note => MUTED,
    }
}

pub fn kind(k: NodeKind) -> Color32 {
    match k {
        NodeKind::Module => ACCENT,
        NodeKind::Service => INFO,
        NodeKind::Database => WARNING,
        NodeKind::Library => Color32::from_rgb(41, 179, 158),
        NodeKind::Queue => Color32::from_rgb(217, 97, 158),
        NodeKind::Ui => Color32::from_rgb(158, 97, 242),
        NodeKind::External => MUTED,
    }
}

pub fn severity(s: Severity) -> Color32 {
    match s {
        Severity::Error => ERROR,
        Severity::Warning => WARNING,
        Severity::Info => INFO,
    }
}

pub fn score(n: u8) -> Color32 {
    match n {
        85.. => SUCCESS,
        60.. => WARNING,
        _ => ERROR,
    }
}

pub fn apply(ctx: &egui::Context) {
    ctx.all_styles_mut(|style| {
        style.spacing.item_spacing = Vec2::new(8.0, 6.0);
        style.spacing.button_padding = Vec2::new(10.0, 4.0);
        style.spacing.interact_size.y = 24.0;
        let v = &mut style.visuals;
        v.selection.bg_fill = ACCENT.gamma_multiply(0.85);
        v.selection.stroke = Stroke::new(1.0, Color32::WHITE);
        v.hyperlink_color = ACCENT;
        v.window_corner_radius = CornerRadius::same(12);
        v.menu_corner_radius = CornerRadius::same(8);
        for w in [
            &mut v.widgets.noninteractive,
            &mut v.widgets.inactive,
            &mut v.widgets.hovered,
            &mut v.widgets.active,
            &mut v.widgets.open,
        ] {
            w.corner_radius = CornerRadius::same(6);
        }
    });
}
