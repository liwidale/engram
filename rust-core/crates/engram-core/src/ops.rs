use crate::error::{invalid, EngramError, Result};
use crate::schema::*;
use crate::store::{Snapshot, Store};
use crate::text::Text;
use crate::time::now_rfc3339;
use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Op {
    UpdateProject(UpdateProject),
    UpsertMemory(UpsertMemory),
    RenameMemory {
        key: String,
        new_key: String,
    },
    SetMemoryStatus {
        key: String,
        status: EntryStatus,
    },
    DeleteMemory {
        key: String,
    },
    AddArchitectureNode(AddNode),
    RemoveArchitectureNode {
        id: String,
    },
    SetNodePosition {
        id: String,
        #[serde(default)]
        position: Option<Point>,
    },
    AddDependency {
        from: String,
        to: String,
    },
    RemoveDependency {
        from: String,
        to: String,
    },
    LogDecision(LogDecision),
    UpdateDecision(UpdateDecision),
    DeleteDecision {
        id: String,
    },
    UpsertPrompt(UpsertPrompt),
    DeletePrompt {
        name: String,
    },
    UpdateHandoff(UpdateHandoff),
    ResetAllNodePositions,
    ReplacePath {
        subject_kind: String,
        subject_id: String,
        from: String,
        to: String,
    },
    UnlinkMemory {
        key: String,
        link: MemoryLink,
    },
    ReplaceRelation {
        id: String,
        from: String,
        #[serde(default)]
        to: Option<String>,
    },
    Batch {
        ops: Vec<Op>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryLink {
    Supersedes,
    SupersededBy,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct UpdateProject {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub repo_root: Option<String>,
    #[serde(default, deserialize_with = "present")]
    pub stale_handoff_days: Option<Option<u32>>,
    #[serde(default)]
    pub health_ignore_dirs: Option<Vec<String>>,
    #[serde(default)]
    pub health_ignore_issues: Option<Vec<String>>,
    #[serde(default, deserialize_with = "present")]
    pub context_min_score: Option<Option<f64>>,
    #[serde(default, deserialize_with = "present")]
    pub mcp_server_name: Option<Option<String>>,
}

fn present<'de, T, D>(d: D) -> std::result::Result<Option<Option<T>>, D::Error>
where
    T: Deserialize<'de>,
    D: serde::Deserializer<'de>,
{
    Option::<T>::deserialize(d).map(Some)
}

#[derive(Clone, Debug, Deserialize)]
pub struct UpsertMemory {
    pub key: String,
    #[serde(default)]
    pub category: Option<String>,
    pub content: String,
    #[serde(default)]
    pub supersedes_key: Option<String>,
    #[serde(default)]
    pub tags: Option<Vec<String>>,
    #[serde(default)]
    pub files: Option<Vec<String>>,
    #[serde(default)]
    pub pinned: Option<bool>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct AddNode {
    pub id: String,
    #[serde(rename = "type", alias = "kind", default)]
    pub node_type: Option<String>,
    #[serde(alias = "depends_on", default)]
    pub dependencies: Option<Vec<String>>,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub path: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(untagged)]
pub enum AlternativeInput {
    Full {
        #[serde(alias = "name", alias = "alternative")]
        option: String,
        #[serde(default, alias = "why")]
        reason: String,
    },
    Line(String),
}

impl AlternativeInput {
    fn into_alternative(self) -> RejectedAlternative {
        match self {
            Self::Full { option, reason } => RejectedAlternative {
                option: option.trim().into(),
                reason: reason.trim().into(),
            },
            Self::Line(s) => {
                let split = s.find(" - ").map(|i| (i, 3)).or_else(|| s.find(':').map(|i| (i, 1)));
                match split {
                    Some((i, w)) => RejectedAlternative {
                        option: s[..i].trim().into(),
                        reason: s[i + w..].trim().into(),
                    },
                    None => RejectedAlternative {
                        option: s.trim().into(),
                        reason: String::new(),
                    },
                }
            }
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct LogDecision {
    #[serde(default)]
    pub title: Option<String>,
    pub context: String,
    pub decision: String,
    #[serde(default)]
    pub rejected_alternatives: Vec<AlternativeInput>,
    #[serde(default)]
    pub consequences: Option<String>,
    #[serde(default)]
    pub related: Vec<String>,
    #[serde(default)]
    pub files: Vec<String>,
    #[serde(default)]
    pub supersedes: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct UpdateDecision {
    pub id: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub status: Option<DecisionStatus>,
    #[serde(default)]
    pub context: Option<String>,
    #[serde(default)]
    pub decision: Option<String>,
    #[serde(default)]
    pub rejected_alternatives: Option<Vec<AlternativeInput>>,
    #[serde(default)]
    pub consequences: Option<String>,
    #[serde(default)]
    pub related: Option<Vec<String>>,
    #[serde(default)]
    pub files: Option<Vec<String>>,
    #[serde(default)]
    pub superseded_by: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct UpsertPrompt {
    pub name: String,
    pub body: String,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub rename_from: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(untagged)]
pub enum ProgressInput {
    Full {
        description: String,
        #[serde(default)]
        files: Vec<String>,
        #[serde(default = "yes")]
        verified: bool,
    },
    Line(String),
}

#[derive(Clone, Debug, Deserialize)]
#[serde(untagged)]
pub enum WorkingMemoryInput {
    Full {
        #[serde(default)]
        hypotheses: Vec<String>,
        #[serde(default)]
        blockers: Vec<String>,
        #[serde(default)]
        notes: String,
    },
    Notes(String),
}

#[derive(Clone, Debug, Deserialize)]
pub struct UpdateHandoff {
    pub objective: String,
    #[serde(default)]
    pub progress: Vec<ProgressInput>,
    #[serde(default)]
    pub working_memory: Option<WorkingMemoryInput>,
    pub next_step: String,
    #[serde(default)]
    pub updated_by: Option<String>,
}

fn yes() -> bool {
    true
}

impl Op {
    pub fn execute(self, store: &Store) -> Result<(Value, Snapshot)> {
        store.transact(|doc| self.apply(doc))
    }

    pub fn execute_tracked(self, store: &Store) -> Result<(Value, EngramDocument, Snapshot)> {
        let ((value, before), snap) = store.transact(|doc| {
            let before = doc.clone();
            self.apply(doc).map(|v| (v, before))
        })?;
        Ok((value, before, snap))
    }

    pub fn label(name: &str) -> &'static str {
        match name {
            "update_project" => "Edit project settings",
            "upsert_memory" => "Edit memory",
            "rename_memory" => "Rename memory entry",
            "set_memory_status" => "Change memory status",
            "delete_memory" => "Delete memory entry",
            "add_architecture_node" => "Edit architecture node",
            "remove_architecture_node" => "Delete architecture node",
            "set_node_position" | "reset_all_node_positions" => "Move nodes",
            "add_dependency" | "remove_dependency" => "Change dependency",
            "log_decision" => "Log decision",
            "update_decision" | "replace_relation" => "Edit decision",
            "delete_decision" => "Delete decision",
            "upsert_prompt" => "Edit prompt",
            "delete_prompt" => "Delete prompt",
            "update_handoff" => "Edit handoff",
            "replace_path" | "unlink_memory" => "Apply fix",
            "batch" => "Import",
            _ => "Change",
        }
    }

    pub fn apply(self, doc: &mut EngramDocument) -> Result<Value> {
        let now = now_rfc3339();
        match self {
            Op::UpdateProject(u) => update_project(doc, u),
            Op::UpsertMemory(m) => upsert_memory(doc, m, &now),
            Op::RenameMemory { key, new_key } => {
                let new_key = required("new_key", &new_key)?;
                if doc.memory_entry(&key).is_none() {
                    return Err(EngramError::NotFound(format!("memory entry '{key}'")));
                }
                if key != new_key && doc.memory_entry(&new_key).is_some() {
                    return Err(EngramError::Conflict(format!(
                        "memory entry '{new_key}' already exists"
                    )));
                }
                for m in &mut doc.memory {
                    if m.key == key {
                        m.key = new_key.clone();
                        m.updated_at = now.clone();
                    }
                    for link in [&mut m.supersedes, &mut m.superseded_by] {
                        if link.as_deref() == Some(key.as_str()) {
                            *link = Some(new_key.clone());
                        }
                    }
                }
                for d in &mut doc.decisions {
                    d.related
                        .iter_mut()
                        .filter(|r| **r == key)
                        .for_each(|r| *r = new_key.clone());
                }
                Ok(json!({ "key": new_key, "renamed_from": key }))
            }
            Op::SetMemoryStatus { key, status } => {
                let m = doc
                    .memory_entry_mut(&key)
                    .ok_or_else(|| EngramError::NotFound(format!("memory entry '{key}'")))?;
                if m.status != status {
                    m.status = status;
                    m.updated_at = now;
                    if status == EntryStatus::Active {
                        m.superseded_by = None;
                        m.content = strip_superseded_tag(&m.content).to_owned().into();
                    }
                }
                Ok(json!({ "key": key, "status": status }))
            }
            Op::DeleteMemory { key } => {
                let before = doc.memory.len();
                doc.memory.retain(|m| m.key != key);
                if doc.memory.len() == before {
                    return Err(EngramError::NotFound(format!("memory entry '{key}'")));
                }
                for m in &mut doc.memory {
                    if m.supersedes.as_deref() == Some(key.as_str()) {
                        m.supersedes = None;
                    }
                }
                Ok(json!({ "deleted": key }))
            }
            Op::AddArchitectureNode(n) => add_node(doc, n),
            Op::RemoveArchitectureNode { id } => {
                let before = doc.architecture.nodes.len();
                doc.architecture.nodes.retain(|n| n.id != id);
                if doc.architecture.nodes.len() == before {
                    return Err(EngramError::NotFound(format!("architecture node '{id}'")));
                }
                let mut detached = Vec::new();
                for n in &mut doc.architecture.nodes {
                    if n.depends_on.contains(&id) {
                        n.depends_on.retain(|d| *d != id);
                        detached.push(n.id.clone());
                    }
                }
                Ok(json!({ "deleted": id, "detached_from": detached }))
            }
            Op::ResetAllNodePositions => {
                let mut reset = 0;
                for n in &mut doc.architecture.nodes {
                    if n.position.take().is_some() {
                        reset += 1;
                    }
                }
                Ok(json!({ "reset": reset }))
            }
            Op::ReplacePath {
                subject_kind,
                subject_id,
                from,
                to,
            } => replace_path(doc, &subject_kind, &subject_id, &from, &to, &now),
            Op::UnlinkMemory { key, link } => {
                let m = doc
                    .memory_entry_mut(&key)
                    .ok_or_else(|| EngramError::NotFound(format!("memory entry '{key}'")))?;
                match link {
                    MemoryLink::Supersedes => m.supersedes = None,
                    MemoryLink::SupersededBy => m.superseded_by = None,
                }
                m.updated_at = now;
                Ok(json!({ "key": key }))
            }
            Op::ReplaceRelation { id, from, to } => {
                let d = doc
                    .decision_mut(&id)
                    .ok_or_else(|| EngramError::NotFound(format!("decision '{id}'")))?;
                let Some(pos) = d.related.iter().position(|r| *r == from) else {
                    return Err(EngramError::NotFound(format!("relation '{from}' on {id}")));
                };
                match to.map(|t| t.trim().to_owned()).filter(|t| !t.is_empty()) {
                    Some(t) if !d.related.contains(&t) => d.related[pos] = t,
                    _ => {
                        d.related.remove(pos);
                    }
                }
                d.updated_at = now;
                Ok(json!({ "id": id }))
            }
            Op::Batch { ops } => {
                let mut results = Vec::with_capacity(ops.len());
                for op in ops {
                    results.push(op.apply(doc)?);
                }
                Ok(json!({ "results": results }))
            }
            Op::SetNodePosition { id, position } => {
                let n = doc
                    .node_mut(&id)
                    .ok_or_else(|| EngramError::NotFound(format!("architecture node '{id}'")))?;
                n.position = position.map(|p| Point {
                    x: p.x.round(),
                    y: p.y.round(),
                });
                Ok(json!({ "id": id }))
            }
            Op::AddDependency { from, to } => {
                if from == to {
                    return invalid("a node cannot depend on itself");
                }
                if doc.node(&to).is_none() {
                    return Err(EngramError::NotFound(format!("architecture node '{to}'")));
                }
                let n = doc
                    .node_mut(&from)
                    .ok_or_else(|| EngramError::NotFound(format!("architecture node '{from}'")))?;
                if !n.depends_on.contains(&to) {
                    n.depends_on.push(to.clone());
                }
                Ok(json!({ "from": from, "to": to }))
            }
            Op::RemoveDependency { from, to } => {
                let n = doc
                    .node_mut(&from)
                    .ok_or_else(|| EngramError::NotFound(format!("architecture node '{from}'")))?;
                n.depends_on.retain(|d| *d != to);
                Ok(json!({ "from": from, "to": to }))
            }
            Op::LogDecision(d) => log_decision(doc, d, &now),
            Op::UpdateDecision(u) => update_decision(doc, u, &now),
            Op::DeleteDecision { id } => {
                let before = doc.decisions.len();
                doc.decisions.retain(|d| !d.id.eq_ignore_ascii_case(&id));
                if doc.decisions.len() == before {
                    return Err(EngramError::NotFound(format!("decision '{id}'")));
                }
                Ok(json!({ "deleted": id }))
            }
            Op::UpsertPrompt(p) => upsert_prompt(doc, p, &now),
            Op::DeletePrompt { name } => {
                let before = doc.prompts.len();
                doc.prompts.retain(|p| p.name != name);
                if doc.prompts.len() == before {
                    return Err(EngramError::NotFound(format!("prompt '{name}'")));
                }
                Ok(json!({ "deleted": name }))
            }
            Op::UpdateHandoff(h) => {
                let objective = required("objective", &h.objective)?;
                let next_step = required("next_step", &h.next_step)?;
                let progress = h
                    .progress
                    .into_iter()
                    .map(|p| match p {
                        ProgressInput::Full {
                            description,
                            files,
                            verified,
                        } => ProgressItem {
                            description,
                            files: clean_list(files),
                            verified,
                        },
                        ProgressInput::Line(description) => ProgressItem {
                            description,
                            files: Vec::new(),
                            verified: true,
                        },
                    })
                    .filter(|p| !p.description.trim().is_empty())
                    .collect();
                let working_memory = match h.working_memory {
                    Some(WorkingMemoryInput::Full {
                        hypotheses,
                        blockers,
                        notes,
                    }) => WorkingMemory {
                        hypotheses: clean_list(hypotheses),
                        blockers: clean_list(blockers),
                        notes: notes.into(),
                    },
                    Some(WorkingMemoryInput::Notes(notes)) => WorkingMemory {
                        notes: notes.into(),
                        ..Default::default()
                    },
                    None => WorkingMemory::default(),
                };
                doc.handoff = Handoff {
                    objective,
                    progress,
                    working_memory,
                    next_step,
                    updated_at: Some(now.clone()),
                    updated_by: h.updated_by.filter(|s| !s.trim().is_empty()),
                };
                Ok(json!({ "updated_at": now }))
            }
        }
    }
}

fn update_project(doc: &mut EngramDocument, u: UpdateProject) -> Result<Value> {
    if let Some(name) = u.name {
        if name.trim().is_empty() {
            return invalid("project name must not be empty");
        }
        doc.project.name = name.trim().into();
    }
    if let Some(d) = u.description {
        doc.project.description = d.into();
    }
    if let Some(root) = u.repo_root {
        doc.project.repo_root = Some(root.trim().to_owned()).filter(|r| !r.is_empty());
    }
    let settings = &mut doc.project.settings;
    if let Some(days) = u.stale_handoff_days {
        if days == Some(0) {
            return invalid("stale_handoff_days must be at least 1");
        }
        settings.stale_handoff_days = days;
    }
    if let Some(dirs) = u.health_ignore_dirs {
        settings.health_ignore_dirs = clean_list(dirs.into_iter().map(|d| d.trim_matches('/').to_owned()).collect());
    }
    if let Some(issues) = u.health_ignore_issues {
        settings.health_ignore_issues = clean_list(issues);
    }
    if let Some(score) = u.context_min_score {
        if score.is_some_and(|s| !(0.0..=10.0).contains(&s)) {
            return invalid("context_min_score must be between 0 and 10");
        }
        settings.context_min_score = score;
    }
    if let Some(name) = u.mcp_server_name {
        settings.mcp_server_name = name.map(|n| n.trim().to_owned()).filter(|n| !n.is_empty());
    }
    Ok(json!({ "project": doc.project.name }))
}

fn swap_path(list: &mut [String], from: &str, to: &str) -> bool {
    let mut hit = false;
    for p in list.iter_mut().filter(|p| p.as_str() == from) {
        *p = to.to_owned();
        hit = true;
    }
    hit
}

fn swap_inline(text: &mut Text, from: &str, to: &str) -> bool {
    let needle = format!("`{from}`");
    if !text.contains(&needle) {
        return false;
    }
    *text = text.replace(&needle, &format!("`{to}`")).into();
    true
}

fn replace_path(doc: &mut EngramDocument, kind: &str, id: &str, from: &str, to: &str, now: &str) -> Result<Value> {
    let to = required("to", to)?;
    let missing = || EngramError::NotFound(format!("{kind} '{id}'"));
    let changed = match kind {
        "memory" => {
            let m = doc.memory_entry_mut(id).ok_or_else(missing)?;
            let hit = swap_path(&mut m.files, from, &to) | swap_inline(&mut m.content, from, &to);
            if hit {
                m.updated_at = now.to_owned();
            }
            hit
        }
        "decision" => {
            let d = doc.decision_mut(id).ok_or_else(missing)?;
            let mut hit = swap_path(&mut d.files, from, &to);
            for t in [&mut d.context, &mut d.decision, &mut d.consequences] {
                hit |= swap_inline(t, from, &to);
            }
            if hit {
                d.updated_at = now.to_owned();
            }
            hit
        }
        "node" => {
            let n = doc.node_mut(id).ok_or_else(missing)?;
            let hit = n.path.as_deref() == Some(from);
            if hit {
                n.path = Some(to.clone());
            }
            hit
        }
        "handoff" => doc
            .handoff
            .progress
            .iter_mut()
            .fold(false, |hit, p| swap_path(&mut p.files, from, &to) | hit),
        other => return invalid(format!("unknown subject kind '{other}'")),
    };
    if !changed {
        return Err(EngramError::NotFound(format!("path '{from}' in {kind} '{id}'")));
    }
    Ok(json!({ "from": from, "to": to }))
}

fn required(field: &str, value: &str) -> Result<String> {
    let v = value.trim();
    if v.is_empty() {
        return invalid(format!("{field} must not be empty"));
    }
    Ok(v.to_owned())
}

fn clean_list(items: Vec<String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::with_capacity(items.len());
    for s in items {
        let s = s.trim();
        if !s.is_empty() && !out.iter().any(|o| o == s) {
            out.push(s.to_owned());
        }
    }
    out
}

fn upsert_memory(doc: &mut EngramDocument, m: UpsertMemory, now: &str) -> Result<Value> {
    let key = required("key", &m.key)?;
    let category = m
        .category
        .as_deref()
        .filter(|c| !c.trim().is_empty())
        .map(MemoryCategory::parse)
        .transpose()?;
    let tagged_superseded = has_superseded_tag(&m.content);
    let mut warnings: Vec<String> = Vec::new();

    if let Some(old) = m.supersedes_key.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        if old == key {
            return invalid("an entry cannot supersede itself");
        }
        let prev = doc
            .memory_entry_mut(old)
            .ok_or_else(|| EngramError::NotFound(format!("memory entry '{old}' (supersedes_key)")))?;
        if let Some(by) = prev.superseded_by.as_deref().filter(|b| *b != key) {
            warnings.push(format!("'{old}' was already superseded by '{by}'"));
        }
        prev.status = EntryStatus::Superseded;
        prev.superseded_by = Some(key.clone());
        prev.updated_at = now.to_owned();
    }

    let created = match doc.memory_entry_mut(&key) {
        Some(e) => {
            if let Some(c) = category {
                e.category = c;
            }
            let before = e.clone();
            e.content = m.content.into();
            if let Some(t) = m.tags {
                e.tags = clean_list(t);
            }
            if let Some(f) = m.files {
                e.files = clean_list(f);
            }
            if let Some(p) = m.pinned {
                e.pinned = p;
            }
            if let Some(s) = m.supersedes_key.filter(|s| !s.trim().is_empty()) {
                e.supersedes = Some(s.trim().into());
            }
            if tagged_superseded {
                e.status = EntryStatus::Superseded;
            } else if let Some(by) = &e.superseded_by {
                warnings.push(format!(
                    "'{key}' is superseded by '{by}'; it stays excluded from context"
                ));
            }
            if *e != before {
                e.updated_at = now.to_owned();
            }
            false
        }
        None => {
            let Some(category) = category else {
                return invalid("category is required when creating a memory entry");
            };
            doc.memory.push(MemoryEntry {
                key: key.clone(),
                category,
                status: if tagged_superseded {
                    EntryStatus::Superseded
                } else {
                    EntryStatus::Active
                },
                pinned: m.pinned.unwrap_or(false),
                content: m.content.into(),
                tags: clean_list(m.tags.unwrap_or_default()),
                files: clean_list(m.files.unwrap_or_default()),
                supersedes: m.supersedes_key.map(|s| s.trim().to_owned()).filter(|s| !s.is_empty()),
                superseded_by: None,
                created_at: now.to_owned(),
                updated_at: now.to_owned(),
            });
            true
        }
    };
    let e = doc.memory_entry(&key).expect("just upserted");
    Ok(json!({ "key": key, "created": created, "status": e.status, "supersedes": e.supersedes, "warnings": warnings }))
}

fn add_node(doc: &mut EngramDocument, n: AddNode) -> Result<Value> {
    let id = required("id", &n.id)?;
    if id.chars().any(char::is_whitespace) {
        return invalid(format!("node id '{id}' must not contain whitespace"));
    }
    let kind = n
        .node_type
        .as_deref()
        .filter(|t| !t.trim().is_empty())
        .map(NodeKind::parse)
        .transpose()?;
    let deps = n.dependencies.map(clean_list);
    if deps.as_ref().is_some_and(|d| d.contains(&id)) {
        return invalid(format!("node '{id}' cannot depend on itself"));
    }
    let unknown: Vec<String> = deps
        .iter()
        .flatten()
        .filter(|d| doc.node(d).is_none())
        .cloned()
        .collect();
    let created = match doc.node_mut(&id) {
        Some(node) => {
            if let Some(k) = kind {
                node.kind = k;
            }
            if let Some(d) = deps {
                node.depends_on = d;
            }
            if let Some(l) = n.label {
                node.label = Some(l.trim().to_owned()).filter(|l| !l.is_empty());
            }
            if let Some(d) = n.description {
                node.description = d.into();
            }
            if let Some(p) = n.path {
                node.path = Some(p.trim().to_owned()).filter(|p| !p.is_empty());
            }
            false
        }
        None => {
            doc.architecture.nodes.push(ArchNode {
                id: id.clone(),
                kind: kind.unwrap_or(NodeKind::Module),
                label: n.label.map(|l| l.trim().to_owned()).filter(|l| !l.is_empty()),
                description: n.description.map(Text::from).unwrap_or_default(),
                path: n.path.map(|p| p.trim().to_owned()).filter(|p| !p.is_empty()),
                depends_on: deps.unwrap_or_default(),
                position: None,
            });
            true
        }
    };
    let node = doc.node(&id).expect("just upserted");
    let warnings: Vec<String> = unknown
        .iter()
        .map(|d| format!("dependency '{d}' is not a known node yet; add it with add_architecture_node"))
        .collect();
    Ok(json!({ "id": id, "created": created, "type": node.kind, "depends_on": node.depends_on, "warnings": warnings }))
}

fn derive_title(decision: &str) -> String {
    let first = decision
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("Untitled decision");
    let first = first.trim_start_matches('#').trim();
    let sentence = first
        .split_inclusive(['.', '!', '?'])
        .next()
        .unwrap_or(first)
        .trim_end_matches(['.', '!', '?']);
    let mut title: String = sentence.chars().take(80).collect();
    if sentence.chars().count() > 80 {
        title.push('…');
    }
    title
}

fn log_decision(doc: &mut EngramDocument, d: LogDecision, now: &str) -> Result<Value> {
    let context = required("context", &d.context)?;
    let decision = required("decision", &d.decision)?;
    let id = doc.next_decision_id();
    if let Some(old) = d.supersedes.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        let prev = doc
            .decision_mut(old)
            .ok_or_else(|| EngramError::NotFound(format!("decision '{old}' (supersedes)")))?;
        prev.status = DecisionStatus::Deprecated;
        prev.superseded_by = Some(id.clone());
        prev.updated_at = now.to_owned();
    }
    let mut related = clean_list(d.related);
    if let Some(old) = d.supersedes.map(|s| s.trim().to_owned()).filter(|s| !s.is_empty()) {
        if !related.contains(&old) {
            related.push(old);
        }
    }
    let title = d
        .title
        .map(|t| t.trim().to_owned())
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| derive_title(&decision));
    let rejected: Vec<RejectedAlternative> = d
        .rejected_alternatives
        .into_iter()
        .map(AlternativeInput::into_alternative)
        .filter(|a| !a.option.is_empty())
        .collect();
    doc.decisions.push(Decision {
        id: id.clone(),
        title: title.clone(),
        status: DecisionStatus::Active,
        context: context.into(),
        decision: decision.into(),
        rejected_alternatives: rejected,
        consequences: d.consequences.unwrap_or_default().into(),
        related,
        files: clean_list(d.files),
        superseded_by: None,
        created_at: now.to_owned(),
        updated_at: now.to_owned(),
    });
    Ok(json!({ "id": id, "title": title }))
}

fn update_decision(doc: &mut EngramDocument, u: UpdateDecision, now: &str) -> Result<Value> {
    let d = doc
        .decision_mut(&u.id)
        .ok_or_else(|| EngramError::NotFound(format!("decision '{}'", u.id)))?;
    let before = d.clone();
    if let Some(t) = u.title {
        d.title = required("title", &t)?;
    }
    if let Some(s) = u.status {
        d.status = s;
        if s == DecisionStatus::Active {
            d.superseded_by = None;
        }
    }
    if let Some(c) = u.context {
        d.context = c.into();
    }
    if let Some(x) = u.decision {
        d.decision = x.into();
    }
    if let Some(r) = u.rejected_alternatives {
        d.rejected_alternatives = r
            .into_iter()
            .map(AlternativeInput::into_alternative)
            .filter(|a| !a.option.is_empty())
            .collect();
    }
    if let Some(c) = u.consequences {
        d.consequences = c.into();
    }
    if let Some(r) = u.related {
        d.related = clean_list(r);
    }
    if let Some(f) = u.files {
        d.files = clean_list(f);
    }
    if let Some(s) = u.superseded_by {
        d.superseded_by = Some(s.trim().to_owned()).filter(|s| !s.is_empty());
        if d.superseded_by.is_some() {
            d.status = DecisionStatus::Deprecated;
        }
    }
    if *d != before {
        d.updated_at = now.to_owned();
    }
    Ok(json!({ "id": d.id, "status": d.status }))
}

fn upsert_prompt(doc: &mut EngramDocument, p: UpsertPrompt, now: &str) -> Result<Value> {
    let name = required("name", &p.name)?;
    let lookup = p
        .rename_from
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(&name)
        .to_owned();
    if lookup != name && doc.prompts.iter().any(|x| x.name == name) {
        return Err(EngramError::Conflict(format!("prompt '{name}' already exists")));
    }
    let created = match doc.prompts.iter_mut().find(|x| x.name == lookup) {
        Some(existing) => {
            let before = existing.clone();
            existing.name = name.clone();
            existing.body = p.body.into();
            if let Some(c) = p.category {
                existing.category = c.trim().into();
            }
            if let Some(d) = p.description {
                existing.description = d.trim().into();
            }
            if *existing != before {
                existing.updated_at = now.to_owned();
            }
            false
        }
        None if lookup != name => return Err(EngramError::NotFound(format!("prompt '{lookup}'"))),
        None => {
            doc.prompts.push(Prompt {
                name: name.clone(),
                category: p.category.unwrap_or_default().trim().into(),
                description: p.description.unwrap_or_default().trim().into(),
                body: p.body.into(),
                created_at: now.to_owned(),
                updated_at: now.to_owned(),
            });
            true
        }
    };
    let vars = doc.prompt(&name).map(Prompt::variables).unwrap_or_default();
    Ok(json!({ "name": name, "created": created, "variables": vars }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn op(v: Value) -> Op {
        serde_json::from_value(v).unwrap()
    }

    fn doc() -> EngramDocument {
        EngramDocument::new("Test")
    }

    #[test]
    fn upsert_memory_create_update_and_supersede() {
        let mut d = doc();
        let r = op(json!({"op":"upsert_memory","key":"db","category":"tech stack","content":"Use SQLite"}))
            .apply(&mut d)
            .unwrap();
        assert_eq!(r["created"], true);
        assert!(op(json!({"op":"upsert_memory","key":"x","content":"no category"}))
            .apply(&mut d)
            .is_err());

        let r = op(json!({"op":"upsert_memory","key":"db-v2","category":"tech_stack","content":"Use Postgres 16","supersedes_key":"db"}))
            .apply(&mut d)
            .unwrap();
        assert_eq!(r["supersedes"], "db");
        let old = d.memory_entry("db").unwrap();
        assert_eq!(old.status, EntryStatus::Superseded);
        assert_eq!(old.superseded_by.as_deref(), Some("db-v2"));

        op(json!({"op":"upsert_memory","key":"db-v2","content":"Use Postgres 17","pinned":true}))
            .apply(&mut d)
            .unwrap();
        let e = d.memory_entry("db-v2").unwrap();
        assert_eq!(
            (e.category, e.pinned, e.content.as_str()),
            (MemoryCategory::TechStack, true, "Use Postgres 17")
        );

        op(json!({"op":"upsert_memory","key":"old","category":"rule","content":"[SUPERSEDED] Tabs"}))
            .apply(&mut d)
            .unwrap();
        assert_eq!(d.memory_entry("old").unwrap().status, EntryStatus::Superseded);
        op(json!({"op":"set_memory_status","key":"old","status":"active"}))
            .apply(&mut d)
            .unwrap();
        let e = d.memory_entry("old").unwrap();
        assert_eq!((e.status, e.content.as_str()), (EntryStatus::Active, "Tabs"));

        assert_eq!(
            op(json!({"op":"upsert_memory","key":"a","category":"rule","content":"x","supersedes_key":"missing"}))
                .apply(&mut d)
                .unwrap_err()
                .code(),
            "not_found"
        );
    }

    #[test]
    fn rename_memory_rewrites_references() {
        let mut d = doc();
        op(json!({"op":"upsert_memory","key":"a","category":"rule","content":"x"}))
            .apply(&mut d)
            .unwrap();
        op(json!({"op":"upsert_memory","key":"b","category":"rule","content":"y","supersedes_key":"a"}))
            .apply(&mut d)
            .unwrap();
        op(json!({"op":"log_decision","context":"c","decision":"d","related":["b"]}))
            .apply(&mut d)
            .unwrap();
        op(json!({"op":"rename_memory","key":"b","new_key":"b2"}))
            .apply(&mut d)
            .unwrap();
        assert_eq!(d.memory_entry("a").unwrap().superseded_by.as_deref(), Some("b2"));
        assert_eq!(d.decisions[0].related, vec!["b2"]);
        assert_eq!(
            op(json!({"op":"rename_memory","key":"a","new_key":"b2"}))
                .apply(&mut d)
                .unwrap_err()
                .code(),
            "conflict"
        );
    }

    #[test]
    fn architecture_ops() {
        let mut d = doc();
        let r = op(json!({"op":"add_architecture_node","id":"api","type":"service","dependencies":["db","db"]}))
            .apply(&mut d)
            .unwrap();
        assert_eq!(r["warnings"].as_array().unwrap().len(), 1);
        assert_eq!(d.node("api").unwrap().depends_on, vec!["db"]);
        op(json!({"op":"add_architecture_node","id":"db","type":"database"}))
            .apply(&mut d)
            .unwrap();
        op(json!({"op":"add_architecture_node","id":"ui","kind":"frontend","depends_on":["api"]}))
            .apply(&mut d)
            .unwrap();
        assert_eq!(d.node("ui").unwrap().kind, NodeKind::Ui);
        assert!(op(json!({"op":"add_dependency","from":"db","to":"nowhere"}))
            .apply(&mut d)
            .is_err());
        op(json!({"op":"add_dependency","from":"ui","to":"db"}))
            .apply(&mut d)
            .unwrap();
        op(json!({"op":"set_node_position","id":"ui","position":{"x":10.4,"y":20.6}}))
            .apply(&mut d)
            .unwrap();
        assert_eq!(d.node("ui").unwrap().position, Some(Point { x: 10.0, y: 21.0 }));
        let r = op(json!({"op":"remove_architecture_node","id":"db"}))
            .apply(&mut d)
            .unwrap();
        assert_eq!(r["detached_from"], json!(["api", "ui"]));
        assert!(d.node("ui").unwrap().depends_on == vec!["api"]);
        assert!(op(json!({"op":"add_architecture_node","id":"bad id"}))
            .apply(&mut d)
            .is_err());
        assert!(op(json!({"op":"add_architecture_node","id":"x","dependencies":["x"]}))
            .apply(&mut d)
            .is_err());
    }

    #[test]
    fn decisions_log_supersede_and_update() {
        let mut d = doc();
        let r = op(json!({
            "op":"log_decision",
            "context":"We need client state.",
            "decision":"Use Zustand for client state. It is small.",
            "rejected_alternatives":[{"option":"Redux","reason":"boilerplate"}, "MobX: implicit reactivity", "Recoil"]
        }))
        .apply(&mut d)
        .unwrap();
        assert_eq!(r["id"], "ADR-0001");
        let adr = &d.decisions[0];
        assert_eq!(adr.title, "Use Zustand for client state");
        assert_eq!(
            adr.rejected_alternatives[1],
            RejectedAlternative {
                option: "MobX".into(),
                reason: "implicit reactivity".into()
            }
        );
        assert_eq!(adr.rejected_alternatives[2].reason, "");

        op(json!({"op":"log_decision","title":"Signals","context":"perf","decision":"Use signals","supersedes":"adr-0001"})).apply(&mut d).unwrap();
        assert_eq!(d.decisions[0].status, DecisionStatus::Deprecated);
        assert_eq!(d.decisions[0].superseded_by.as_deref(), Some("ADR-0002"));
        assert_eq!(d.decisions[1].related, vec!["adr-0001"]);

        op(json!({"op":"update_decision","id":"ADR-0001","status":"active"}))
            .apply(&mut d)
            .unwrap();
        assert_eq!(d.decisions[0].superseded_by, None);
        op(json!({"op":"delete_decision","id":"ADR-0002"}))
            .apply(&mut d)
            .unwrap();
        assert_eq!(d.next_decision_id(), "ADR-0002");
    }

    #[test]
    fn prompts_and_handoff() {
        let mut d = doc();
        let r =
            op(json!({"op":"upsert_prompt","name":"review","category":"QA","body":"Review {{file}} for {{concern}}"}))
                .apply(&mut d)
                .unwrap();
        assert_eq!(r["variables"], json!(["file", "concern"]));
        op(json!({"op":"upsert_prompt","name":"code-review","rename_from":"review","body":"Review {{file}}"}))
            .apply(&mut d)
            .unwrap();
        assert_eq!(d.prompts.len(), 1);
        assert_eq!(d.prompts[0].category, "QA");
        assert!(
            op(json!({"op":"upsert_prompt","name":"z","rename_from":"nope","body":""}))
                .apply(&mut d)
                .is_err()
        );

        op(json!({
            "op":"update_handoff",
            "objective":"Ship sync",
            "progress":["Wrote schema", {"description":"Added tests","files":["src/a.rs"],"verified":false}],
            "working_memory":{"hypotheses":["race in watcher"],"blockers":[]},
            "next_step":"Fix watcher",
            "updated_by":"agent"
        }))
        .apply(&mut d)
        .unwrap();
        assert_eq!(d.handoff.progress.len(), 2);
        assert!(!d.handoff.progress[1].verified);
        assert_eq!(d.handoff.working_memory.hypotheses, vec!["race in watcher"]);
        assert!(d.handoff.updated_at.is_some());
        assert!(op(json!({"op":"update_handoff","objective":" ","next_step":"x"}))
            .apply(&mut d)
            .is_err());
        op(json!({"op":"update_handoff","objective":"o","next_step":"n","working_memory":"just notes"}))
            .apply(&mut d)
            .unwrap();
        assert_eq!(d.handoff.working_memory.notes.as_str(), "just notes");
        assert!(d.handoff.progress.is_empty(), "handoff is a full overwrite");
    }

    #[test]
    fn execute_persists_through_store() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path().join("p.engram"));
        store.create("P").unwrap();
        let (r, snap) = op(json!({"op":"upsert_memory","key":"k","category":"rule","content":"x"}))
            .execute(&store)
            .unwrap();
        assert_eq!(r["created"], true);
        assert_eq!(snap.document.revision, 2);
        assert_eq!(store.load().unwrap().document.memory.len(), 1);
        let err = op(json!({"op":"delete_memory","key":"missing"}))
            .execute(&store)
            .unwrap_err();
        assert_eq!(err.code(), "not_found");
        assert_eq!(store.load().unwrap().document.revision, 2);
    }

    #[test]
    fn reset_all_positions_and_batches_are_atomic() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path().join("p.engram"));
        store.create("P").unwrap();
        op(json!({"op":"batch","ops":[
            {"op":"add_architecture_node","id":"a","type":"module"},
            {"op":"add_architecture_node","id":"b","type":"module"},
            {"op":"set_node_position","id":"a","position":{"x":1,"y":2}},
            {"op":"set_node_position","id":"b","position":{"x":3,"y":4}}
        ]}))
        .execute(&store)
        .unwrap();
        let rev = store.load().unwrap().document.revision;
        assert_eq!(rev, 2, "a batch is one transaction");

        let (r, snap) = op(json!({"op":"reset_all_node_positions"})).execute(&store).unwrap();
        assert_eq!(r["reset"], 2);
        assert!(snap.document.architecture.nodes.iter().all(|n| n.position.is_none()));
        assert_eq!(snap.document.revision, rev + 1);

        let err = op(json!({"op":"batch","ops":[
            {"op":"upsert_memory","key":"k","category":"rule","content":"x"},
            {"op":"delete_memory","key":"missing"}
        ]}))
        .execute(&store)
        .unwrap_err();
        assert_eq!(err.code(), "not_found");
        assert!(
            store.load().unwrap().document.memory.is_empty(),
            "a failed batch writes nothing"
        );
    }

    #[test]
    fn project_settings_update_and_clear() {
        let mut d = doc();
        op(json!({"op":"update_project","stale_handoff_days":3,"context_min_score":0.3,"mcp_server_name":" mem ","health_ignore_issues":["a","a",""]}))
            .apply(&mut d)
            .unwrap();
        let s = &d.project.settings;
        assert_eq!(
            (s.stale_handoff_days, s.context_min_score, s.mcp_server_name.as_deref()),
            (Some(3), Some(0.3), Some("mem"))
        );
        assert_eq!(s.health_ignore_issues, vec!["a"]);
        op(json!({"op":"update_project","name":"Renamed"}))
            .apply(&mut d)
            .unwrap();
        assert_eq!(
            d.project.settings.stale_handoff_days,
            Some(3),
            "absent fields are left alone"
        );
        op(json!({"op":"update_project","stale_handoff_days":null,"context_min_score":null,"mcp_server_name":"","health_ignore_issues":[]}))
            .apply(&mut d)
            .unwrap();
        assert!(d.project.settings.is_default());
        assert!(op(json!({"op":"update_project","stale_handoff_days":0}))
            .apply(&mut d)
            .is_err());
        assert!(op(json!({"op":"update_project","context_min_score":-1}))
            .apply(&mut d)
            .is_err());
    }

    #[test]
    fn replace_path_and_relation_ops() {
        let mut d = doc();
        op(json!({"op":"upsert_memory","key":"m","category":"rule","content":"See `a.rs` and `a.rs`.","files":["a.rs"]})).apply(&mut d).unwrap();
        op(json!({"op":"replace_path","subject_kind":"memory","subject_id":"m","from":"a.rs","to":"src/a.rs"}))
            .apply(&mut d)
            .unwrap();
        let m = d.memory_entry("m").unwrap();
        assert_eq!(m.files, vec!["src/a.rs"]);
        assert_eq!(m.content.as_str(), "See `src/a.rs` and `src/a.rs`.");
        assert!(
            op(json!({"op":"replace_path","subject_kind":"memory","subject_id":"m","from":"zzz","to":"y"}))
                .apply(&mut d)
                .is_err()
        );
        op(json!({"op":"log_decision","context":"c","decision":"d","related":["x","y"]}))
            .apply(&mut d)
            .unwrap();
        op(json!({"op":"replace_relation","id":"ADR-0001","from":"x","to":"y"}))
            .apply(&mut d)
            .unwrap();
        assert_eq!(d.decisions[0].related, vec!["y"], "duplicates collapse");
        op(json!({"op":"replace_relation","id":"ADR-0001","from":"y"}))
            .apply(&mut d)
            .unwrap();
        assert!(d.decisions[0].related.is_empty());
    }
}
