use crate::i18n::{tr, trf};
use crate::settings;
use eframe::egui::{self, Rect};
use engram_core::context::ContextBundle;
use engram_core::diff::{diff, DocumentDiff};
use engram_core::graph::{Graph, Layout, LayoutOptions};
use engram_core::health::HealthReport;
use engram_core::history::History;
use engram_core::interop::{MemoryCandidate, PromptCandidate};
use engram_core::tokens::count_tokens_cached;
use engram_core::{ArchNode, Decision, EngramDocument, Handoff, MemoryEntry, Op, Prompt, Store};
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Module {
    Memory,
    Architecture,
    Decisions,
    Prompts,
    Context,
    Health,
    Handoff,
}

impl Module {
    pub const ALL: [Module; 7] = [
        Module::Memory,
        Module::Architecture,
        Module::Decisions,
        Module::Prompts,
        Module::Context,
        Module::Health,
        Module::Handoff,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Module::Memory => tr("Project memory"),
            Module::Architecture => tr("Architecture map"),
            Module::Decisions => tr("Decision log"),
            Module::Prompts => tr("Prompt vault"),
            Module::Context => tr("Context builder"),
            Module::Health => tr("Context health"),
            Module::Handoff => tr("Agent handoff"),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ToastKind {
    Info,
    Success,
    Error,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ToastAction {
    Undo,
}

pub struct Toast {
    pub kind: ToastKind,
    pub text: String,
    pub until: Instant,
    pub action: Option<ToastAction>,
}

pub struct Activity {
    pub at: Instant,
    pub author: String,
    pub external: bool,
    pub diff: DocumentDiff,
}

pub enum Sync {
    Synced,
    Updated(String, Instant),
}

#[derive(Clone, Default, PartialEq)]
pub struct MemoryDraft {
    pub key: String,
    pub content: String,
    pub tags: String,
    pub files: Vec<String>,
}

impl From<&MemoryEntry> for MemoryDraft {
    fn from(e: &MemoryEntry) -> Self {
        Self {
            key: e.key.clone(),
            content: e.content.to_string(),
            tags: e.tags.join(", "),
            files: e.files.clone(),
        }
    }
}

#[derive(Clone, Default, PartialEq)]
pub struct DecisionDraft {
    pub id: String,
    pub title: String,
    pub context: String,
    pub decision: String,
    pub rejected: Vec<(String, String)>,
    pub consequences: String,
    pub related: Vec<String>,
    pub files: Vec<String>,
}

impl From<&Decision> for DecisionDraft {
    fn from(d: &Decision) -> Self {
        Self {
            id: d.id.clone(),
            title: d.title.clone(),
            context: d.context.to_string(),
            decision: d.decision.to_string(),
            rejected: d
                .rejected_alternatives
                .iter()
                .map(|a| (a.option.clone(), a.reason.clone()))
                .collect(),
            consequences: d.consequences.to_string(),
            related: d.related.clone(),
            files: d.files.clone(),
        }
    }
}

#[derive(Clone, Default, PartialEq)]
pub struct PromptDraft {
    pub original: String,
    pub name: String,
    pub category: String,
    pub description: String,
    pub body: String,
}

impl From<&Prompt> for PromptDraft {
    fn from(p: &Prompt) -> Self {
        Self {
            original: p.name.clone(),
            name: p.name.clone(),
            category: p.category.clone(),
            description: p.description.clone(),
            body: p.body.to_string(),
        }
    }
}

#[derive(Clone, Default, PartialEq)]
pub struct HandoffDraft {
    pub objective: String,
    pub progress: Vec<(String, Vec<String>, bool)>,
    pub hypotheses: String,
    pub blockers: String,
    pub notes: String,
    pub next_step: String,
}

impl From<&Handoff> for HandoffDraft {
    fn from(h: &Handoff) -> Self {
        Self {
            objective: h.objective.clone(),
            progress: h
                .progress
                .iter()
                .map(|p| (p.description.clone(), p.files.clone(), p.verified))
                .collect(),
            hypotheses: h.working_memory.hypotheses.join("\n"),
            blockers: h.working_memory.blockers.join("\n"),
            notes: h.working_memory.notes.to_string(),
            next_step: h.next_step.clone(),
        }
    }
}

impl HandoffDraft {
    pub fn valid(&self) -> bool {
        !self.objective.trim().is_empty() && !self.next_step.trim().is_empty()
    }
}

#[derive(Clone, Default, PartialEq)]
pub struct NodeDraft {
    pub id: String,
    pub label: String,
    pub path: String,
    pub description: String,
}

impl From<&ArchNode> for NodeDraft {
    fn from(n: &ArchNode) -> Self {
        Self {
            id: n.id.clone(),
            label: n.label.clone().unwrap_or_default(),
            path: n.path.clone().unwrap_or_default(),
            description: n.description.to_string(),
        }
    }
}

pub struct Draft<T> {
    pub value: T,
    pub base: T,
    pub conflict: Option<T>,
    pub edited_at: Instant,
    pub tokens: usize,
    tokens_fresh: bool,
}

impl<T: Clone + PartialEq> Draft<T> {
    pub fn new(value: T) -> Self {
        Self {
            base: value.clone(),
            value,
            conflict: None,
            edited_at: Instant::now() - Duration::from_secs(1),
            tokens: 0,
            tokens_fresh: false,
        }
    }

    pub fn dirty(&self) -> bool {
        self.value != self.base
    }

    pub fn due(&self, after: Duration) -> bool {
        self.dirty() && self.conflict.is_none() && self.edited_at.elapsed() >= after
    }

    pub fn touch(&mut self) {
        self.edited_at = Instant::now();
        self.tokens_fresh = false;
    }

    pub fn refresh_tokens(&mut self, text: &str) {
        if !self.tokens_fresh && self.edited_at.elapsed() >= Duration::from_millis(300) {
            self.tokens = count_tokens_cached(text);
            self.tokens_fresh = true;
        }
    }

    pub fn sync(&mut self, external: T, prompt: bool) {
        if external == self.base {
            return;
        }
        if !self.dirty() {
            self.value = external.clone();
            self.base = external;
            self.tokens_fresh = false;
        } else if external == self.value {
            self.base = external;
        } else if prompt {
            self.conflict = Some(external);
        } else {
            self.base = external;
        }
    }

    pub fn load_external(&mut self) {
        if let Some(external) = self.conflict.take() {
            self.value = external.clone();
            self.base = external;
            self.tokens_fresh = false;
        }
    }

    pub fn keep_mine(&mut self) {
        if let Some(external) = self.conflict.take() {
            self.base = external;
            self.edited_at = Instant::now() - Duration::from_secs(10);
        }
    }

    pub fn mark_saved(&mut self) {
        self.base = self.value.clone();
    }
}

pub struct ContextState {
    pub task: String,
    pub max_tokens: usize,
    pub auto_rank: bool,
    pub include_handoff: bool,
    pub markdown: bool,
    pub forced: BTreeSet<String>,
    pub excluded: BTreeSet<String>,
    pub prompt: Option<String>,
    pub vars: BTreeMap<String, String>,
    pub bundle: Option<ContextBundle>,
    pub stale: bool,
    pub changed_at: Instant,
}

impl Default for ContextState {
    fn default() -> Self {
        let s = settings::get();
        Self {
            task: String::new(),
            max_tokens: s.context_budget,
            auto_rank: true,
            include_handoff: s.context_handoff,
            markdown: s.context_markdown,
            forced: BTreeSet::new(),
            excluded: BTreeSet::new(),
            prompt: None,
            vars: BTreeMap::new(),
            bundle: None,
            stale: true,
            changed_at: Instant::now(),
        }
    }
}

#[derive(Clone, Default)]
pub struct ProjectDraft {
    pub name: String,
    pub description: String,
    pub repo_root: String,
    pub custom_days: bool,
    pub days: u32,
    pub ignored_dirs: Vec<String>,
    pub new_dir: String,
    pub ignored_issues: Vec<String>,
    pub custom_score: bool,
    pub min_score: f64,
    pub server_name: String,
}

pub enum Dialog {
    NewMemory {
        key: String,
        category: engram_core::MemoryCategory,
        content: String,
        supersedes: Option<String>,
        tokens: usize,
    },
    NewDecision {
        title: String,
        context: String,
        decision: String,
        rejected: Vec<(String, String)>,
        supersedes: Option<String>,
    },
    NewNode {
        id: String,
        label: String,
        kind: engram_core::NodeKind,
        path: String,
        deps: BTreeSet<String>,
    },
    NewPrompt {
        name: String,
        category: String,
        description: String,
        body: String,
    },
    FillPrompt {
        name: String,
        values: BTreeMap<String, String>,
    },
    Confirm {
        title: String,
        op: Value,
    },
    Review {
        title: String,
        memory: Vec<(bool, MemoryCandidate)>,
        prompts: Vec<(bool, PromptCandidate)>,
    },
    Project(Box<ProjectDraft>),
    Connect {
        client: engram_core::client_config::McpClient,
        check: Option<Result<String, String>>,
        written: Option<String>,
    },
    Shortcuts,
}

#[derive(Default)]
pub struct GraphView {
    pub scene_rect: Option<Rect>,
    pub dragging: Option<(String, egui::Vec2)>,
    pub linking: Option<(String, egui::Pos2)>,
    pub highlight: BTreeSet<String>,
}

pub struct Workspace {
    pub store: Store,
    pub doc: EngramDocument,
    fingerprint: u64,
    pub layout: Layout,
    pub module: Module,
    pub memory: Option<String>,
    pub decision: Option<String>,
    pub prompt: Option<String>,
    pub node: Option<String>,
    pub memory_draft: Option<Draft<MemoryDraft>>,
    pub decision_draft: Option<Draft<DecisionDraft>>,
    pub prompt_draft: Option<Draft<PromptDraft>>,
    pub handoff_draft: Option<Draft<HandoffDraft>>,
    pub node_draft: Option<Draft<NodeDraft>>,
    pub memory_filter: String,
    pub category_filter: Option<engram_core::MemoryCategory>,
    pub show_superseded: bool,
    pub decision_filter: String,
    pub decision_preview: bool,
    pub prompt_filter: String,
    pub health_filter: Option<engram_core::health::Severity>,
    pub path_query: BTreeMap<String, String>,
    pub context: ContextState,
    pub health: Option<HealthReport>,
    pub auditing: Option<Receiver<HealthReport>>,
    audit_again: bool,
    pub graph: GraphView,
    pub dialog: Option<Dialog>,
    pub toasts: Vec<Toast>,
    pub history: History,
    pub activity: Vec<Activity>,
    pub sync: Sync,
    watcher: Option<(RecommendedWatcher, Receiver<()>)>,
    last_poll: Instant,
    repo_files: Option<(Instant, Vec<String>)>,
}

fn watch(path: &Path, ctx: &egui::Context) -> Option<(RecommendedWatcher, Receiver<()>)> {
    let (tx, rx) = channel();
    let name = path.file_name()?.to_owned();
    let ctx = ctx.clone();
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        if res.is_ok_and(|ev| ev.paths.iter().any(|p| p.file_name() == Some(name.as_os_str()))) {
            let _ = tx.send(());
            ctx.request_repaint();
        }
    })
    .ok()?;
    let dir = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    watcher.watch(dir, RecursiveMode::NonRecursive).ok()?;
    Some((watcher, rx))
}

impl Workspace {
    pub fn open(path: &Path, ctx: &egui::Context) -> engram_core::Result<Self> {
        let store = Store::new(path);
        let snap = store.load()?;
        let mut ws = Self::from_snapshot(store, snap);
        ws.watcher = watch(path, ctx);
        ws.run_audit(ctx);
        Ok(ws)
    }

