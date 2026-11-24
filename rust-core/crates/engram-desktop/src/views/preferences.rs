use crate::i18n::tr;
use crate::settings::{self, AppSettings, EditorMode, Language, Startup, ThemeChoice};
use crate::widgets::{self, muted};
use crate::workspace::Workspace;
use eframe::egui::{self, RichText};
use engram_core::client_config::McpClient;

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum Tab {
    #[default]
    General,
    Appearance,
    Agents,
    Context,
}

impl Tab {
    const ALL: [Tab; 4] = [Tab::General, Tab::Appearance, Tab::Agents, Tab::Context];

    fn title(self) -> &'static str {
        match self {
            Tab::General => tr("General"),
            Tab::Appearance => tr("Appearance"),
            Tab::Agents => tr("Agents"),
            Tab::Context => tr("Context"),
        }
    }
}

pub struct Preferences {
    pub tab: Tab,
    pub check: Option<Result<String, String>>,
}

pub fn show(ctx: &egui::Context, state: &mut Preferences) -> bool {
    let mut s = settings::get();
    let before = s.clone();
    let mut open = true;
    egui::Window::new(tr("Settings"))
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .default_width(520.0)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                for t in Tab::ALL {
                    ui.selectable_value(&mut state.tab, t, t.title());
                }
            });
            ui.separator();
            match state.tab {
                Tab::General => general(ui, &mut s),
                Tab::Appearance => appearance(ui, &mut s),
                Tab::Agents => agents(ui, &mut s, &mut state.check),
                Tab::Context => context(ui, &mut s),
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button(tr("Restore defaults")).clicked() {
                    s = AppSettings::default();
                }
            });
        });
    if s != before {
        apply_changes(ctx, &before, &s);
        settings::set(s);
    }
    open && !ctx.input(|i| i.key_pressed(egui::Key::Escape))
}

pub fn apply_changes(ctx: &egui::Context, before: &AppSettings, after: &AppSettings) {
    if before.language != after.language {
        crate::i18n::set_language(after.language);
    }
    if before.theme != after.theme {
        crate::theme::apply(ctx, after.theme);
    }
    if before.mono_font != after.mono_font {
        let _ = crate::theme::apply_font(ctx, after.mono_font.as_deref());
    }
}

fn general(ui: &mut egui::Ui, s: &mut AppSettings) {
    egui::Grid::new("prefs-general")
        .num_columns(2)
        .spacing([16.0, 8.0])
        .show(ui, |ui| {
            ui.label(tr("Language"));
            egui::ComboBox::from_id_salt("language")
                .selected_text(language_title(s.language))
                .show_ui(ui, |ui| {
                    for l in [Language::System, Language::English, Language::Russian] {
                        ui.selectable_value(&mut s.language, l, language_title(l));
                    }
                });
            ui.end_row();
            ui.label(tr("On launch"));
            ui.horizontal(|ui| {
                ui.selectable_value(&mut s.startup, Startup::Welcome, tr("Show welcome"));
                ui.selectable_value(&mut s.startup, Startup::Restore, tr("Reopen last files"));
            });
            ui.end_row();
            ui.label(tr("Saving"));
            ui.vertical(|ui| {
                ui.checkbox(&mut s.autosave, tr("Save edits automatically"));
                ui.add_enabled(
                    s.autosave,
                    egui::Slider::new(&mut s.autosave_ms, 500..=3000)
                        .step_by(100.0)
                        .suffix(" ms")
                        .text(tr("after typing stops")),
                );
            });
            ui.end_row();
            ui.label(tr("Safety"));
            ui.checkbox(&mut s.confirm_delete, tr("Confirm before deleting"));
            ui.end_row();
            ui.label(tr("External changes"));
            ui.vertical(|ui| {
                ui.checkbox(&mut s.notify_external, tr("Notify when an agent edits the file"));
                ui.checkbox(&mut s.conflict_prompt, tr("Ask before replacing unsaved edits"));
            });
            ui.end_row();
        });
}

fn language_title(l: Language) -> &'static str {
    match l {
        Language::System => tr("System"),
        Language::English => "English",
        Language::Russian => "Русский",
    }
}

