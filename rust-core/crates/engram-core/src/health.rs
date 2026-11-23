use crate::analysis::{contains_phrase, polarity, sentences, terms, Polarity};
use crate::graph::Graph;
use crate::schema::*;
use crate::store::fingerprint;
use crate::time::{now_rfc3339, now_unix, parse_rfc3339};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

const STALE_HANDOFF_DAYS: i64 = 7;
const MAX_INDEXED_FILES: usize = 60_000;
const SKIP_DIRS: &[&str] = &[
    ".git",
    ".hg",
    ".svn",
    "node_modules",
    "target",
    ".build",
    "build",
    "dist",
    "DerivedData",
    ".next",
    ".venv",
    "venv",
    "__pycache__",
    "Pods",
    ".gradle",
    ".idea",
    ".swiftpm",
    "vendor",
];
const CODE_EXTENSIONS: &[&str] = &[
    "rs", "swift", "ts", "tsx", "js", "jsx", "mjs", "cjs", "py", "go", "java", "kt", "kts", "rb", "php", "cs", "c",
    "h", "hpp", "cpp", "cc", "m", "mm", "md", "json", "toml", "yaml", "yml", "sql", "css", "scss", "html", "sh",
    "proto", "graphql", "gql", "lock", "plist", "xml", "gradle", "vue", "svelte", "ex", "exs", "dart", "zig", "tf",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Error,
    Warning,
    Info,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IssueKind {
    MissingRepo,
    BrokenPath,
    ZombieReference,
    DeprecatedReference,
    DeprecatedDecision,
    Contradiction,
    RejectedAlternativeEndorsed,
    SupersededTagMismatch,
    DependencyCycle,
    StaleHandoff,
    EmptyContent,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize)]
pub struct EntityRef {
    pub kind: &'static str,
    pub id: String,
}

