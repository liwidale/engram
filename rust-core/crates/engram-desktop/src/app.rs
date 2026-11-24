use crate::i18n::{tr, trf};
use crate::settings::{self, AppSettings, Startup};
use crate::theme;
use crate::views::{self, palette::Palette, preferences::Preferences};
use crate::widgets::{self, muted};
use crate::workspace::{Dialog, Module, Sync, ToastAction, ToastKind, Workspace};
use eframe::egui::{self, Key, KeyboardShortcut, Modifiers, RichText};
use engram_core::diff::ChangeKind;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

pub enum Action {
    Open,
    New,
    OpenPath(PathBuf),
    CloseTab,
    Save,
    Reload,
    Undo,
    Redo,
    Module(Module),
    NewMemory,
    NewDecision,
    NewPrompt,
    NewNode,
    Audit,
    Scan,
    ConnectAgent,
    ProjectSettings,
    Settings,
    Shortcuts,
    CopyMemory,
    CopyHandoff,
    Reveal(&'static str, String),
    RevealPrompt(String),
}

pub struct EngramApp {
    tabs: Vec<Workspace>,
    active: usize,
    recents: Vec<PathBuf>,
    palette: Option<Palette>,
    preferences: Option<Preferences>,
    logo: egui::TextureHandle,
    title: String,
    notice: Option<(String, Instant)>,
}

const RECENTS: &str = "recents";
const SESSION: &str = "session";
const SETTINGS: &str = "settings";

impl EngramApp {
    pub fn new(cc: &eframe::CreationContext<'_>, paths: Vec<PathBuf>) -> Self {
        let storage = cc.storage;
        let prefs: AppSettings = storage.and_then(|s| eframe::get_value(s, SETTINGS)).unwrap_or_default();
        crate::i18n::set_language(prefs.language);
        theme::apply(&cc.egui_ctx, prefs.theme);
        let font_error = theme::apply_font(&cc.egui_ctx, prefs.mono_font.as_deref()).err();
        let restore = prefs.startup == Startup::Restore;
        settings::set(prefs);
        let recents: Vec<PathBuf> = storage.and_then(|s| eframe::get_value(s, RECENTS)).unwrap_or_default();
        let session: Vec<PathBuf> = storage.and_then(|s| eframe::get_value(s, SESSION)).unwrap_or_default();
        let logo = cc.egui_ctx.load_texture(
            "engram-logo",
            image_from_png(crate::LOGO_PNG),
            egui::TextureOptions::LINEAR,
        );
        let mut app = Self {
            tabs: Vec::new(),
            active: 0,
            recents,
            palette: None,
            preferences: None,
            logo,
            title: String::new(),
            notice: font_error.map(|e| {
                (
                    trf("Could not load the editor font: {error}", &[("error", &e)]),
                    Instant::now(),
                )
            }),
        };
        let start = match (paths.is_empty(), restore) {
            (false, _) => paths,
            (true, true) => session,
            (true, false) => Vec::new(),
        };
        for p in start.into_iter().filter(|p| p.exists()) {
            app.open(&cc.egui_ctx, &p);
        }
        app
    }

    fn open(&mut self, ctx: &egui::Context, path: &Path) {
        let path = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        if let Some(i) = self.tabs.iter().position(|w| w.path() == path) {
            self.active = i;
            return;
        }
        match Workspace::open(&path, ctx) {
            Ok(ws) => self.adopt(ws, path),
            Err(e) => self.notice = Some((e.to_string(), Instant::now())),
        }
    }

    fn adopt(&mut self, ws: Workspace, path: PathBuf) {
        self.tabs.push(ws);
        self.active = self.tabs.len() - 1;
        self.recents.retain(|p| p != &path);
        self.recents.insert(0, path);
        self.recents.truncate(12);
    }

    fn ws(&mut self) -> Option<&mut Workspace> {
        self.tabs.get_mut(self.active)
    }

    fn file_dialog() -> rfd::FileDialog {
        rfd::FileDialog::new().add_filter(tr("Engram project memory"), &["engram"])
    }

