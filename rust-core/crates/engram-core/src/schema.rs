use crate::error::{invalid, Result};
use crate::text::Text;
use crate::time::now_rfc3339;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub const FORMAT_MAGIC: &str = "engram";
pub const FORMAT_VERSION: u32 = 1;
pub const SUPERSEDED_TAG: &str = "[SUPERSEDED]";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EngramDocument {
    pub format: String,
    pub version: u32,
    #[serde(default)]
    pub revision: u64,
    pub project: Project,
    #[serde(default)]
    pub memory: Vec<MemoryEntry>,
    #[serde(default)]
    pub architecture: Architecture,
    #[serde(default)]
    pub decisions: Vec<Decision>,
    #[serde(default)]
    pub prompts: Vec<Prompt>,
    #[serde(default)]
    pub handoff: Handoff,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Project {
    pub name: String,
    #[serde(default)]
    pub description: Text,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo_root: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryCategory {
    Rule,
    TechStack,
    Invariant,
    Convention,
    Note,
}

impl MemoryCategory {
    pub const ALL: [MemoryCategory; 5] = [
        Self::Rule,
        Self::TechStack,
        Self::Invariant,
        Self::Convention,
        Self::Note,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Rule => "rule",
            Self::TechStack => "tech_stack",
            Self::Invariant => "invariant",
            Self::Convention => "convention",
            Self::Note => "note",
        }
    }

    pub fn parse(s: &str) -> Result<Self> {
        let norm: String = s
            .trim()
            .to_ascii_lowercase()
            .chars()
            .map(|c| if c == '-' || c == ' ' { '_' } else { c })
            .collect();
        Ok(match norm.trim_end_matches('s') {
            "rule" | "policy" | "constraint" => Self::Rule,
            "tech_stack" | "stack" | "tech" | "technology" | "dependency" => Self::TechStack,
            "invariant" | "architecture" | "architecture_invariant" => Self::Invariant,
            "convention" | "style" | "pattern" => Self::Convention,
            "note" | "context" | "fact" => Self::Note,
            _ => {
                let valid: Vec<_> = Self::ALL.iter().map(|c| c.as_str()).collect();
                return invalid(format!(
                    "unknown memory category '{s}' (expected one of {})",
                    valid.join(", ")
                ));
            }
        })
    }

    pub fn weight(self) -> f64 {
        match self {
            Self::Invariant => 1.5,
            Self::Rule => 1.35,
            Self::TechStack => 1.2,
            Self::Convention => 1.1,
            Self::Note => 0.9,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntryStatus {
    #[default]
    Active,
    Superseded,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub key: String,
    pub category: MemoryCategory,
    #[serde(default)]
    pub status: EntryStatus,
    #[serde(default, skip_serializing_if = "is_false")]
    pub pinned: bool,
    pub content: Text,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supersedes: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub superseded_by: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl MemoryEntry {
    pub fn is_active(&self) -> bool {
        self.status == EntryStatus::Active
    }

    pub fn body(&self) -> &str {
        strip_superseded_tag(&self.content)
    }
}

pub fn strip_superseded_tag(s: &str) -> &str {
    let t = s.trim_start();
    match t.get(..SUPERSEDED_TAG.len()) {
        Some(head) if head.eq_ignore_ascii_case(SUPERSEDED_TAG) => t[SUPERSEDED_TAG.len()..].trim_start(),
        _ => s,
    }
}

pub fn has_superseded_tag(s: &str) -> bool {
    strip_superseded_tag(s).len() != s.len()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    Module,
    Service,
    Database,
    Library,
    Queue,
    Ui,
    External,
}

impl NodeKind {
    pub const ALL: [NodeKind; 7] = [
        Self::Module,
        Self::Service,
        Self::Database,
        Self::Library,
        Self::Queue,
        Self::Ui,
        Self::External,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Module => "module",
            Self::Service => "service",
            Self::Database => "database",
            Self::Library => "library",
            Self::Queue => "queue",
            Self::Ui => "ui",
            Self::External => "external",
        }
    }

    pub fn parse(s: &str) -> Result<Self> {
        Ok(match s.trim().to_ascii_lowercase().as_str() {
            "module" | "package" | "crate" | "component" => Self::Module,
            "service" | "api" | "server" | "worker" => Self::Service,
            "database" | "db" | "store" | "datastore" | "cache" => Self::Database,
            "library" | "lib" | "sdk" => Self::Library,
            "queue" | "bus" | "topic" | "stream" => Self::Queue,
            "ui" | "frontend" | "app" | "client" => Self::Ui,
            "external" | "third_party" | "third-party" | "saas" => Self::External,
            _ => {
                let valid: Vec<_> = Self::ALL.iter().map(|k| k.as_str()).collect();
                return invalid(format!(
                    "unknown node type '{s}' (expected one of {})",
                    valid.join(", ")
                ));
            }
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ArchNode {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: NodeKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default)]
    pub description: Text,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default)]
    pub depends_on: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<Point>,
}

impl ArchNode {
    pub fn display_name(&self) -> &str {
        self.label.as_deref().filter(|l| !l.is_empty()).unwrap_or(&self.id)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Architecture {
    #[serde(default)]
    pub nodes: Vec<ArchNode>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionStatus {
    #[default]
    Active,
    Deprecated,
}

impl DecisionStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Deprecated => "deprecated",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RejectedAlternative {
    pub option: String,
    pub reason: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Decision {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub status: DecisionStatus,
    pub context: Text,
    pub decision: Text,
    #[serde(default)]
    pub rejected_alternatives: Vec<RejectedAlternative>,
    #[serde(default, skip_serializing_if = "Text::is_empty")]
    pub consequences: Text,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub related: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub superseded_by: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl Decision {
    pub fn is_active(&self) -> bool {
        self.status == DecisionStatus::Active
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Prompt {
    pub name: String,
    #[serde(default)]
    pub category: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    pub body: Text,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RenderedPrompt {
    pub text: String,
    pub missing: Vec<String>,
}

impl Prompt {
    pub fn variables(&self) -> Vec<String> {
        let mut seen = HashSet::new();
        placeholders(&self.body)
            .filter_map(|(_, name)| seen.insert(name).then(|| name.to_owned()))
            .collect()
    }

    pub fn render(&self, vars: &serde_json::Map<String, serde_json::Value>) -> RenderedPrompt {
        let body: &str = &self.body;
        let mut out = String::with_capacity(body.len());
        let mut missing: Vec<String> = Vec::new();
        let mut last = 0;
        for (range, name) in placeholders(body) {
            out.push_str(&body[last..range.start]);
            match vars.get(name) {
                Some(serde_json::Value::String(s)) => out.push_str(s),
                Some(serde_json::Value::Null) | None => {
                    out.push_str(&body[range.clone()]);
                    if !missing.iter().any(|m| m == name) {
                        missing.push(name.to_owned());
                    }
                }
                Some(other) => out.push_str(&other.to_string()),
            }
            last = range.end;
        }
        out.push_str(&body[last..]);
        RenderedPrompt { text: out, missing }
    }
}

fn placeholders(s: &str) -> impl Iterator<Item = (std::ops::Range<usize>, &str)> {
    let mut pos = 0;
    std::iter::from_fn(move || loop {
        let start = pos + s.get(pos..)?.find("{{")?;
        let close = start + 2 + s[start + 2..].find("}}")?;
        pos = close + 2;
        let name = s[start + 2..close].trim();
        if !name.is_empty()
            && name
                .chars()
                .all(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '.'))
        {
            return Some((start..close + 2, name));
        }
    })
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProgressItem {
    pub description: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<String>,
    #[serde(default = "default_true")]
    pub verified: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct WorkingMemory {
    #[serde(default)]
    pub hypotheses: Vec<String>,
    #[serde(default)]
    pub blockers: Vec<String>,
    #[serde(default, skip_serializing_if = "Text::is_empty")]
    pub notes: Text,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Handoff {
    #[serde(default)]
    pub objective: String,
    #[serde(default)]
    pub progress: Vec<ProgressItem>,
    #[serde(default)]
    pub working_memory: WorkingMemory,
    #[serde(default)]
    pub next_step: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_by: Option<String>,
}

impl Handoff {
    pub fn is_empty(&self) -> bool {
        self.objective.is_empty()
            && self.progress.is_empty()
            && self.next_step.is_empty()
            && self.working_memory.hypotheses.is_empty()
            && self.working_memory.blockers.is_empty()
            && self.working_memory.notes.is_empty()
    }
}

impl EngramDocument {
    pub fn new(project_name: impl Into<String>) -> Self {
        let now = now_rfc3339();
        Self {
            format: FORMAT_MAGIC.into(),
            version: FORMAT_VERSION,
            revision: 0,
            project: Project {
                name: project_name.into(),
                description: Text::default(),
                repo_root: None,
                created_at: now.clone(),
                updated_at: now,
            },
            memory: Vec::new(),
            architecture: Architecture::default(),
            decisions: Vec::new(),
            prompts: Vec::new(),
            handoff: Handoff::default(),
        }
    }

    pub fn memory_entry(&self, key: &str) -> Option<&MemoryEntry> {
        self.memory.iter().find(|m| m.key == key)
    }
    pub fn memory_entry_mut(&mut self, key: &str) -> Option<&mut MemoryEntry> {
        self.memory.iter_mut().find(|m| m.key == key)
    }
    pub fn node(&self, id: &str) -> Option<&ArchNode> {
        self.architecture.nodes.iter().find(|n| n.id == id)
    }
    pub fn node_mut(&mut self, id: &str) -> Option<&mut ArchNode> {
        self.architecture.nodes.iter_mut().find(|n| n.id == id)
    }
    pub fn decision(&self, id: &str) -> Option<&Decision> {
        self.decisions.iter().find(|d| d.id.eq_ignore_ascii_case(id))
    }
    pub fn decision_mut(&mut self, id: &str) -> Option<&mut Decision> {
        self.decisions.iter_mut().find(|d| d.id.eq_ignore_ascii_case(id))
    }
    pub fn prompt(&self, name: &str) -> Option<&Prompt> {
        self.prompts
            .iter()
            .find(|p| p.name == name)
            .or_else(|| self.prompts.iter().find(|p| p.name.eq_ignore_ascii_case(name)))
    }

    pub fn next_decision_id(&self) -> String {
        let max = self
            .decisions
            .iter()
            .filter_map(|d| d.id.rsplit('-').next()?.parse::<u32>().ok())
            .max()
            .unwrap_or(0);
        format!("ADR-{:04}", max + 1)
    }

    pub fn validate(&self) -> Result<()> {
        if self.format != FORMAT_MAGIC {
            return invalid(format!("format must be \"{FORMAT_MAGIC}\", found \"{}\"", self.format));
        }
        if self.version > FORMAT_VERSION {
            return invalid(format!(
                "file uses format version {} but this build supports up to {FORMAT_VERSION}",
                self.version
            ));
        }
        if self.project.name.trim().is_empty() {
            return invalid("project.name must not be empty");
        }
        unique("memory key", self.memory.iter().map(|m| m.key.as_str()))?;
        unique(
            "architecture node id",
            self.architecture.nodes.iter().map(|n| n.id.as_str()),
        )?;
        unique("decision id", self.decisions.iter().map(|d| d.id.as_str()))?;
        unique("prompt name", self.prompts.iter().map(|p| p.name.as_str()))?;
        for n in &self.architecture.nodes {
            if n.depends_on.iter().any(|d| d == &n.id) {
                return invalid(format!("architecture node '{}' depends on itself", n.id));
            }
        }
        Ok(())
    }
}

fn unique<'a>(what: &str, items: impl Iterator<Item = &'a str>) -> Result<()> {
    let mut seen = HashSet::new();
    for item in items {
        if item.trim().is_empty() {
            return invalid(format!("{what} must not be empty"));
        }
        if !seen.insert(item.to_ascii_lowercase()) {
            return invalid(format!("duplicate {what} '{item}'"));
        }
    }
    Ok(())
}

impl Text {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

fn is_false(b: &bool) -> bool {
    !*b
}

fn default_true() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn prompt(body: &str) -> Prompt {
        Prompt {
            name: "p".into(),
            category: String::new(),
            description: String::new(),
            body: body.into(),
            created_at: String::new(),
            updated_at: String::new(),
        }
    }

    #[test]
    fn prompt_variables_and_render() {
        let p = prompt("Fix {{ bug }} in {{file}}. Again: {{bug}}. Not a var: {{two words}} {{}}");
        assert_eq!(p.variables(), vec!["bug", "file"]);
        let vars = json!({"bug": "the crash", "file": 3}).as_object().unwrap().clone();
        let r = p.render(&vars);
        assert_eq!(
            r.text,
            "Fix the crash in 3. Again: the crash. Not a var: {{two words}} {{}}"
        );
        assert!(r.missing.is_empty());
        let r = p.render(&serde_json::Map::new());
        assert_eq!(r.missing, vec!["bug", "file"]);
        assert!(r.text.contains("{{ bug }}"));
    }

    #[test]
    fn superseded_tag_detection() {
        assert!(has_superseded_tag("[SUPERSEDED] use v1"));
        assert!(has_superseded_tag("  [superseded]use v1"));
        assert!(!has_superseded_tag("use v1 [SUPERSEDED]"));
        assert_eq!(strip_superseded_tag("[SUPERSEDED]  use v1"), "use v1");
    }

    #[test]
    fn category_and_kind_parsing() {
        assert_eq!(MemoryCategory::parse("Tech Stack").unwrap(), MemoryCategory::TechStack);
        assert_eq!(MemoryCategory::parse("rules").unwrap(), MemoryCategory::Rule);
        assert!(MemoryCategory::parse("banana").is_err());
        assert_eq!(NodeKind::parse("DB").unwrap(), NodeKind::Database);
        assert!(NodeKind::parse("?").is_err());
    }

    #[test]
    fn validation_catches_duplicates_and_self_edges() {
        let mut doc = EngramDocument::new("x");
        doc.validate().unwrap();
        doc.prompts.push(prompt("a"));
        doc.prompts.push(prompt("b"));
        assert!(doc
            .validate()
            .unwrap_err()
            .to_string()
            .contains("duplicate prompt name"));
        doc.prompts.pop();
        doc.architecture.nodes.push(ArchNode {
            id: "a".into(),
            kind: NodeKind::Module,
            label: None,
            description: Text::default(),
            path: None,
            depends_on: vec!["a".into()],
            position: None,
        });
        assert!(doc.validate().is_err());
    }

    #[test]
    fn next_decision_id_increments() {
        let mut doc = EngramDocument::new("x");
        assert_eq!(doc.next_decision_id(), "ADR-0001");
        doc.decisions.push(Decision {
            id: "ADR-0041".into(),
            title: "t".into(),
            status: DecisionStatus::Active,
            context: Text::default(),
            decision: Text::default(),
            rejected_alternatives: vec![],
            consequences: Text::default(),
            related: vec![],
            files: vec![],
            superseded_by: None,
            created_at: String::new(),
            updated_at: String::new(),
        });
        assert_eq!(doc.next_decision_id(), "ADR-0042");
    }
}
