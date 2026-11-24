use crate::settings::ThemeChoice;
use eframe::egui::{self, Color32, CornerRadius, Stroke, Vec2};
use engram_core::health::Severity;
use engram_core::{MemoryCategory, NodeKind};

pub const ACCENT: Color32 = Color32::from_rgb(168, 120, 85);
pub const ERROR: Color32 = Color32::from_rgb(214, 84, 76);
pub const WARNING: Color32 = Color32::from_rgb(214, 152, 56);
pub const SUCCESS: Color32 = Color32::from_rgb(84, 168, 112);
pub const MUTED: Color32 = Color32::from_rgb(138, 138, 144);
pub const NEUTRAL: Color32 = Color32::from_rgb(120, 128, 138);
pub const SUPERSEDED: Color32 = Color32::from_rgb(112, 112, 118);
pub const INVARIANT: Color32 = Color32::from_rgb(150, 112, 196);
pub const PINNED: Color32 = ACCENT;

const GRAPHITE_DARK: Color32 = Color32::from_rgb(28, 29, 31);
const GRAPHITE_PANEL: Color32 = Color32::from_rgb(34, 35, 38);
const GRAPHITE_FAINT: Color32 = Color32::from_rgb(42, 43, 47);
const PAPER: Color32 = Color32::from_rgb(248, 247, 245);
const PAPER_PANEL: Color32 = Color32::from_rgb(241, 239, 236);
const PAPER_FAINT: Color32 = Color32::from_rgb(233, 231, 227);

pub fn category(c: MemoryCategory) -> Color32 {
    match c {
        MemoryCategory::Invariant => INVARIANT,
        MemoryCategory::Rule => Color32::from_rgb(150, 132, 112),
        MemoryCategory::TechStack => Color32::from_rgb(112, 132, 150),
        MemoryCategory::Convention => Color32::from_rgb(112, 146, 136),
        MemoryCategory::Note => MUTED,
    }
}

pub fn kind(k: NodeKind) -> Color32 {
    match k {
        NodeKind::Module => ACCENT,
        NodeKind::Service => Color32::from_rgb(112, 132, 150),
        NodeKind::Database => Color32::from_rgb(150, 132, 112),
        NodeKind::Library => Color32::from_rgb(112, 146, 136),
        NodeKind::Queue => Color32::from_rgb(146, 118, 138),
        NodeKind::Ui => Color32::from_rgb(132, 122, 156),
        NodeKind::External => MUTED,
    }
}

pub fn severity(s: Severity) -> Color32 {
    match s {
        Severity::Error => ERROR,
        Severity::Warning => WARNING,
        Severity::Info => NEUTRAL,
    }
}

pub fn score(n: u8) -> Color32 {
    match n {
        85.. => SUCCESS,
        60.. => WARNING,
        _ => ERROR,
    }
}

pub fn apply(ctx: &egui::Context, choice: ThemeChoice) {
    ctx.set_theme(match choice {
        ThemeChoice::System => egui::ThemePreference::System,
        ThemeChoice::Light => egui::ThemePreference::Light,
        ThemeChoice::Dark => egui::ThemePreference::Dark,
    });
    for (theme, bg, panel, faint) in [
        (egui::Theme::Dark, GRAPHITE_DARK, GRAPHITE_PANEL, GRAPHITE_FAINT),
        (egui::Theme::Light, PAPER, PAPER_PANEL, PAPER_FAINT),
    ] {
        ctx.style_mut_of(theme, |style| {
            style.spacing.item_spacing = Vec2::new(8.0, 6.0);
            style.spacing.button_padding = Vec2::new(10.0, 4.0);
            style.spacing.interact_size.y = 24.0;
            let v = &mut style.visuals;
            v.panel_fill = panel;
            v.window_fill = bg;
            v.extreme_bg_color = bg;
            v.faint_bg_color = faint;
            v.selection.bg_fill = ACCENT.gamma_multiply(0.8);
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
}

pub fn apply_font(ctx: &egui::Context, mono: Option<&std::path::Path>) -> Result<(), String> {
    let mut fonts = egui::FontDefinitions::default();
    if let Some(path) = mono {
        let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
        fonts
            .font_data
            .insert("custom-mono".into(), egui::FontData::from_owned(bytes).into());
        if let Some(family) = fonts.families.get_mut(&egui::FontFamily::Monospace) {
            family.insert(0, "custom-mono".into());
        }
    }
    ctx.set_fonts(fonts);
    Ok(())
}