    fn run(&mut self, ctx: &egui::Context, action: Action) {
        match action {
            Action::Open => {
                if let Some(paths) = Self::file_dialog().set_title(tr("Open project memory")).pick_files() {
                    for p in paths {
                        self.open(ctx, &p);
                    }
                }
            }
            Action::New => {
                let picked = Self::file_dialog()
                    .set_title(tr("Create project memory"))
                    .set_file_name("project.engram")
                    .save_file();
                if let Some(mut path) = picked {
                    if path.extension().is_none_or(|e| e != "engram") {
                        path.set_extension("engram");
                    }
                    match Workspace::create(&path, ctx) {
                        Ok(ws) => {
                            let path = std::fs::canonicalize(&path).unwrap_or(path);
                            self.adopt(ws, path);
                        }
                        Err(e) => self.notice = Some((e.to_string(), Instant::now())),
                    }
                }
            }
            Action::OpenPath(p) => self.open(ctx, &p),
            Action::CloseTab => {
                if let Some(ws) = self.ws() {
                    ws.flush();
                }
                if self.active < self.tabs.len() {
                    self.tabs.remove(self.active);
                    self.active = self.active.min(self.tabs.len().saturating_sub(1));
                }
            }
            Action::Settings => {
                self.preferences = match self.preferences {
                    Some(_) => None,
                    None => Some(Preferences {
                        tab: Default::default(),
                        check: None,
                    }),
                }
            }
            action => {
                if let Some(ws) = self.ws() {
                    Self::run_in(ctx, ws, action);
                }
            }
        }
    }

    fn run_in(ctx: &egui::Context, ws: &mut Workspace, action: Action) {
        match action {
            Action::Save => {
                if ws.flush() {
                    let rev = ws.doc.revision;
                    ws.toast(ToastKind::Success, trf("Saved, revision {rev}", &[("rev", &rev)]));
                }
            }
            Action::Reload => ws.reload(ctx),
            Action::Undo => ws.undo(),
            Action::Redo => ws.redo(),
            Action::Module(m) => {
                ws.flush();
                ws.module = m;
            }
            Action::NewMemory => {
                ws.module = Module::Memory;
                views::memory::new_entry(ws, None);
            }
            Action::NewDecision => {
                ws.module = Module::Decisions;
                views::decisions::new_decision(ws, None);
            }
            Action::NewPrompt => {
                ws.module = Module::Prompts;
                views::prompts::new_prompt(ws, None);
            }
            Action::NewNode => {
                ws.module = Module::Architecture;
                views::architecture::new_node(ws);
            }
            Action::Audit => {
                ws.module = Module::Health;
                ws.run_audit(ctx);
            }
            Action::Scan => views::memory::scan(ws),
            Action::ConnectAgent => {
                ws.dialog = Some(Dialog::Connect {
                    client: settings::get().default_client,
                    check: None,
                    written: None,
                });
            }
            Action::ProjectSettings => {
                ws.flush();
                ws.dialog = Some(Dialog::Project(Box::new(ws.project_draft())));
            }
            Action::Shortcuts => ws.dialog = Some(Dialog::Shortcuts),
            Action::CopyMemory => {
                ctx.copy_text(engram_core::render::memory_markdown(&ws.doc));
                ws.toast(ToastKind::Success, tr("Project memory copied"));
            }
            Action::CopyHandoff => {
                ws.flush();
                ctx.copy_text(engram_core::render::handoff_xml(&ws.doc));
                ws.toast(ToastKind::Success, tr("Handoff copied"));
            }
            Action::Reveal(kind, id) => views::health::reveal(ws, kind, &id),
            Action::RevealPrompt(name) => {
                ws.flush();
                ws.module = Module::Prompts;
                ws.prompt = Some(name);
                ws.prompt_draft = None;
            }
            Action::Open | Action::New | Action::OpenPath(_) | Action::CloseTab | Action::Settings => {}
        }
    }