impl EntityRef {
    fn memory(id: &str) -> Self {
        Self {
            kind: "memory",
            id: id.into(),
        }
    }
    fn decision(id: &str) -> Self {
        Self {
            kind: "decision",
            id: id.into(),
        }
    }
    fn node(id: &str) -> Self {
        Self {
            kind: "node",
            id: id.into(),
        }
    }
    fn handoff() -> Self {
        Self {
            kind: "handoff",
            id: "handoff".into(),
        }
    }
    fn project() -> Self {
        Self {
            kind: "project",
            id: "project".into(),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Issue {
    pub id: String,
    pub severity: Severity,
    pub kind: IssueKind,
    pub message: String,
    pub subject: EntityRef,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub related: Option<EntityRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggestion: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct HealthSummary {
    pub errors: usize,
    pub warnings: usize,
    pub infos: usize,
    pub paths_checked: usize,
    pub references_checked: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct HealthReport {
    pub score: u8,
    pub grade: &'static str,
    pub repo_root: Option<String>,
    pub repo_found: bool,
    pub scanned_at: String,
    pub revision: u64,
    pub summary: HealthSummary,
    pub issues: Vec<Issue>,
}

struct Auditor<'a> {
    doc: &'a EngramDocument,
    issues: Vec<Issue>,
    summary: HealthSummary,
}

impl Auditor<'_> {
    #[allow(clippy::too_many_arguments)]
    fn push(
        &mut self,
        severity: Severity,
        kind: IssueKind,
        subject: EntityRef,
        message: String,
        related: Option<EntityRef>,
        path: Option<String>,
        suggestion: Option<String>,
    ) {
        let key = format!("{kind:?}|{}|{}|{:?}|{:?}", subject.kind, subject.id, related, path);
        let id = format!("{:016x}", fingerprint(key.as_bytes()));
        if self.issues.iter().any(|i| i.id == id) {
            return;
        }
        self.issues.push(Issue {
            id,
            severity,
            kind,
            message,
            subject,
            related,
            path,
            suggestion,
        });
    }
}

pub fn resolve_repo_root(doc: &EngramDocument, engram_dir: &Path, override_root: Option<&str>) -> (PathBuf, bool) {
    let configured = override_root
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .or(doc.project.repo_root.as_deref());
    match configured {
        Some(root) => (absolutize(engram_dir, root), true),
        None => (engram_dir.to_path_buf(), false),
    }
}

fn absolutize(base: &Path, p: &str) -> PathBuf {
    let p = p.trim();
    if let Some(rest) = p.strip_prefix("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home).join(rest);
        }
    }
    let path = Path::new(p);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        base.join(path)
    }
}

pub fn audit(doc: &EngramDocument, engram_dir: &Path, override_root: Option<&str>) -> HealthReport {
    let (root, explicit) = resolve_repo_root(doc, engram_dir, override_root);
    let repo_found = root.is_dir();
    let mut a = Auditor {
        doc,
        issues: Vec::new(),
        summary: HealthSummary::default(),
    };

    if !repo_found {
        a.push(
            Severity::Error,
            IssueKind::MissingRepo,
            EntityRef::project(),
            format!("Linked codebase directory does not exist: {}", root.display()),
            None,
            Some(root.display().to_string()),
            Some(
                if explicit {
                    "Update the repository root in Context Health."
                } else {
                    "Link the codebase directory."
                }
                .into(),
            ),
        );
    } else {
        check_paths(&mut a, &root);
    }
    check_references(&mut a);
    check_deprecated(&mut a);
    check_contradictions(&mut a);
    check_hygiene(&mut a);

    a.issues.sort_by(|x, y| {
        x.severity
            .cmp(&y.severity)
            .then_with(|| x.subject.id.cmp(&y.subject.id))
    });
    for i in &a.issues {
        match i.severity {
            Severity::Error => a.summary.errors += 1,
            Severity::Warning => a.summary.warnings += 1,
            Severity::Info => a.summary.infos += 1,
        }
    }
    let penalty = 15 * a.summary.errors + 5 * a.summary.warnings + a.summary.infos;
    let score = 100usize.saturating_sub(penalty) as u8;
    HealthReport {
        score,
        grade: match score {
            85.. => "healthy",
            60.. => "needs_attention",
            _ => "critical",
        },
        repo_root: Some(root.display().to_string()),
        repo_found,
        scanned_at: now_rfc3339(),
        revision: doc.revision,
        summary: a.summary,
        issues: a.issues,
    }
}

struct PathRef {
    subject: EntityRef,
    path: String,
    explicit: bool,
}

pub fn inline_paths(markdown: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut in_fence = false;
    for line in markdown.lines() {
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        let mut parts = line.split('`');
        parts.next();
        while let (Some(code), Some(_)) = (parts.next(), parts.next()) {
            if let Some(p) = normalize_path_candidate(code) {
                if !out.contains(&p) {
                    out.push(p);
                }
            }
        }
    }
    out
}

fn normalize_path_candidate(raw: &str) -> Option<String> {
    let s = raw.trim();
    if s.is_empty() || s.len() > 240 || s.contains(char::is_whitespace) || s.contains("://") || s.starts_with('-') {
        return None;
    }
    if s.contains([
        '*', '?', '{', '}', '$', '<', '>', '(', ')', '=', ',', '\'', '"', '|', '@',
    ]) {
        return None;
    }
    let s = s.split("#L").next().unwrap_or(s);
    let s = s.trim_end_matches(|c: char| c.is_ascii_digit() || c == ':');
    let s = s.trim_end_matches(['.', ',']);
    let file = s.rsplit('/').next().unwrap_or(s);
    let ext = file.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase());
    let has_code_ext = ext.as_deref().is_some_and(|e| CODE_EXTENSIONS.contains(&e));
    let looks_like_dir = s.contains('/') && s.split('/').filter(|p| !p.is_empty()).count() >= 2 && !s.starts_with("//");
    (has_code_ext || looks_like_dir).then(|| s.to_owned())
}

fn collect_path_refs(doc: &EngramDocument) -> Vec<PathRef> {
    let mut refs = Vec::new();
    let mut add = |subject: EntityRef, path: &str, explicit: bool| {
        let path = path.trim();
        if !path.is_empty() {
            refs.push(PathRef {
                subject,
                path: path.to_owned(),
                explicit,
            });
        }
    };
    for m in doc.memory.iter().filter(|m| m.is_active()) {
        m.files.iter().for_each(|f| add(EntityRef::memory(&m.key), f, true));
        inline_paths(m.body())
            .iter()
            .for_each(|f| add(EntityRef::memory(&m.key), f, false));
    }
    for d in doc.decisions.iter().filter(|d| d.is_active()) {
        d.files.iter().for_each(|f| add(EntityRef::decision(&d.id), f, true));
        for t in [&d.context, &d.decision, &d.consequences] {
            inline_paths(t)
                .iter()
                .for_each(|f| add(EntityRef::decision(&d.id), f, false));
        }
    }
    for n in &doc.architecture.nodes {
        if let Some(p) = &n.path {
            add(EntityRef::node(&n.id), p, true);
        }
    }
    for p in &doc.handoff.progress {
        p.files.iter().for_each(|f| add(EntityRef::handoff(), f, true));
    }
    refs
}