fn appearance(ui: &mut egui::Ui, s: &mut AppSettings) {
    egui::Grid::new("prefs-appearance")
        .num_columns(2)
        .spacing([16.0, 8.0])
        .show(ui, |ui| {
            ui.label(tr("Theme"));
            ui.horizontal(|ui| {
                ui.selectable_value(&mut s.theme, ThemeChoice::System, tr("System"));
                ui.selectable_value(&mut s.theme, ThemeChoice::Light, tr("Light"));
                ui.selectable_value(&mut s.theme, ThemeChoice::Dark, tr("Dark"));
            });
            ui.end_row();
            ui.label(tr("Editor font"));
            ui.horizontal(|ui| {
                let name = s
                    .mono_font
                    .as_ref()
                    .and_then(|p| p.file_name())
                    .map(|f| f.to_string_lossy().into_owned())
                    .unwrap_or_else(|| tr("Built-in").to_owned());
                ui.label(RichText::new(name).monospace());
                if ui.button(tr("Choose")).clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter(tr("Fonts"), &["ttf", "otf"])
                        .pick_file()
                    {
                        s.mono_font = Some(path);
                    }
                }
                if s.mono_font.is_some() && ui.button(tr("Reset")).clicked() {
                    s.mono_font = None;
                }
            });
            ui.end_row();
            ui.label(tr("Editor size"));
            ui.add(egui::Slider::new(&mut s.mono_size, 10.0..=20.0).step_by(1.0));
            ui.end_row();
            ui.label(tr("Editor"));
            ui.vertical(|ui| {
                ui.checkbox(&mut s.soft_wrap, tr("Wrap long lines"));
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut s.memory_mode, EditorMode::Edit, tr("Edit"));
                    ui.selectable_value(&mut s.memory_mode, EditorMode::Split, tr("Split"));
                    ui.selectable_value(&mut s.memory_mode, EditorMode::Preview, tr("Preview"));
                });
            });
            ui.end_row();
            ui.label(tr("Architecture map"));
            ui.vertical(|ui| {
                ui.checkbox(&mut s.map_grid, tr("Show grid"));
                ui.checkbox(&mut s.map_paths, tr("Show paths on nodes"));
                ui.checkbox(&mut s.map_curved, tr("Curved edges"));
            });
            ui.end_row();
        });
}

fn agents(ui: &mut egui::Ui, s: &mut AppSettings, check: &mut Option<Result<String, String>>) {
    egui::Grid::new("prefs-agents")
        .num_columns(2)
        .spacing([16.0, 8.0])
        .show(ui, |ui| {
            ui.label(tr("engram-mcp binary"));
            ui.vertical(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut s.mcp_binary)
                        .hint_text(Workspace::detected_mcp_binary().display().to_string())
                        .font(egui::TextStyle::Monospace)
                        .desired_width(320.0),
                );
                ui.horizontal(|ui| {
                    if ui.button(tr("Choose")).clicked() {
                        if let Some(path) = rfd::FileDialog::new().pick_file() {
                            s.mcp_binary = path.display().to_string();
                            *check = None;
                        }
                    }
                    if ui.button(tr("Verify server")).clicked() {
                        *check = Some(crate::views::dialogs::verify(&Workspace::mcp_binary()));
                    }
                });
                match check {
                    Some(Ok(v)) => drop(ui.label(RichText::new(v.as_str()).color(crate::theme::SUCCESS))),
                    Some(Err(e)) => drop(ui.label(RichText::new(e.as_str()).color(crate::theme::ERROR))),
                    None => drop(muted(ui, tr("Leave empty to use the binary next to Engram."))),
                }
            });
            ui.end_row();
            ui.label(tr("Default client"));
            egui::ComboBox::from_id_salt("default-client")
                .selected_text(s.default_client.title())
                .show_ui(ui, |ui| {
                    for c in McpClient::ALL {
                        ui.selectable_value(&mut s.default_client, c, c.title());
                    }
                });
            ui.end_row();
            ui.label(tr("Handoff author"));
            ui.add(egui::TextEdit::singleline(&mut s.handoff_author).hint_text("human"));
            ui.end_row();
        });
}

fn context(ui: &mut egui::Ui, s: &mut AppSettings) {
    egui::Grid::new("prefs-context")
        .num_columns(2)
        .spacing([16.0, 8.0])
        .show(ui, |ui| {
            ui.label(tr("Token budget"));
            ui.add(
                egui::Slider::new(&mut s.context_budget, 500..=32000)
                    .step_by(500.0)
                    .logarithmic(true),
            );
            ui.end_row();
            ui.label(tr("Defaults"));
            ui.vertical(|ui| {
                ui.checkbox(&mut s.context_handoff, tr("Include the agent handoff"));
                ui.checkbox(&mut s.context_markdown, tr("Output Markdown instead of XML"));
            });
            ui.end_row();
        });
    widgets::muted(ui, tr("These defaults apply to projects opened after the change."));
}