    fn shortcuts(&mut self, ctx: &egui::Context) -> Option<Action> {
        let cmd = Modifiers::COMMAND;
        let shift = Modifiers::COMMAND | Modifiers::SHIFT;
        let hit = |m: Modifiers, k: Key| ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(m, k)));
        if hit(cmd, Key::K) {
            self.palette = if self.palette.is_some() {
                None
            } else {
                Some(Palette::default())
            };
            return None;
        }
        let digits = [
            Key::Num1,
            Key::Num2,
            Key::Num3,
            Key::Num4,
            Key::Num5,
            Key::Num6,
            Key::Num7,
        ];
        for (k, m) in digits.into_iter().zip(Module::ALL) {
            if hit(cmd, k) {
                return Some(Action::Module(m));
            }
        }
        if !ctx.egui_wants_keyboard_input() {
            if hit(shift, Key::Z) {
                return Some(Action::Redo);
            }
            if hit(cmd, Key::Z) {
                return Some(Action::Undo);
            }
        }
        [
            (shift, Key::N, Action::NewMemory),
            (shift, Key::D, Action::NewDecision),
            (shift, Key::H, Action::Audit),
            (shift, Key::M, Action::ConnectAgent),
            (cmd, Key::Comma, Action::Settings),
            (cmd, Key::Slash, Action::Shortcuts),
            (cmd, Key::O, Action::Open),
            (cmd, Key::N, Action::New),
            (cmd, Key::S, Action::Save),
            (cmd, Key::R, Action::Reload),
            (cmd, Key::W, Action::CloseTab),
        ]
        .into_iter()
        .find(|(m, k, _)| hit(*m, *k))
        .map(|(_, _, a)| a)
    }

    fn tab_bar(&mut self, ui: &mut egui::Ui) -> Option<Action> {
        let mut action = None;
        egui::Panel::top("tabs").show(ui, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                let mut close = None;
                for (i, ws) in self.tabs.iter().enumerate() {
                    let selected = i == self.active;
                    let text = RichText::new(ws.title());
                    if ui
                        .selectable_label(selected, if selected { text.strong() } else { text })
                        .on_hover_text(ws.path().display().to_string())
                        .clicked()
                    {
                        self.active = i;
                    }
                    if ui.small_button("×").on_hover_text(tr("Close tab")).clicked() {
                        close = Some(i);
                    }
                    ui.separator();
                }
                if ui.button("+").on_hover_text(tr("Open file")).clicked() {
                    action = Some(Action::Open);
                }
                if let Some(i) = close {
                    self.active = i;
                    action = Some(Action::CloseTab);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let Some(ws) = self.tabs.get(self.active) else { return };
                    if ui
                        .button("⚙")
                        .on_hover_text(format!("{} (Ctrl+,)", tr("Settings")))
                        .clicked()
                    {
                        action = Some(Action::Settings);
                    }
                    if ui.button(tr("Connect agent")).on_hover_text("Ctrl+Shift+M").clicked() {
                        action = Some(Action::ConnectAgent);
                    }
                    if ui.button(tr("Commands")).on_hover_text("Ctrl+K").clicked() {
                        self.palette = Some(Palette::default());
                    }
                    let h = ws.history.state();
                    if ui
                        .add_enabled(h.can_redo, egui::Button::new("↷"))
                        .on_hover_text(
                            h.redo_label
                                .map_or_else(|| tr("Redo").to_owned(), |l| trf("Redo {what}", &[("what", &l)])),
                        )
                        .clicked()
                    {
                        action = Some(Action::Redo);
                    }
                    if ui
                        .add_enabled(h.can_undo, egui::Button::new("↶"))
                        .on_hover_text(
                            h.undo_label
                                .map_or_else(|| tr("Undo").to_owned(), |l| trf("Undo {what}", &[("what", &l)])),
                        )
                        .clicked()
                    {
                        action = Some(Action::Undo);
                    }
                    sync_pill(ui, ws);
                });
            });
            ui.add_space(2.0);
        });
        action
    }

    fn sidebar(ui: &mut egui::Ui, ws: &mut Workspace) -> Option<Action> {
        let mut action = None;
        egui::Panel::left("modules")
            .resizable(false)
            .exact_size(210.0)
            .show(ui, |ui| {
                ui.add_space(8.0);
                let section = |ui: &mut egui::Ui, t: &str| {
                    ui.add_space(6.0);
                    ui.label(RichText::new(t.to_uppercase()).small().strong().color(theme::MUTED));
                };
                for (i, m) in Module::ALL.iter().enumerate() {
                    if i == 0 {
                        section(ui, tr("Memory"));
                    }
                    if i == 4 {
                        section(ui, tr("Agents"));
                    }
                    let (badge, color) = match m {
                        Module::Memory => (
                            ws.doc.memory.iter().filter(|e| e.is_active()).count().to_string(),
                            theme::MUTED,
                        ),
                        Module::Architecture => (ws.doc.architecture.nodes.len().to_string(), theme::MUTED),
                        Module::Decisions => (
                            ws.doc.decisions.iter().filter(|d| d.is_active()).count().to_string(),
                            theme::MUTED,
                        ),
                        Module::Prompts => (ws.doc.prompts.len().to_string(), theme::MUTED),
                        Module::Health => ws
                            .health
                            .as_ref()
                            .map(|h| (h.score.to_string(), theme::score(h.score)))
                            .unwrap_or((String::new(), theme::MUTED)),
                        _ => (String::new(), theme::MUTED),
                    };
                    let selected = ws.module == *m;
                    let r = ui
                        .horizontal(|ui| {
                            widgets::module_icon(ui, *m, if selected { theme::ACCENT } else { theme::MUTED });
                            let r = ui.add_sized([140.0, 26.0], egui::Button::selectable(selected, m.title()));
                            ui.label(RichText::new(badge).small().color(color));
                            r
                        })
                        .inner;
                    if r.on_hover_text(format!("Ctrl+{}", i + 1)).clicked() {
                        action = Some(Action::Module(*m));
                    }
                }
                ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
                    ui.add_space(8.0);
                    muted(ui, ws.resolved_repo_root().display().to_string());
                    ui.horizontal(|ui| {
                        let file = ws
                            .path()
                            .file_name()
                            .map(|f| f.to_string_lossy().into_owned())
                            .unwrap_or_default();
                        ui.label(RichText::new(file).small());
                        if ui.small_button("⚙").on_hover_text(tr("Project settings")).clicked() {
                            action = Some(Action::ProjectSettings);
                        }
                    });
                    ui.separator();
                });
            });
        action
    }

    fn toasts(ctx: &egui::Context, ws: &mut Workspace) {
        let now = Instant::now();
        ws.toasts.retain(|t| t.until > now);
        let Some(next) = ws.toasts.iter().map(|t| t.until).min() else {
            return;
        };
        let mut undo = false;
        egui::Area::new(egui::Id::unique("toasts"))
            .anchor(egui::Align2::CENTER_BOTTOM, [0.0, -18.0])
            .show(ctx, |ui| {
                for t in ws.toasts.iter().rev().take(3) {
                    let color = match t.kind {
                        ToastKind::Info => theme::NEUTRAL,
                        ToastKind::Success => theme::SUCCESS,
                        ToastKind::Error => theme::ERROR,
                    };
                    egui::Frame::popup(ui.style())
                        .stroke(egui::Stroke::new(1.0, color))
                        .show(ui, |ui| {
                            ui.set_max_width(560.0);
                            ui.horizontal(|ui| {
                                ui.label(&t.text);
                                if t.action == Some(ToastAction::Undo) && ui.link(tr("Undo")).clicked() {
                                    undo = true;
                                }
                            });
                        });
                }
            });
        if undo {
            ws.toasts.retain(|t| t.action.is_none());
            ws.undo();
        }
        ctx.request_repaint_after(next.saturating_duration_since(now));
    }
}