fn index_file_names(root: &Path) -> HashMap<String, Vec<String>> {
    let mut index: HashMap<String, Vec<String>> = HashMap::new();
    let mut stack = vec![root.to_path_buf()];
    let mut seen = 0usize;
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            let Ok(ft) = e.file_type() else { continue };
            if ft.is_dir() {
                if !SKIP_DIRS.contains(&name.as_str()) && !name.starts_with('.') {
                    stack.push(e.path());
                }
                continue;
            }
            seen += 1;
            if seen > MAX_INDEXED_FILES {
                return index;
            }
            if let Ok(rel) = e.path().strip_prefix(root) {
                index.entry(name).or_default().push(rel.to_string_lossy().into_owned());
            }
        }
    }
    index
}

fn check_paths(a: &mut Auditor, root: &Path) {
    let refs = collect_path_refs(a.doc);
    a.summary.paths_checked = refs.len();
    let missing: Vec<&PathRef> = refs.iter().filter(|r| !absolutize(root, &r.path).exists()).collect();
    if missing.is_empty() {
        return;
    }
    let index = index_file_names(root);
    for r in missing {
        let name = r.path.trim_end_matches('/').rsplit('/').next().unwrap_or(&r.path);
        let suggestion = index.get(name).map(|cands| {
            let mut c = cands.clone();
            c.sort_by_key(|p| p.len());
            c.truncate(3);
            format!("File may have moved: {}", c.join(", "))
        });
        let what = match r.subject.kind {
            "node" => format!(
                "Architecture node '{}' points at a path that no longer exists",
                r.subject.id
            ),
            "handoff" => "Handoff progress references a missing file".to_owned(),
            k => format!("{} '{}' references a missing path", capitalize(k), r.subject.id),
        };
        a.push(
            if r.explicit { Severity::Error } else { Severity::Warning },
            IssueKind::BrokenPath,
            r.subject.clone(),
            format!("{what}: {}", r.path),
            None,
            Some(r.path.clone()),
            suggestion.or_else(|| Some("Update or remove the reference.".into())),
        );
    }
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    c.next()
        .map(|f| f.to_uppercase().chain(c).collect())
        .unwrap_or_default()
}

