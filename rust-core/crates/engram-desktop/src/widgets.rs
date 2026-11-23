use crate::i18n::tr;
use crate::theme;
use eframe::egui::{self, Color32, CornerRadius, Margin, RichText, Ui};

pub fn chip(ui: &mut Ui, text: impl Into<String>, color: Color32) -> egui::Response {
    egui::Frame::new()
        .fill(color.gamma_multiply(0.16))
        .corner_radius(CornerRadius::same(9))
        .inner_margin(Margin::symmetric(7, 1))
        .show(ui, |ui| {
            ui.label(RichText::new(text.into()).small().color(color).strong())
        })
        .response
}

pub fn card<R>(ui: &mut Ui, title: Option<&str>, add: impl FnOnce(&mut Ui) -> R) -> R {
    egui::Frame::new()
        .fill(ui.visuals().extreme_bg_color)
        .stroke(ui.visuals().widgets.noninteractive.bg_stroke)
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::same(12))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            if let Some(t) = title {
                ui.label(RichText::new(t.to_uppercase()).small().strong().color(theme::MUTED));
                ui.add_space(2.0);
            }
            add(ui)
        })
        .inner
}

pub fn empty_state(ui: &mut Ui, title: &str, message: &str, action: Option<&str>) -> bool {
    let mut clicked = false;
    ui.vertical_centered(|ui| {
        ui.add_space((ui.available_height() * 0.3).max(24.0));
        ui.label(RichText::new(title).heading().strong());
        ui.add_space(4.0);
        ui.set_max_width(420.0);
        ui.label(RichText::new(message).color(theme::MUTED));
        if let Some(a) = action {
            ui.add_space(10.0);
            clicked = ui.add(primary_button(a)).clicked();
        }
    });
    clicked
}

pub fn primary_button(text: &str) -> egui::Button<'_> {
    egui::Button::new(RichText::new(text).color(Color32::WHITE).strong()).fill(theme::ACCENT)
}

pub fn tokens(ui: &mut Ui, tokens: usize, budget: Option<usize>) {
    let over = budget.is_some_and(|b| tokens > b);
    let text = match budget {
        Some(b) => format!("{tokens} / {b} {}", tr("tokens")),
        None => format!("{tokens} {}", tr("tokens")),
    };
    ui.label(
        RichText::new(text)
            .small()
            .color(if over { theme::ERROR } else { theme::MUTED }),
    );
}

pub fn muted(ui: &mut Ui, text: impl Into<String>) -> egui::Response {
    ui.label(RichText::new(text.into()).small().color(theme::MUTED))
}

pub fn heading(ui: &mut Ui, text: impl Into<String>) {
    ui.label(RichText::new(text.into()).size(18.0).strong());
}

pub fn multiline(ui: &mut Ui, text: &mut String, hint: &str, rows: usize, code: bool) -> egui::Response {
    let mut edit = egui::TextEdit::multiline(text)
        .hint_text(hint)
        .desired_rows(rows)
        .desired_width(f32::INFINITY);
    if code {
        edit = edit.code_editor();
    }
    ui.add(edit)
}

pub fn lines_to_list(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_owned)
        .collect()
}

pub fn relative_time(rfc3339: Option<&str>) -> String {
    let Some(at) = rfc3339.and_then(engram_core::time::parse_rfc3339) else {
        return tr("never").to_owned();
    };
    let secs = (engram_core::time::now_unix() - at).max(0);
    match secs {
        0..=44 => tr("a moment ago").to_owned(),
        45..=3599 => crate::i18n::trf("{n} min ago", &[("n", &(secs / 60).max(1))]),
        3600..=86_399 => crate::i18n::trf("{n} h ago", &[("n", &(secs / 3600))]),
        _ => crate::i18n::trf("{n} d ago", &[("n", &(secs / 86_400))]),
    }
}
