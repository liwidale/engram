use crate::i18n::{tr, trf};
use crate::theme;
use crate::views::{self, palette::Palette};
use crate::widgets::muted;
use crate::workspace::{Module, ToastKind, Workspace};
use eframe::egui::{self, Key, KeyboardShortcut, Modifiers, RichText};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

pub enum Action {
    Open,
    New,
    OpenPath(PathBuf),
    CloseTab,
    Save,
    Reload,
    Module(Module),
    NewMemory,
    NewDecision,
    NewPrompt,
    NewNode,
    Audit,
    CopyMcp,
    CopyMcpCommand,
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
    logo: egui::TextureHandle,
    title: String,
    notice: Option<(String, Instant)>,
}

const RECENTS: &str = "recents";
const SESSION: &str = "session";

impl EngramApp {
    pub fn new(cc: &eframe::CreationContext<'_>, paths: Vec<PathBuf>) -> Self {
        theme::apply(&cc.egui_ctx);
        let storage = cc.storage;
        let recents: Vec<PathBuf> = storage.and_then(|s| eframe::get_value(s, RECENTS)).unwrap_or_default();
        let session: Vec<PathBuf> = storage.and_then(|s| eframe::get_value(s, SESSION)).unwrap_or_default();
        let logo = {
            let img = image_from_png(crate::LOGO_PNG);
            cc.egui_ctx
                .load_texture("engram-logo", img, egui::TextureOptions::LINEAR)
        };
        let mut app = Self {
            tabs: Vec::new(),
            active: 0,
            recents,
            palette: None,
            logo,
            title: String::new(),
            notice: None,
        };
        let start = if paths.is_empty() { session } else { paths };
        for p in start.into_iter().filter(|p| p.exists()) {
            app.open(&p);
        }
        app
    }

    fn open(&mut self, path: &Path) {
        let path = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        if let Some(i) = self.tabs.iter().position(|w| w.path() == path) {
            self.active = i;
            return;
        }
        match Workspace::open(&path) {
            Ok(ws) => {
                self.tabs.push(ws);
                self.active = self.tabs.len() - 1;
                self.remember(path);
            }
            Err(e) => self.notice = Some((e.to_string(), Instant::now())),
        }
    }