fn check_references(a: &mut Auditor) {
    let doc = a.doc;
    let graph = Graph::new(&doc.architecture);
    for e in graph.missing_edges() {
        a.summary.references_checked += 1;
        a.push(
            Severity::Error,
            IssueKind::ZombieReference,
            EntityRef::node(&e.from),
            format!(
                "'{}' depends on '{}', which is not in the architecture map",
                e.from, e.to
            ),
            None,
            None,
            Some(format!("Add node '{}' or remove the dependency.", e.to)),
        );
    }
    for m in &doc.memory {
        for (field, target) in [("supersedes", &m.supersedes), ("superseded_by", &m.superseded_by)] {
            if let Some(t) = target {
                a.summary.references_checked += 1;
                if doc.memory_entry(t).is_none() {
                    a.push(
                        Severity::Warning,
                        IssueKind::ZombieReference,
                        EntityRef::memory(&m.key),
                        format!("'{}' {field} '{t}', which no longer exists", m.key),
                        None,
                        None,
                        Some("Clear the link or restore the entry.".into()),
                    );
                }
            }
        }
    }
    for d in &doc.decisions {
        for r in &d.related {
            a.summary.references_checked += 1;
            let memory = doc.memory_entry(r);
            let known = memory.is_some() || doc.node(r).is_some() || doc.decision(r).is_some();
            if !known {
                a.push(
                    Severity::Warning,
                    IssueKind::ZombieReference,
                    EntityRef::decision(&d.id),
                    format!("{} relates to '{r}', which does not exist", d.id),
                    None,
                    None,
                    Some("Remove the stale relation.".into()),
                );
            } else if let Some(m) = memory.filter(|m| !m.is_active() && d.is_active()) {
                a.push(
                    Severity::Warning,
                    IssueKind::ZombieReference,
                    EntityRef::decision(&d.id),
                    format!(
                        "{} relates to superseded memory '{}'{}",
                        d.id,
                        m.key,
                        m.superseded_by
                            .as_ref()
                            .map(|b| format!(" (replaced by '{b}')"))
                            .unwrap_or_default()
                    ),
                    Some(EntityRef::memory(&m.key)),
                    None,
                    m.superseded_by
                        .as_ref()
                        .map(|b| format!("Point the relation at '{b}'.")),
                );
            }
        }
        if let Some(by) = &d.superseded_by {
            a.summary.references_checked += 1;
            if doc.decision(by).is_none() {
                a.push(
                    Severity::Warning,
                    IssueKind::ZombieReference,
                    EntityRef::decision(&d.id),
                    format!("{} is superseded by {by}, which does not exist", d.id),
                    None,
                    None,
                    None,
                );
            }
        }
    }
    let superseded: Vec<&MemoryEntry> = doc.memory.iter().filter(|m| !m.is_active()).collect();
    for m in doc.memory.iter().filter(|m| m.is_active()) {
        for s in &superseded {
            if s.superseded_by.as_deref() != Some(m.key.as_str())
                && m.supersedes.as_deref() != Some(s.key.as_str())
                && m.content.contains(&format!("`{}`", s.key))
            {
                a.push(
                    Severity::Warning,
                    IssueKind::ZombieReference,
                    EntityRef::memory(&m.key),
                    format!("'{}' mentions superseded entry '{}'", m.key, s.key),
                    Some(EntityRef::memory(&s.key)),
                    None,
                    s.superseded_by.as_ref().map(|b| format!("Reference '{b}' instead.")),
                );
            }
        }
    }
}

fn check_deprecated(a: &mut Auditor) {
    let doc = a.doc;
    for old in doc.decisions.iter().filter(|d| !d.is_active()) {
        if old.superseded_by.is_none() {
            a.push(
                Severity::Info,
                IssueKind::DeprecatedDecision,
                EntityRef::decision(&old.id),
                format!("{} is deprecated without a replacement decision", old.id),
                None,
                None,
                Some("Log the decision that replaced it so agents know what to do instead.".into()),
            );
        }
        for d in doc.decisions.iter().filter(|d| d.is_active() && d.id != old.id) {
            let is_successor = old
                .superseded_by
                .as_deref()
                .is_some_and(|s| s.eq_ignore_ascii_case(&d.id));
            let refs = d.related.iter().any(|r| r.eq_ignore_ascii_case(&old.id))
                || contains_phrase(&format!("{} {}", d.context, d.decision), &old.id);
            if refs && !is_successor {
                a.push(
                    Severity::Warning,
                    IssueKind::DeprecatedReference,
                    EntityRef::decision(&d.id),
                    format!("Active {} builds on deprecated {}", d.id, old.id),
                    Some(EntityRef::decision(&old.id)),
                    None,
                    old.superseded_by.as_ref().map(|s| format!("Reference {s} instead.")),
                );
            }
        }
        for m in doc
            .memory
            .iter()
            .filter(|m| m.is_active() && contains_phrase(&m.content, &old.id))
        {
            a.push(
                Severity::Warning,
                IssueKind::DeprecatedReference,
                EntityRef::memory(&m.key),
                format!("'{}' cites deprecated {}", m.key, old.id),
                Some(EntityRef::decision(&old.id)),
                None,
                old.superseded_by.as_ref().map(|s| format!("Cite {s} instead.")),
            );
        }
        let h = &doc.handoff;
        let handoff_text = format!("{} {} {}", h.objective, h.next_step, h.working_memory.notes);
        if contains_phrase(&handoff_text, &old.id) {
            a.push(
                Severity::Warning,
                IssueKind::DeprecatedReference,
                EntityRef::handoff(),
                format!("The handoff cites deprecated {}", old.id),
                Some(EntityRef::decision(&old.id)),
                None,
                None,
            );
        }
    }
    for d in doc
        .decisions
        .iter()
        .filter(|d| d.is_active() && d.superseded_by.is_some())
    {
        a.push(
            Severity::Warning,
            IssueKind::DeprecatedDecision,
            EntityRef::decision(&d.id),
            format!("{} is marked superseded but its status is still active", d.id),
            None,
            None,
            Some("Set its status to deprecated.".into()),
        );
    }
}