    pub fn create(path: &Path, ctx: &egui::Context) -> engram_core::Result<Self> {
        let store = Store::new(path);
        let name = path
            .parent()
            .and_then(Path::file_name)
            .map(|n| n.to_string_lossy().into_owned())
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| "Project".into());
        store.create(&name)?;
        Self::open(path, ctx)
    }

    fn from_snapshot(store: Store, snap: engram_core::Snapshot) -> Self {
        let layout = Graph::new(&snap.document.architecture).layout(LayoutOptions::default());
        let memory = snap
            .document
            .memory
            .iter()
            .find(|m| m.is_active())
            .map(|m| m.key.clone());
        let decision = snap
            .document
            .decisions
            .iter()
            .rev()
            .find(|d| d.is_active())
            .map(|d| d.id.clone());
        let prompt = snap.document.prompts.first().map(|p| p.name.clone());
        Self {
            store,
            fingerprint: snap.fingerprint,
            doc: snap.document,
            layout,
            module: Module::Memory,
            memory,
            decision,
            prompt,
            node: None,
            memory_draft: None,
            decision_draft: None,
            prompt_draft: None,
            handoff_draft: None,
            node_draft: None,
            memory_filter: String::new(),
            category_filter: None,
            show_superseded: true,
            decision_filter: String::new(),
            decision_preview: false,
            prompt_filter: String::new(),
            health_filter: None,
            path_query: BTreeMap::new(),
            context: ContextState::default(),
            health: None,
            auditing: None,
            audit_again: false,
            graph: GraphView::default(),
            dialog: None,
            toasts: Vec::new(),
            history: History::default(),
            activity: Vec::new(),
            sync: Sync::Synced,
            watcher: None,
            last_poll: Instant::now(),
            repo_files: None,
        }
    }