    fn remember(&mut self, path: PathBuf) {
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
                        self.open(&p);
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
                    match Workspace::create(&path) {
                        Ok(ws) => {
                            self.tabs.push(ws);
                            self.active = self.tabs.len() - 1;
                            self.remember(std::fs::canonicalize(&path).unwrap_or(path));
                        }
                        Err(e) => self.notice = Some((e.to_string(), Instant::now())),
                    }
                }
            }
            Action::OpenPath(p) => self.open(&p),
            Action::CloseTab => {
                if let Some(ws) = self.ws() {
                    ws.flush();
                }
                if self.active < self.tabs.len() {
                    self.tabs.remove(self.active);
                    self.active = self.active.min(self.tabs.len().saturating_sub(1));
                }
            }
            Action::Save => {
                if let Some(ws) = self.ws() {
                    ws.flush();
                    let rev = ws.doc.revision;
                    ws.toast(ToastKind::Success, trf("Saved, revision {rev}", &[("rev", &rev)]));
                }
            }
            Action::Reload => {
                if let Some(ws) = self.ws() {
                    ws.reload();
                }
            }
            Action::Module(m) => {
                if let Some(ws) = self.ws() {
                    ws.flush();
                    ws.module = m;
                }
            }
            Action::NewMemory => {
                if let Some(ws) = self.ws() {
                    ws.module = Module::Memory;
                    views::memory::new_entry(ws, None);
                }
            }
            Action::NewDecision => {
                if let Some(ws) = self.ws() {
                    ws.module = Module::Decisions;
                    views::decisions::new_decision(ws, None);
                }
            }
            Action::NewPrompt => {
                if let Some(ws) = self.ws() {
                    ws.module = Module::Prompts;
                    views::prompts::new_prompt(ws);
                }
            }
            Action::NewNode => {
                if let Some(ws) = self.ws() {
                    ws.module = Module::Architecture;
                    views::architecture::new_node(ws);
                }
            }
            Action::Audit => {
                if let Some(ws) = self.ws() {
                    ws.module = Module::Health;
                    ws.health = None;
                    ws.run_audit(ctx);
                }
            }
            Action::CopyMcp | Action::CopyMcpCommand => {
                if let Some(ws) = self.ws() {
                    let cfg = ws.mcp_config();
                    let key = if matches!(action, Action::CopyMcp) {
                        "json"
                    } else {
                        "claude_code_command"
                    };
                    ctx.copy_text(cfg[key].as_str().unwrap_or_default().to_owned());
                    ws.toast(
                        ToastKind::Success,
                        if key == "json" {
                            tr("MCP config copied")
                        } else {
                            tr("Command copied")
                        },
                    );
                }
            }
            Action::CopyMemory => {
                if let Some(ws) = self.ws() {
                    ctx.copy_text(engram_core::render::memory_markdown(&ws.doc));
                    ws.toast(ToastKind::Success, tr("Project memory copied"));
                }
            }
            Action::CopyHandoff => {
                if let Some(ws) = self.ws() {
                    ws.flush();
                    ctx.copy_text(engram_core::render::handoff_xml(&ws.doc));
                    ws.toast(ToastKind::Success, tr("Handoff copied"));
                }
            }
            Action::Reveal(kind, id) => {
                if let Some(ws) = self.ws() {
                    ws.flush();
                    views::health::reveal(ws, kind, &id);
                }
            }
            Action::RevealPrompt(name) => {
                if let Some(ws) = self.ws() {
                    ws.flush();
                    ws.module = Module::Prompts;
                    ws.prompt = Some(name);
                    ws.prompt_draft = None;
                }
            }
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
        [
            (shift, Key::N, Action::NewMemory),
            (shift, Key::D, Action::NewDecision),
            (shift, Key::H, Action::Audit),
            (shift, Key::M, Action::CopyMcp),
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
                    ui.menu_button(tr("Copy MCP config"), |ui| {
                        if ui
                            .button(tr("Copy JSON for claude_desktop_config.json or .mcp.json"))
                            .clicked()
                        {
                            action = Some(Action::CopyMcp);
                            ui.close();
                        }
                        if ui.button(tr("Copy claude mcp add command")).clicked() {
                            action = Some(Action::CopyMcpCommand);
                            ui.close();
                        }
                        ui.separator();
                        muted(ui, Workspace::mcp_binary().display().to_string());
                    });
                    if ui.button(tr("Commands")).on_hover_text("Ctrl+K").clicked() {
                        self.palette = Some(Palette::default());
                    }
                    let pulse = ws
                        .external_revision
                        .is_some_and(|(_, at)| at.elapsed() < Duration::from_millis(1200));
                    ui.label(
                        RichText::new(trf("rev {n}", &[("n", &ws.doc.revision)]))
                            .small()
                            .color(if pulse { theme::ACCENT } else { theme::MUTED }),
                    )
                    .on_hover_text(tr("Live: edits from engram-mcp reload automatically"));
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
                    let badge = match m {
                        Module::Memory => ws.doc.memory.iter().filter(|e| e.is_active()).count().to_string(),
                        Module::Architecture => ws.doc.architecture.nodes.len().to_string(),
                        Module::Decisions => ws.doc.decisions.iter().filter(|d| d.is_active()).count().to_string(),
                        Module::Prompts => ws.doc.prompts.len().to_string(),
                        Module::Health => ws.health.as_ref().map(|h| h.score.to_string()).unwrap_or_default(),
                        _ => String::new(),
                    };
                    ui.horizontal(|ui| {
                        let r = ui.add_sized([150.0, 26.0], egui::Button::selectable(ws.module == *m, m.title()));
                        if r.clicked() {
                            action = Some(Action::Module(*m));
                        }
                        r.on_hover_text(format!("Ctrl+{}", i + 1));
                        muted(ui, badge);
                    });
                }
                ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
                    ui.add_space(8.0);
                    muted(ui, ws.resolved_repo_root().display().to_string());
                    ui.label(
                        RichText::new(
                            ws.path()
                                .file_name()
                                .map(|f| f.to_string_lossy().into_owned())
                                .unwrap_or_default(),
                        )
                        .small(),
                    );
                    ui.separator();
                });
            });
        action
    }

    fn toasts(ctx: &egui::Context, ws: &mut Workspace) {
        ws.toasts.retain(|t| t.until > Instant::now());
        if ws.toasts.is_empty() {
            return;
        }
        egui::Area::new(egui::Id::unique("toasts"))
            .anchor(egui::Align2::CENTER_BOTTOM, [0.0, -18.0])
            .show(ctx, |ui| {
                for t in ws.toasts.iter().rev().take(3) {
                    let color = match t.kind {
                        ToastKind::Info => theme::ACCENT,
                        ToastKind::Success => theme::SUCCESS,
                        ToastKind::Error => theme::ERROR,
                    };
                    egui::Frame::popup(ui.style())
                        .stroke(egui::Stroke::new(1.0, color))
                        .show(ui, |ui| {
                            ui.set_max_width(560.0);
                            ui.label(&t.text);
                        });
                }
            });
        ctx.request_repaint_after(Duration::from_millis(250));
    }
}

fn image_from_png(bytes: &[u8]) -> egui::ColorImage {
    let icon = eframe::icon_data::from_png_bytes(bytes).expect("bundled logo is a valid PNG");
    egui::ColorImage::from_rgba_unmultiplied([icon.width as usize, icon.height as usize], &icon.rgba)
}

impl eframe::App for EngramApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        for ws in &mut self.tabs {
            ws.poll();
            ws.autosave();
        }
        let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).collect());
        for p in dropped
            .into_iter()
            .filter(|p| p.extension().is_some_and(|e| e == "engram"))
        {
            self.open(&p);
        }
        let title = match self.tabs.get(self.active) {
            Some(ws) => format!("{} · {} · Engram", ws.title(), ws.module.title()),
            None => "Engram".to_owned(),
        };
        if title != self.title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.title = title;
        }
        ctx.request_repaint_after(Duration::from_millis(500));
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
        if let Some((text, at)) = &self.notice {
            if at.elapsed() < Duration::from_secs(6) {
                egui::Area::new(egui::Id::unique("notice"))
                    .anchor(egui::Align2::CENTER_BOTTOM, [0.0, -18.0])
                    .show(&ctx, |ui| {
                        egui::Frame::popup(ui.style())
                            .stroke(egui::Stroke::new(1.0, theme::ERROR))
                            .show(ui, |ui| ui.label(text));
                    });
            } else {
                self.notice = None;
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