fn sync_pill(ui: &mut egui::Ui, ws: &Workspace) {
    let (text, color) = match &ws.sync {
        Sync::Updated(who, at) if at.elapsed() < Duration::from_secs(8) => {
            ui.ctx()
                .request_repaint_after(Duration::from_secs(8).saturating_sub(at.elapsed()));
            (trf("Updated by {who}", &[("who", who)]), theme::ACCENT)
        }
        _ if ws.dirty() => (tr("Unsaved edits").to_owned(), theme::WARNING),
        _ if ws.watching() => (tr("Live").to_owned(), theme::SUCCESS),
        _ => (tr("Synced").to_owned(), theme::MUTED),
    };
    let label = format!("● {text} · rev {}", ws.doc.revision);
    let resp = ui
        .add(egui::Button::new(RichText::new(label).small().color(color)).frame(false))
        .on_hover_text(tr(
            "Edits from engram-mcp reload automatically. Click for recent activity.",
        ));
    egui::Popup::from_toggle_button_response(&resp).show(|ui| {
        ui.set_width(340.0);
        ui.label(RichText::new(tr("Recent activity")).strong());
        if ws.activity.is_empty() {
            muted(ui, tr("No changes in this session yet."));
        }
        egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
            for a in ws.activity.iter().take(20) {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&a.author).small().strong().color(if a.external {
                        theme::ACCENT
                    } else {
                        theme::MUTED
                    }));
                    muted(ui, ago(a.at.elapsed()));
                });
                for c in a.diff.changes.iter().take(6) {
                    let verb = match c.kind {
                        ChangeKind::Added => tr("Added"),
                        ChangeKind::Updated => tr("Updated"),
                        ChangeKind::Deleted => tr("Deleted"),
                        ChangeKind::Superseded => tr("Superseded"),
                    };
                    ui.label(RichText::new(format!("{verb} {}", c.title)).small());
                }
                ui.separator();
            }
        });
    });
}

