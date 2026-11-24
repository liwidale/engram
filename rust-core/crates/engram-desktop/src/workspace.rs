use crate::i18n::{tr, trf};
use eframe::egui::{self, Rect};
use engram_core::context::ContextBundle;
use engram_core::graph::{Graph, Layout, LayoutOptions};
use engram_core::health::HealthReport;
use engram_core::{EngramDocument, Op, Store};
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

pub struct Toast {
    pub kind: ToastKind,
    pub text: String,
    pub until: Instant,
}

#[derive(Clone, Default, PartialEq)]
pub struct MemoryDraft {
    pub key: String,
    pub content: String,
    pub tags: String,
    pub files: String,
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
}

#[derive(Clone, Default, PartialEq)]
pub struct PromptDraft {
    pub original: String,
    pub name: String,
    pub category: String,
    pub description: String,
    pub body: String,
}

#[derive(Clone, Default, PartialEq)]
pub struct HandoffDraft {
    pub objective: String,
    pub progress: Vec<(String, String, bool)>,
    pub hypotheses: String,
    pub blockers: String,
    pub notes: String,
    pub next_step: String,
}

pub struct Draft<T> {
    pub value: T,
    pub base: T,
    pub edited_at: Instant,
}

impl<T: Clone + PartialEq> Draft<T> {
    pub fn new(value: T) -> Self {
        Self {
            base: value.clone(),
            value,
            edited_at: Instant::now(),
        }
    }
    pub fn dirty(&self) -> bool {
        self.value != self.base
    }
    pub fn due(&self, after: Duration) -> bool {
        self.dirty() && self.edited_at.elapsed() >= after
    }
}