struct Directive {
    owner: EntityRef,
    text: String,
    polarity: Polarity,
    terms: HashSet<String>,
}

fn directive_terms(sentence: &str) -> HashSet<String> {
    const VERBS: &[&str] = &[
        "use",
        "always",
        "never",
        "prefer",
        "avoid",
        "require",
        "required",
        "allow",
        "allowed",
        "ban",
        "forbid",
        "forbidden",
        "stop",
        "longer",
        "only",
        "everywhere",
        "anywhere",
        "instead",
        "adopt",
    ];
    terms(sentence)
        .into_iter()
        .filter(|t| !VERBS.contains(&t.as_str()))
        .collect()
}

fn check_contradictions(a: &mut Auditor) {
    let doc = a.doc;
    let mut directives: Vec<Directive> = Vec::new();
    for m in doc
        .memory
        .iter()
        .filter(|m| m.is_active() && m.category != MemoryCategory::Note)
    {
        for s in sentences(m.body()) {
            let p = polarity(&s);
            if p != Polarity::Neutral {
                directives.push(Directive {
                    owner: EntityRef::memory(&m.key),
                    terms: directive_terms(&s),
                    polarity: p,
                    text: s,
                });
            }
        }
    }
    for d in doc.decisions.iter().filter(|d| d.is_active()) {
        for s in sentences(&d.decision) {
            let p = polarity(&s);
            if p != Polarity::Neutral {
                directives.push(Directive {
                    owner: EntityRef::decision(&d.id),
                    terms: directive_terms(&s),
                    polarity: p,
                    text: s,
                });
            }
        }
    }
    for i in 0..directives.len() {
        for j in i + 1..directives.len() {
            let (x, y) = (&directives[i], &directives[j]);
            if x.owner == y.owner || x.polarity == y.polarity || x.terms.is_empty() || y.terms.is_empty() {
                continue;
            }
            let shared = x.terms.intersection(&y.terms).count();
            let union = x.terms.union(&y.terms).count();
            let diff = union - shared;
            if shared >= 1 && diff <= 1 && shared as f64 / union as f64 >= 0.6 {
                a.push(
                    Severity::Error,
                    IssueKind::Contradiction,
                    x.owner.clone(),
                    format!(
                        "Contradicting rules: \u{201c}{}\u{201d} vs \u{201c}{}\u{201d}",
                        x.text, y.text
                    ),
                    Some(y.owner.clone()),
                    None,
                    Some("Supersede one of the entries so only one rule stays canonical.".into()),
                );
            }
        }
    }
    for d in doc.decisions.iter().filter(|d| d.is_active()) {
        for alt in d
            .rejected_alternatives
            .iter()
            .filter(|alt| !alt.option.is_empty() && alt.option.split_whitespace().count() <= 5)
        {
            for dir in directives
                .iter()
                .filter(|x| x.polarity == Polarity::Positive && x.owner != EntityRef::decision(&d.id))
            {
                if contains_phrase(&dir.text, &alt.option) && !contains_phrase(&d.decision, &alt.option) {
                    a.push(
                        Severity::Warning,
                        IssueKind::RejectedAlternativeEndorsed,
                        dir.owner.clone(),
                        format!(
                            "\u{201c}{}\u{201d} endorses {}, which {} rejected: {}",
                            dir.text, alt.option, d.id, alt.reason
                        ),
                        Some(EntityRef::decision(&d.id)),
                        None,
                        Some(format!("Align with {} or log a new decision that supersedes it.", d.id)),
                    );
                }
            }
        }
    }
}