    pub fn path(&self) -> &Path {
        self.store.path()
    }

    pub fn title(&self) -> &str {
        &self.doc.project.name
    }

    pub fn is_empty(&self) -> bool {
        let d = &self.doc;
        d.memory.is_empty()
            && d.architecture.nodes.is_empty()
            && d.decisions.is_empty()
            && d.prompts.is_empty()
            && d.handoff.is_empty()
    }

    pub fn watching(&self) -> bool {
        self.watcher.is_some()
    }

    fn adopt(&mut self, snap: engram_core::Snapshot) {
        let architecture_changed = snap.document.architecture != self.doc.architecture;
        self.fingerprint = snap.fingerprint;
        self.doc = snap.document;
        if architecture_changed {
            self.layout = Graph::new(&self.doc.architecture).layout(LayoutOptions::default());
        }
        self.context.stale = true;
        self.sync_drafts();
    }

    fn sync_drafts(&mut self) {
        let prompt = settings::get().conflict_prompt;
        if let Some(d) = self.memory_draft.as_mut() {
            match self.doc.memory_entry(&d.base.key) {
                Some(e) => d.sync(MemoryDraft::from(e), prompt),
                None => self.memory_draft = None,
            }
        }
        if let Some(d) = self.decision_draft.as_mut() {
            match self.doc.decision(&d.base.id) {
                Some(x) => d.sync(DecisionDraft::from(x), prompt),
                None => self.decision_draft = None,
            }
        }
        if let Some(d) = self.prompt_draft.as_mut() {
            match self.doc.prompt(&d.base.original) {
                Some(p) => d.sync(PromptDraft::from(p), prompt),
                None => self.prompt_draft = None,
            }
        }
        if let Some(d) = self.handoff_draft.as_mut() {
            d.sync(HandoffDraft::from(&self.doc.handoff), prompt);
        }
        if let Some(d) = self.node_draft.as_mut() {
            match self.doc.node(&d.base.id) {
                Some(n) => d.sync(NodeDraft::from(n), prompt),
                None => self.node_draft = None,
            }
        }
    }