fn ago(d: Duration) -> String {
    match d.as_secs() {
        0..=9 => tr("a moment ago").to_owned(),
        s @ 10..=59 => trf("{n} s ago", &[("n", &s)]),
        s @ 60..=3599 => trf("{n} min ago", &[("n", &(s / 60))]),
        s => trf("{n} h ago", &[("n", &(s / 3600))]),
    }
}

fn image_from_png(bytes: &[u8]) -> egui::ColorImage {
    let icon = eframe::icon_data::from_png_bytes(bytes).expect("bundled logo is a valid PNG");
    egui::ColorImage::from_rgba_unmultiplied([icon.width as usize, icon.height as usize], &icon.rgba)
}

impl eframe::App for EngramApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let delay = settings::get().autosave_delay();
        for ws in &mut self.tabs {
            ws.poll(ctx);
            ws.autosave();
            if !ws.watching() {
                ctx.request_repaint_after(Duration::from_secs(1));
            }
            if ws.dirty() || ws.auditing.is_some() {
                ctx.request_repaint_after(delay / 2);
            }
        }
        let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).collect());
        for p in dropped
            .into_iter()
            .filter(|p| p.extension().is_some_and(|e| e == "engram"))
        {
            self.open(ctx, &p);
        }
        let title = match self.tabs.get(self.active) {
            Some(ws) => format!("{} · {} · Engram", ws.title(), ws.module.title()),
            None => "Engram".to_owned(),
        };
        if title != self.title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.title = title;
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let mut actions: Vec<Action> = self.shortcuts(&ctx).into_iter().collect();
        if self.tabs.is_empty() {
            egui::CentralPanel::default().show(ui, |ui| {
                if let Some(a) = views::welcome::show(ui, &self.recents, &self.logo) {
                    actions.push(a);
                }
            });
        } else {
            actions.extend(self.tab_bar(ui));
            let ws = &mut self.tabs[self.active];
            actions.extend(Self::sidebar(ui, ws));
            egui::CentralPanel::default().show(ui, |ui| match ws.module {
                Module::Memory => views::memory::show(ui, ws),
                Module::Architecture => views::architecture::show(ui, ws),
                Module::Decisions => views::decisions::show(ui, ws),
                Module::Prompts => views::prompts::show(ui, ws),
                Module::Context => views::context::show(ui, ws),
                Module::Health => views::health::show(ui, ws),
                Module::Handoff => views::handoff::show(ui, ws),
            });
            views::dialogs::show(&ctx, ws);
            Self::toasts(&ctx, ws);
        }
        if let Some(p) = self.palette.as_mut() {
            let (close, action) = views::palette::show(&ctx, p, self.tabs.get(self.active), &self.recents);
            if close {
                self.palette = None;
            }
            actions.extend(action);
        }
        if let Some(p) = self.preferences.as_mut() {
            if !views::preferences::show(&ctx, p) {
                self.preferences = None;
            }
        }
        if let Some((text, at)) = &self.notice {
            let left = Duration::from_secs(6).saturating_sub(at.elapsed());
            if left.is_zero() {
                self.notice = None;
            } else {
                egui::Area::new(egui::Id::unique("notice"))
                    .anchor(egui::Align2::CENTER_BOTTOM, [0.0, -18.0])
                    .show(&ctx, |ui| {
                        egui::Frame::popup(ui.style())
                            .stroke(egui::Stroke::new(1.0, theme::ERROR))
                            .show(ui, |ui| ui.label(text));
                    });
                ctx.request_repaint_after(left);
            }
        }
        for a in actions {
            self.run(&ctx, a);
        }
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        for ws in &mut self.tabs {
            ws.flush();
        }
        eframe::set_value(storage, SETTINGS, &settings::get());
        eframe::set_value(storage, RECENTS, &self.recents);
        let session: Vec<PathBuf> = self.tabs.iter().map(|w| w.path().to_path_buf()).collect();
        eframe::set_value(storage, SESSION, &session);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        for ws in &mut self.tabs {
            ws.flush();
        }
    }
}