pub struct ContextState {
    pub task: String,
    pub max_tokens: usize,
    pub auto_rank: bool,
    pub include_handoff: bool,
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
        Self {
            task: String::new(),
            max_tokens: 4000,
            auto_rank: true,
            include_handoff: true,
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

pub enum Dialog {
    NewMemory {
        key: String,
        category: engram_core::MemoryCategory,
        content: String,
        supersedes: Option<String>,
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
    FillPrompt {
        name: String,
        values: BTreeMap<String, String>,
    },
}

#[derive(Default)]
pub struct GraphView {
    pub scene_rect: Option<Rect>,
    pub dragging: Option<(String, egui::Vec2)>,
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
    pub memory_filter: String,
    pub show_superseded: bool,
    pub memory_mode: usize,
    pub decision_filter: String,
    pub prompt_filter: String,
    pub context: ContextState,
    pub health: Option<HealthReport>,
    pub auditing: Option<Receiver<HealthReport>>,
    pub graph: GraphView,
    pub node_edit: Option<[String; 4]>,
    pub dialog: Option<Dialog>,
    pub toasts: Vec<Toast>,
    pub external_revision: Option<(u64, Instant)>,
    last_poll: Instant,
}

impl Workspace {
    pub fn open(path: &Path) -> engram_core::Result<Self> {
        let store = Store::new(path);
        let snap = store.load()?;
        Ok(Self::from_snapshot(store, snap))
    }

    pub fn create(path: &Path) -> engram_core::Result<Self> {
        let store = Store::new(path);
        let name = path
            .parent()
            .and_then(Path::file_name)
            .map(|n| n.to_string_lossy().into_owned())
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| "Project".into());
        store.create(&name)?;
        Self::open(path)
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
            memory_filter: String::new(),
            show_superseded: true,
            memory_mode: 1,
            decision_filter: String::new(),
            prompt_filter: String::new(),
            context: ContextState::default(),
            health: None,
            auditing: None,
            graph: GraphView::default(),
            node_edit: None,
            dialog: None,
            toasts: Vec::new(),
            external_revision: None,
            last_poll: Instant::now(),
        }
    }

    pub fn path(&self) -> &Path {
        self.store.path()
    }

    pub fn title(&self) -> &str {
        &self.doc.project.name
    }

    fn adopt(&mut self, snap: engram_core::Snapshot) {
        self.fingerprint = snap.fingerprint;
        self.doc = snap.document;
        self.layout = Graph::new(&self.doc.architecture).layout(LayoutOptions::default());
        self.context.stale = true;
        if self.memory_draft.as_ref().is_some_and(|d| !d.dirty()) {
            self.memory_draft = None;
        }
        if self.decision_draft.as_ref().is_some_and(|d| !d.dirty()) {
            self.decision_draft = None;
        }
        if self.prompt_draft.as_ref().is_some_and(|d| !d.dirty()) {
            self.prompt_draft = None;
        }
        if self.handoff_draft.as_ref().is_some_and(|d| !d.dirty()) {
            self.handoff_draft = None;
        }
    }

    pub fn perform(&mut self, op: Op) -> Option<Value> {
        match op.execute(&self.store) {
            Ok((result, snap)) => {
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

    pub fn perform_json(&mut self, value: Value) -> Option<Value> {
        match serde_json::from_value::<Op>(value) {
            Ok(op) => self.perform(op),
            Err(e) => {
                self.toast(ToastKind::Error, e.to_string());
                None
            }
        }
    }

    pub fn toast(&mut self, kind: ToastKind, text: impl Into<String>) {
        let secs = if kind == ToastKind::Error { 6 } else { 3 };
        self.toasts.push(Toast {
            kind,
            text: text.into(),
            until: Instant::now() + Duration::from_secs(secs),
        });
    }

    pub fn poll(&mut self) {
        if self.last_poll.elapsed() < Duration::from_millis(400) {
            return;
        }
        self.last_poll = Instant::now();
        match self.store.disk_fingerprint() {
            Some(fp) if fp != self.fingerprint => match self.store.load() {
                Ok(snap) => {
                    let rev = snap.document.revision;
                    self.adopt(snap);
                    self.external_revision = Some((rev, Instant::now()));
                    self.toast(
                        ToastKind::Info,
                        trf("Reloaded external changes, revision {rev}", &[("rev", &rev)]),
                    );
                }
                Err(e) => self.toast(ToastKind::Error, e.to_string()),
            },
            _ => {}
        }
        if let Some(rx) = &self.auditing {
            if let Ok(report) = rx.try_recv() {
                self.health = Some(report);
                self.auditing = None;
            }
        }
    }

    pub fn reload(&mut self) {
        match self.store.load() {
            Ok(snap) => {
                let rev = snap.document.revision;
                self.adopt(snap);
                self.toast(ToastKind::Info, trf("Reloaded revision {rev}", &[("rev", &rev)]));
            }
            Err(e) => self.toast(ToastKind::Error, e.to_string()),
        }
    }

    pub fn run_audit(&mut self, ctx: &egui::Context) {
        if self.auditing.is_some() {
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

    pub fn mcp_binary() -> PathBuf {
        let name = if cfg!(windows) { "engram-mcp.exe" } else { "engram-mcp" };
        std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(|d| d.join(name)))
            .filter(|p| p.exists())
            .unwrap_or_else(|| PathBuf::from(name))
    }

    pub fn mcp_config(&self) -> Value {
        let binary = engram_core::client_config::display_path(&Self::mcp_binary());
        let name = engram_core::client_config::server_name_for(&self.doc);
        engram_core::client_config::mcp_config(&name, self.store.path(), &binary)
    }

    pub fn flush(&mut self) {
        crate::views::memory::commit(self);
        crate::views::decisions::commit(self);
        crate::views::prompts::commit(self);
        crate::views::handoff::commit(self);
    }

    pub fn autosave(&mut self) {
        let wait = Duration::from_millis(900);
        if self.memory_draft.as_ref().is_some_and(|d| d.due(wait)) {
            crate::views::memory::commit(self);
        }
        if self.decision_draft.as_ref().is_some_and(|d| d.due(wait)) {
            crate::views::decisions::commit(self);
        }
        if self.prompt_draft.as_ref().is_some_and(|d| d.due(wait)) {
            crate::views::prompts::commit(self);
        }
        if self.handoff_draft.as_ref().is_some_and(|d| d.due(wait)) {
            crate::views::handoff::commit(self);
        }
    }
}