    fn log(&mut self, diff: DocumentDiff, author: String, external: bool) {
        if diff.is_empty() {
            return;
        }
        self.activity.insert(
            0,
            Activity {
                at: Instant::now(),
                author,
                external,
                diff,
            },
        );
        self.activity.truncate(30);
    }

    pub fn perform_json(&mut self, value: Value) -> Option<Value> {
        let label = Op::label(value.get("op").and_then(Value::as_str).unwrap_or_default());
        match serde_json::from_value::<Op>(value) {
            Ok(op) => self.perform(op, label),
            Err(e) => {
                self.toast(ToastKind::Error, e.to_string());
                None
            }
        }
    }

    pub fn perform(&mut self, op: Op, label: &str) -> Option<Value> {
        match op.execute_tracked(&self.store) {
            Ok((result, before, snap)) => {
                let changes = diff(&before, &snap.document);
                self.history.record(before, &snap, label);
                self.log(changes, tr("You").to_owned(), false);
                self.adopt(snap);
                let warnings: Vec<&str> = result
                    .get("warnings")
                    .and_then(Value::as_array)
                    .map(|w| w.iter().filter_map(Value::as_str).collect())
                    .unwrap_or_default();
                if !warnings.is_empty() {
                    self.toast(ToastKind::Info, warnings.join(" · "));
                }
                Some(result)
            }
            Err(e) => {
                self.toast(ToastKind::Error, e.to_string());
                None
            }
        }
    }

    pub fn delete(&mut self, title: String, op: Value) {
        if settings::get().confirm_delete {
            self.dialog = Some(Dialog::Confirm { title, op });
        } else {
            self.delete_now(&title, op);
        }
    }

