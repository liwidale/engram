use crate::app::Action;
use crate::i18n::tr;
use crate::theme;
use crate::widgets;
use eframe::egui::{self, RichText, Ui};
use std::path::PathBuf;

pub fn show(ui: &mut Ui, recents: &[PathBuf], logo: &egui::TextureHandle) -> Option<Action> {
    let mut action = None;
    ui.vertical_centered(|ui| {
        ui.add_space((ui.available_height() * 0.16).max(20.0));
        ui.add(egui::Image::new(logo).fit_to_exact_size(egui::vec2(88.0, 88.0)));
        ui.add_space(8.0);
        ui.label(RichText::new("Engram").size(30.0).strong());
        ui.label(
            RichText::new(tr(
                "Project memory for you and your AI agents, versioned with your code.",
            ))
            .color(theme::MUTED),
        );
        ui.add_space(18.0);
        ui.horizontal(|ui| {
            let w = 320.0;
            ui.add_space((ui.available_width() - w).max(0.0) / 2.0);
            if ui
                .add_sized([150.0, 32.0], widgets::primary_button(tr("New project memory")))
                .clicked()
            {
                action = Some(Action::New);
            }
            if ui
                .add_sized([150.0, 32.0], egui::Button::new(tr("Open file")))
                .clicked()
            {
                action = Some(Action::Open);
            }
        });
        ui.add_space(6.0);
        widgets::muted(
            ui,
            tr("Ctrl+N new · Ctrl+O open · Ctrl+K commands · drop a .engram file anywhere"),
        );
        let existing: Vec<&PathBuf> = recents.iter().filter(|p| p.exists()).collect();
        if !existing.is_empty() {
            ui.add_space(24.0);
            ui.label(
                RichText::new(tr("Recent").to_uppercase())
                    .small()
                    .strong()
                    .color(theme::MUTED),
            );
            ui.add_space(4.0);
            ui.set_max_width(520.0);
            for p in existing.into_iter().take(8) {
                let name = p
                    .file_stem()
                    .map(|f| f.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let dir = p.parent().map(|d| d.display().to_string()).unwrap_or_default();
                let r = ui.add_sized([520.0, 40.0], egui::Button::new(format!("{name}\n{dir}")).frame(false));
                if r.clicked() {
                    action = Some(Action::OpenPath(p.clone()));
                }
            }
        }
    });
    action
}
