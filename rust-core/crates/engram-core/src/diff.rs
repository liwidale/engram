use crate::schema::*;
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeKind {
    Added,
    Updated,
    Deleted,
    Superseded,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityKind {
    Project,
    Memory,
    Node,
    Decision,
    Prompt,
    Handoff,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Change {
    pub kind: ChangeKind,
    pub entity: EntityKind,
    pub id: String,
    pub title: String,
    pub fields: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct DocumentDiff {
    pub from_revision: u64,
    pub to_revision: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    pub changes: Vec<Change>,
}

impl DocumentDiff {
    pub fn is_empty(&self) -> bool {
        self.changes.is_empty()
    }

    pub fn summary(&self) -> String {
        let count = |k: ChangeKind| self.changes.iter().filter(|c| c.kind == k).count();
        let parts: Vec<String> = [
            (ChangeKind::Added, "added"),
            (ChangeKind::Updated, "updated"),
            (ChangeKind::Superseded, "superseded"),
            (ChangeKind::Deleted, "deleted"),
        ]
        .into_iter()
        .filter_map(|(k, label)| {
            let n = count(k);
            (n > 0).then(|| format!("{n} {label}"))
        })
        .collect();
        if parts.is_empty() {
            "No content changes".into()
        } else {
            parts.join(", ")
        }
    }
}

const VOLATILE: &[&str] = &["updated_at", "position"];

fn changed_fields<T: Serialize>(old: &T, new: &T) -> Vec<String> {
    let (Ok(Value::Object(a)), Ok(Value::Object(b))) = (serde_json::to_value(old), serde_json::to_value(new)) else {
        return Vec::new();
    };
    let keys: BTreeSet<&String> = a.keys().chain(b.keys()).collect();
    keys.into_iter()
        .filter(|k| !VOLATILE.contains(&k.as_str()) && a.get(*k) != b.get(*k))
        .cloned()
        .collect()
}

fn diff_list<T: Serialize>(
    out: &mut Vec<Change>,
    entity: EntityKind,
    old: &[T],
    new: &[T],
    id: impl Fn(&T) -> &str,
    title: impl Fn(&T) -> String,
    retired: impl Fn(&T, &T) -> bool,
) {
    for n in new {
        match old.iter().find(|o| id(o) == id(n)) {
            None => out.push(Change {
                kind: ChangeKind::Added,
                entity,
                id: id(n).into(),
                title: title(n),
                fields: vec![],
            }),
            Some(o) => {
                let fields = changed_fields(o, n);
                if !fields.is_empty() {
                    let kind = if retired(o, n) {
                        ChangeKind::Superseded
                    } else {
                        ChangeKind::Updated
                    };
                    out.push(Change {
                        kind,
                        entity,
                        id: id(n).into(),
                        title: title(n),
                        fields,
                    });
                }
            }
        }
    }
    for o in old.iter().filter(|o| !new.iter().any(|n| id(n) == id(o))) {
        out.push(Change {
            kind: ChangeKind::Deleted,
            entity,
            id: id(o).into(),
            title: title(o),
            fields: vec![],
        });
    }
}

fn summary_line(text: &str) -> String {
    let line = text
        .lines()
        .map(|l| l.trim().trim_start_matches(['#', '-', '*', '>', ' ']))
        .find(|l| !l.is_empty())
        .unwrap_or("");
    let mut out: String = line.chars().take(80).collect();
    if line.chars().count() > 80 {
        out.push('…');
    }
    out
}

pub fn diff(old: &EngramDocument, new: &EngramDocument) -> DocumentDiff {
    let mut changes = Vec::new();
    let project_fields = changed_fields(&old.project, &new.project);
    if !project_fields.is_empty() {
        changes.push(Change {
            kind: ChangeKind::Updated,
            entity: EntityKind::Project,
            id: "project".into(),
            title: new.project.name.clone(),
            fields: project_fields,
        });
    }
    diff_list(
        &mut changes,
        EntityKind::Memory,
        &old.memory,
        &new.memory,
        |m| &m.key,
        |m| summary_line(m.body()),
        |o, n| o.is_active() && !n.is_active(),
    );
    diff_list(
        &mut changes,
        EntityKind::Node,
        &old.architecture.nodes,
        &new.architecture.nodes,
        |n| &n.id,
        |n| n.display_name().to_owned(),
        |_, _| false,
    );
    diff_list(
        &mut changes,
        EntityKind::Decision,
        &old.decisions,
        &new.decisions,
        |d| &d.id,
        |d| d.title.clone(),
        |o, n| o.is_active() && !n.is_active(),
    );
    diff_list(
        &mut changes,
        EntityKind::Prompt,
        &old.prompts,
        &new.prompts,
        |p| &p.name,
        |p| p.description.clone(),
        |_, _| false,
    );
    let handoff_fields = changed_fields(&old.handoff, &new.handoff);
    let handoff_changed = !handoff_fields.is_empty();
    if handoff_changed {
        changes.push(Change {
            kind: if old.handoff.is_empty() {
                ChangeKind::Added
            } else {
                ChangeKind::Updated
            },
            entity: EntityKind::Handoff,
            id: "handoff".into(),
            title: summary_line(&new.handoff.objective),
            fields: handoff_fields,
        });
    }
    DocumentDiff {
        from_revision: old.revision,
        to_revision: new.revision,
        author: handoff_changed.then(|| new.handoff.updated_by.clone()).flatten(),
        changes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops::Op;
    use serde_json::json;

    fn apply(doc: &mut EngramDocument, v: Value) {
        serde_json::from_value::<Op>(v).unwrap().apply(doc).unwrap();
    }

    #[test]
    fn reports_every_kind_of_change() {
        let mut old = EngramDocument::new("P");
        apply(
            &mut old,
            json!({"op":"upsert_memory","key":"db","category":"tech_stack","content":"MySQL"}),
        );
        apply(
            &mut old,
            json!({"op":"upsert_memory","key":"gone","category":"note","content":"Bye"}),
        );
        apply(
            &mut old,
            json!({"op":"add_architecture_node","id":"api","type":"service"}),
        );
        apply(&mut old, json!({"op":"upsert_prompt","name":"p","body":"x"}));
        let mut new = old.clone();
        new.revision = old.revision + 3;
        apply(
            &mut new,
            json!({"op":"upsert_memory","key":"db2","category":"tech_stack","content":"Postgres","supersedes_key":"db"}),
        );
        apply(&mut new, json!({"op":"delete_memory","key":"gone"}));
        apply(
            &mut new,
            json!({"op":"add_architecture_node","id":"api","label":"Public API"}),
        );
        apply(
            &mut new,
            json!({"op":"set_node_position","id":"api","position":{"x":1,"y":2}}),
        );
        apply(
            &mut new,
            json!({"op":"log_decision","context":"c","decision":"Use Postgres"}),
        );
        apply(
            &mut new,
            json!({"op":"update_handoff","objective":"Migrate","next_step":"n","updated_by":"claude-code"}),
        );
        apply(&mut new, json!({"op":"update_project","stale_handoff_days":3}));

        let d = diff(&old, &new);
        let find = |e: EntityKind, id: &str| d.changes.iter().find(|c| c.entity == e && c.id == id).cloned();
        assert_eq!(find(EntityKind::Memory, "db2").unwrap().kind, ChangeKind::Added);
        assert_eq!(find(EntityKind::Memory, "db").unwrap().kind, ChangeKind::Superseded);
        assert_eq!(find(EntityKind::Memory, "gone").unwrap().kind, ChangeKind::Deleted);
        assert_eq!(find(EntityKind::Node, "api").unwrap().fields, vec!["label"]);
        assert_eq!(find(EntityKind::Decision, "ADR-0001").unwrap().title, "Use Postgres");
        assert_eq!(find(EntityKind::Handoff, "handoff").unwrap().kind, ChangeKind::Added);
        assert_eq!(find(EntityKind::Project, "project").unwrap().fields, vec!["settings"]);
        assert!(find(EntityKind::Prompt, "p").is_none());
        assert_eq!(d.author.as_deref(), Some("claude-code"));
        assert_eq!(d.summary(), "3 added, 2 updated, 1 superseded, 1 deleted");
        assert!(diff(&new, &new).is_empty());
    }

    #[test]
    fn layout_only_changes_are_quiet() {
        let mut old = EngramDocument::new("P");
        apply(&mut old, json!({"op":"add_architecture_node","id":"a","type":"module"}));
        let mut new = old.clone();
        apply(
            &mut new,
            json!({"op":"set_node_position","id":"a","position":{"x":5,"y":5}}),
        );
        assert!(diff(&old, &new).is_empty());
    }
}