    pub fn delete_now(&mut self, title: &str, op: Value) {
        if self.perform_json(op).is_some() {
            self.toasts.push(Toast {
                kind: ToastKind::Success,
                text: trf("Deleted {name}", &[("name", &title)]),
                until: Instant::now() + Duration::from_secs(8),
                action: Some(ToastAction::Undo),
            });
        }
    }

    pub fn undo(&mut self) {
        self.step(true);
    }

    pub fn redo(&mut self) {
        self.step(false);
    }

    fn step(&mut self, undo: bool) {
        self.flush();
        let state = self.history.state();
        let label = if undo { state.undo_label } else { state.redo_label }.unwrap_or_default();
        let current = self.doc.clone();
        let result = if undo {
            self.history.undo(&self.store, &current)
        } else {
            self.history.redo(&self.store, &current)
        };
        match result {
            Ok(snap) => {
                let changes = diff(&current, &snap.document);
                self.log(changes, tr("You").to_owned(), false);
                self.adopt(snap);
                let text = if undo {
                    trf("Undid {what}", &[("what", &label)])
                } else {
                    trf("Redid {what}", &[("what", &label)])
                };
                self.toast(ToastKind::Info, text);
            }
            Err(e) => self.toast(ToastKind::Error, e.to_string()),
        }
    }

    pub fn toast(&mut self, kind: ToastKind, text: impl Into<String>) {
        let secs = if kind == ToastKind::Error { 6 } else { 3 };
        self.toasts.push(Toast {
            kind,
            text: text.into(),
            until: Instant::now() + Duration::from_secs(secs),
            action: None,
        });
    }

    pub fn poll(&mut self, ctx: &egui::Context) {
        let changed = match &self.watcher {
            Some((_, rx)) => rx.try_iter().count() > 0,
            None => {
                let due = self.last_poll.elapsed() >= Duration::from_secs(1);
                if due {
                    self.last_poll = Instant::now();
                }
                due
            }
        };
        if changed && self.store.disk_fingerprint().is_some_and(|fp| fp != self.fingerprint) {
            self.reload_external(ctx);
        }
        if let Some(rx) = &self.auditing {
            if let Ok(report) = rx.try_recv() {
                self.health = Some(report);
                self.auditing = None;
                if std::mem::take(&mut self.audit_again) {
                    self.run_audit(ctx);
                }
            }
        }
    }

    fn reload_external(&mut self, ctx: &egui::Context) {
        match self.store.load() {
            Ok(snap) => {
                let changes = diff(&self.doc, &snap.document);
                let author = changes.author.clone().unwrap_or_else(|| tr("another app").to_owned());
                let summary = changes.summary();
                self.log(changes, author.clone(), true);
                self.adopt(snap);
                self.sync = Sync::Updated(author.clone(), Instant::now());
                if settings::get().notify_external {
                    self.toast(
                        ToastKind::Info,
                        trf("Updated by {who} · {what}", &[("who", &author), ("what", &summary)]),
                    );
                }
                self.run_audit(ctx);
            }
            Err(e) => self.toast(ToastKind::Error, e.to_string()),
        }
    }

    pub fn reload(&mut self, ctx: &egui::Context) {
        if self.store.disk_fingerprint() == Some(self.fingerprint) {
            self.toast(ToastKind::Info, tr("Already up to date"));
        } else {
            self.reload_external(ctx);
        }
    }