fn check_hygiene(a: &mut Auditor) {
    let doc = a.doc;
    for m in &doc.memory {
        if m.is_active() && has_superseded_tag(&m.content) {
            a.push(
                Severity::Warning,
                IssueKind::SupersededTagMismatch,
                EntityRef::memory(&m.key),
                format!(
                    "'{}' is tagged [SUPERSEDED] but still active, so it is still fed to agents",
                    m.key
                ),
                None,
                None,
                Some("Mark it superseded or remove the tag.".into()),
            );
        }
        if !m.is_active() && m.superseded_by.is_none() {
            a.push(
                Severity::Info,
                IssueKind::SupersededTagMismatch,
                EntityRef::memory(&m.key),
                format!("'{}' is superseded but does not say what replaced it", m.key),
                None,
                None,
                None,
            );
        }
        if m.is_active() && m.body().trim().is_empty() {
            a.push(
                Severity::Info,
                IssueKind::EmptyContent,
                EntityRef::memory(&m.key),
                format!("'{}' has no content", m.key),
                None,
                None,
                None,
            );
        }
    }
    for cycle in Graph::new(&doc.architecture).cycles() {
        a.push(
            Severity::Warning,
            IssueKind::DependencyCycle,
            EntityRef::node(&cycle[0]),
            format!("Dependency cycle: {} -> {}", cycle.join(" -> "), cycle[0]),
            None,
            None,
            Some("Break the cycle or document why it is intentional.".into()),
        );
    }
    if let Some(at) = doc.handoff.updated_at.as_deref().and_then(parse_rfc3339) {
        let days = (now_unix() - at) / 86_400;
        if days >= STALE_HANDOFF_DAYS && !doc.handoff.is_empty() {
            a.push(
                Severity::Info,
                IssueKind::StaleHandoff,
                EntityRef::handoff(),
                format!("The agent handoff was last updated {days} days ago"),
                None,
                None,
                Some("Refresh it before the next agent session.".into()),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops::Op;
    use serde_json::json;
    use std::fs;

    fn apply(doc: &mut EngramDocument, v: serde_json::Value) {
        serde_json::from_value::<Op>(v).unwrap().apply(doc).unwrap();
    }

    fn kinds(r: &HealthReport) -> Vec<IssueKind> {
        r.issues.iter().map(|i| i.kind).collect()
    }

    #[test]
    fn inline_path_extraction() {
        let md = "See `src/api/routes.ts:42` and `Cargo.toml`, not `npm run build` or `https://x.io/a/b`.\n```\n`ignored/in/fence.rs`\n```\nAlso `crates/core/` and `foo`.";
        assert_eq!(
            inline_paths(md),
            vec!["src/api/routes.ts", "Cargo.toml", "crates/core/"]
        );
    }

    #[test]
    fn healthy_document_scores_100() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("src")).unwrap();
        fs::write(dir.path().join("src/main.rs"), "").unwrap();
        let mut doc = EngramDocument::new("ok");
        apply(
            &mut doc,
            json!({"op":"upsert_memory","key":"entry","category":"rule","content":"Entry point lives in `src/main.rs`.","files":["src"]}),
        );
        apply(&mut doc, json!({"op":"add_architecture_node","id":"core","path":"src"}));
        let r = audit(&doc, dir.path(), None);
        assert_eq!(r.issues.len(), 0, "{:#?}", r.issues);
        assert_eq!((r.score, r.grade), (100, "healthy"));
        assert_eq!(r.summary.paths_checked, 3);
    }

    #[test]
    fn detects_broken_paths_with_move_suggestions() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("pkg/auth")).unwrap();
        fs::write(dir.path().join("pkg/auth/session.go"), "").unwrap();
        let mut doc = EngramDocument::new("x");
        apply(
            &mut doc,
            json!({"op":"upsert_memory","key":"auth","category":"invariant","content":"Sessions are created in `internal/session.go`.","files":["internal/session.go"]}),
        );
        apply(
            &mut doc,
            json!({"op":"add_architecture_node","id":"auth","path":"internal/auth"}),
        );
        apply(
            &mut doc,
            json!({"op":"update_handoff","objective":"o","next_step":"n","progress":[{"description":"d","files":["gone.rs"]}]}),
        );
        let r = audit(&doc, dir.path(), None);
        let broken: Vec<&Issue> = r.issues.iter().filter(|i| i.kind == IssueKind::BrokenPath).collect();
        assert_eq!(
            broken.len(),
            3,
            "explicit and inline refs to one path collapse into a single issue"
        );
        assert!(broken
            .iter()
            .any(|i| i.suggestion.as_deref() == Some("File may have moved: pkg/auth/session.go")));
        assert!(r.score < 100);
    }

    #[test]
    fn missing_repo_is_reported() {
        let doc = EngramDocument::new("x");
        let r = audit(&doc, Path::new("/"), Some("/definitely/not/here"));
        assert_eq!(kinds(&r), vec![IssueKind::MissingRepo]);
        assert!(!r.repo_found);
    }

    #[test]
    fn detects_zombies_deprecated_refs_and_cycles() {
        let dir = tempfile::tempdir().unwrap();
        let mut doc = EngramDocument::new("x");
        apply(
            &mut doc,
            json!({"op":"add_architecture_node","id":"a","dependencies":["b","ghost"]}),
        );
        apply(
            &mut doc,
            json!({"op":"add_architecture_node","id":"b","dependencies":["a"]}),
        );
        apply(
            &mut doc,
            json!({"op":"upsert_memory","key":"old","category":"rule","content":"Old rule"}),
        );
        apply(
            &mut doc,
            json!({"op":"upsert_memory","key":"new","category":"rule","content":"New rule","supersedes_key":"old"}),
        );
        apply(
            &mut doc,
            json!({"op":"log_decision","context":"c","decision":"Pick A","related":["old","nothing"]}),
        );
        apply(
            &mut doc,
            json!({"op":"log_decision","context":"c","decision":"Pick B","supersedes":"ADR-0001"}),
        );
        apply(
            &mut doc,
            json!({"op":"log_decision","context":"Builds on ADR-0001","decision":"Pick C"}),
        );
        apply(
            &mut doc,
            json!({"op":"upsert_memory","key":"cite","category":"note","content":"Per ADR-0001 we pick A"}),
        );
        let r = audit(&doc, dir.path(), None);
        let k = kinds(&r);
        assert!(k.contains(&IssueKind::ZombieReference));
        assert!(r.issues.iter().any(|i| i.message.contains("'ghost'")));
        assert!(r.issues.iter().any(|i| i.kind == IssueKind::DependencyCycle));
        let deprecated_refs: Vec<_> = r
            .issues
            .iter()
            .filter(|i| i.kind == IssueKind::DeprecatedReference)
            .map(|i| i.subject.id.as_str())
            .collect();
        assert!(deprecated_refs.contains(&"ADR-0003"), "{deprecated_refs:?}");
        assert!(deprecated_refs.contains(&"cite"));
        assert!(
            !deprecated_refs.contains(&"ADR-0002"),
            "the successor may reference its predecessor"
        );
    }

    #[test]
    fn detects_contradictions_and_endorsed_rejections() {
        let dir = tempfile::tempdir().unwrap();
        let mut doc = EngramDocument::new("x");
        apply(
            &mut doc,
            json!({"op":"upsert_memory","key":"tabs","category":"convention","content":"Always use tabs for indentation."}),
        );
        apply(
            &mut doc,
            json!({"op":"upsert_memory","key":"spaces","category":"convention","content":"Never use tabs for indentation."}),
        );
        apply(
            &mut doc,
            json!({"op":"upsert_memory","key":"db","category":"tech_stack","content":"Use Postgres for payments data."}),
        );
        apply(
            &mut doc,
            json!({"op":"upsert_memory","key":"db2","category":"tech_stack","content":"Never use MySQL for payments data."}),
        );
        apply(
            &mut doc,
            json!({"op":"upsert_memory","key":"state","category":"rule","content":"Prefer Redux for global state."}),
        );
        apply(
            &mut doc,
            json!({"op":"log_decision","context":"state","decision":"Use Zustand.","rejected_alternatives":[{"option":"Redux","reason":"boilerplate"}]}),
        );
        apply(
            &mut doc,
            json!({"op":"upsert_memory","key":"tagged","category":"rule","content":"x"}),
        );
        doc.memory_entry_mut("tagged").unwrap().content = "[SUPERSEDED] x".into();
        let r = audit(&doc, dir.path(), None);
        let contradictions: Vec<&Issue> = r.issues.iter().filter(|i| i.kind == IssueKind::Contradiction).collect();
        assert_eq!(contradictions.len(), 1, "{contradictions:#?}");
        assert_eq!(contradictions[0].subject.id, "tabs");
        let endorsed: Vec<&Issue> = r
            .issues
            .iter()
            .filter(|i| i.kind == IssueKind::RejectedAlternativeEndorsed)
            .collect();
        assert_eq!(endorsed.len(), 1);
        assert_eq!(endorsed[0].subject.id, "state");
        assert!(kinds(&r).contains(&IssueKind::SupersededTagMismatch));
    }
}