    pub fn run_audit(&mut self, ctx: &egui::Context) {
        if self.auditing.is_some() {
            self.audit_again = true;
            return;
        }
        let (tx, rx) = channel();
        let doc = self.doc.clone();
        let base = self.store.base_dir();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(engram_core::audit(&doc, &base, None));
            ctx.request_repaint();
        });
        self.auditing = Some(rx);
    }

    pub fn resolved_repo_root(&self) -> PathBuf {
        engram_core::health::resolve_repo_root(&self.doc, &self.store.base_dir(), None).0
    }

    pub fn repo_files(&mut self, query: &str) -> Vec<String> {
        if self
            .repo_files
            .as_ref()
            .is_none_or(|(at, _)| at.elapsed() > Duration::from_secs(10))
        {
            let files = engram_core::health::repo_files(
                &self.resolved_repo_root(),
                &self.doc.project.settings.health_ignore_dirs,
            );
            self.repo_files = Some((Instant::now(), files));
        }
        let files = self.repo_files.as_ref().map(|(_, f)| f.as_slice()).unwrap_or_default();
        engram_core::ffi::rank_files(files, query, 8)
    }

    pub fn detected_mcp_binary() -> PathBuf {
        let name = if cfg!(windows) { "engram-mcp.exe" } else { "engram-mcp" };
        std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(|d| d.join(name)))
            .filter(|p| p.exists())
            .unwrap_or_else(|| PathBuf::from(name))
    }

    pub fn mcp_binary() -> PathBuf {
        let custom = settings::get().mcp_binary.trim().to_owned();
        if custom.is_empty() {
            Self::detected_mcp_binary()
        } else {
            PathBuf::from(custom)
        }
    }

    pub fn client_configs(&self) -> Vec<engram_core::client_config::ClientConfig> {
        let binary = engram_core::client_config::display_path(&Self::mcp_binary());
        engram_core::client_config::client_configs(&self.doc, &binary, self.store.path(), &self.resolved_repo_root())
    }

    pub fn flush(&mut self) -> bool {
        let a = crate::views::memory::commit(self);
        let b = crate::views::decisions::commit(self);
        let c = crate::views::prompts::commit(self);
        let d = crate::views::handoff::commit(self);
        let e = crate::views::architecture::commit(self);
        a || b || c || d || e
    }

    pub fn dirty(&self) -> bool {
        self.memory_draft.as_ref().is_some_and(Draft::dirty)
            || self.decision_draft.as_ref().is_some_and(Draft::dirty)
            || self.prompt_draft.as_ref().is_some_and(Draft::dirty)
            || self.handoff_draft.as_ref().is_some_and(Draft::dirty)
            || self.node_draft.as_ref().is_some_and(Draft::dirty)
    }

    pub fn autosave(&mut self) {
        let s = settings::get();
        if !s.autosave {
            return;
        }
        let wait = s.autosave_delay();
        if self.memory_draft.as_ref().is_some_and(|d| d.due(wait)) {
            crate::views::memory::commit(self);
        }
        if self.decision_draft.as_ref().is_some_and(|d| d.due(wait)) {
            crate::views::decisions::commit(self);
        }
        if self.prompt_draft.as_ref().is_some_and(|d| d.due(wait)) {
            crate::views::prompts::commit(self);
        }
        if self
            .handoff_draft
            .as_ref()
            .is_some_and(|d| d.due(wait) && d.value.valid())
        {
            crate::views::handoff::commit(self);
        }
        if self.node_draft.as_ref().is_some_and(|d| d.due(wait)) {
            crate::views::architecture::commit(self);
        }
    }

    pub fn project_draft(&self) -> ProjectDraft {
        let p = &self.doc.project;
        ProjectDraft {
            name: p.name.clone(),
            description: p.description.to_string(),
            repo_root: p.repo_root.clone().unwrap_or_default(),
            custom_days: p.settings.stale_handoff_days.is_some(),
            days: p.settings.stale_handoff_days(),
            ignored_dirs: p.settings.health_ignore_dirs.clone(),
            new_dir: String::new(),
            ignored_issues: p.settings.health_ignore_issues.clone(),
            custom_score: p.settings.context_min_score.is_some(),
            min_score: p.settings.context_min_score(),
            server_name: p.settings.mcp_server_name.clone().unwrap_or_default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drafts_detect_conflicts_but_not_their_own_writes() {
        let mut d = Draft::new("a".to_owned());
        d.sync("b".into(), true);
        assert_eq!(
            (d.value.as_str(), d.dirty()),
            ("b", false),
            "clean drafts follow the file"
        );
        d.value = "mine".into();
        d.mark_saved();
        d.sync("mine".into(), true);
        assert!(d.conflict.is_none(), "our own save comes back unchanged");
        d.value = "mine 2".into();
        d.sync("agent".into(), true);
        assert_eq!(d.conflict.as_deref(), Some("agent"));
        assert!(!d.due(Duration::ZERO), "autosave waits while a conflict is open");
        d.keep_mine();
        assert!(d.conflict.is_none() && d.dirty() && d.due(Duration::ZERO));
        d.sync("agent 2".into(), true);
        d.load_external();
        assert_eq!((d.value.as_str(), d.dirty()), ("agent 2", false));
        d.value = "x".into();
        d.sync("y".into(), false);
        assert!(d.conflict.is_none() && d.dirty(), "without prompting, local edits win");
    }
}
