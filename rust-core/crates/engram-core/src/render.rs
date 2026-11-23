use crate::context::handoff_document_xml;
use crate::graph::Graph;
use crate::schema::*;
use serde_json::{json, Value};
use std::fmt::Write as _;

fn category_title(c: MemoryCategory) -> &'static str {
    match c {
        MemoryCategory::Invariant => "Architecture Invariants",
        MemoryCategory::Rule => "Rules",
        MemoryCategory::TechStack => "Tech Stack",
        MemoryCategory::Convention => "Conventions",
        MemoryCategory::Note => "Notes",
    }
}

const CATEGORY_ORDER: [MemoryCategory; 5] = [
    MemoryCategory::Invariant,
    MemoryCategory::Rule,
    MemoryCategory::TechStack,
    MemoryCategory::Convention,
    MemoryCategory::Note,
];

pub fn memory_entry_markdown(m: &MemoryEntry) -> String {
    let mut x = String::new();
    if m.is_active() {
        let _ = write!(x, "### {}", m.key);
        if m.pinned {
            x.push_str(" (pinned)");
        }
    } else {
        let _ = write!(x, "### {SUPERSEDED_TAG} {}", m.key);
        if let Some(by) = &m.superseded_by {
            let _ = write!(x, " → replaced by `{by}`");
        }
    }
    let _ = write!(x, "\n_{}_", m.category.as_str());
    if !m.tags.is_empty() {
        let _ = write!(x, " · tags: {}", m.tags.join(", "));
    }
    if !m.files.is_empty() {
        let _ = write!(
            x,
            " · files: {}",
            m.files.iter().map(|f| format!("`{f}`")).collect::<Vec<_>>().join(", ")
        );
    }
    let _ = write!(x, "\n\n{}\n", m.body().trim());
    x
}

pub fn memory_markdown(doc: &EngramDocument) -> String {
    let mut x = format!("# Project Memory: {}\n\n", doc.project.name);
    if !doc.project.description.trim().is_empty() {
        let _ = writeln!(x, "{}\n", doc.project.description.trim());
    }
    x.push_str(
        "Canonical project truth. Entries marked [SUPERSEDED] are kept for history only and must not be followed.\n",
    );
    for cat in CATEGORY_ORDER {
        let entries: Vec<&MemoryEntry> = doc
            .memory
            .iter()
            .filter(|m| m.is_active() && m.category == cat)
            .collect();
        if entries.is_empty() {
            continue;
        }
        let _ = writeln!(x, "\n## {}\n", category_title(cat));
        for m in entries {
            x.push_str(&memory_entry_markdown(m));
            x.push('\n');
        }
    }
    let superseded: Vec<&MemoryEntry> = doc.memory.iter().filter(|m| !m.is_active()).collect();
    if !superseded.is_empty() {
        x.push_str("\n## Superseded (history only, do not follow)\n\n");
        for m in superseded {
            x.push_str(&memory_entry_markdown(m));
            x.push('\n');
        }
    }
    if doc.memory.is_empty() {
        x.push_str("\n_No memory entries yet._\n");
    }
    x
}

pub fn decision_markdown(d: &Decision) -> String {
    let status = match d.status {
        DecisionStatus::Active => "Active".to_owned(),
        DecisionStatus::Deprecated => match &d.superseded_by {
            Some(by) => format!("Deprecated, superseded by {by}"),
            None => "Deprecated".to_owned(),
        },
    };
    let mut x = format!(
        "## {}: {}\n_Status: {status} · recorded {}_\n\n",
        d.id, d.title, d.created_at
    );
    let _ = writeln!(x, "**Context**\n\n{}\n", d.context.trim());
    let _ = writeln!(x, "**Decision**\n\n{}\n", d.decision.trim());
    if !d.rejected_alternatives.is_empty() {
        x.push_str("**Rejected alternatives (do not re-propose)**\n\n");
        for a in &d.rejected_alternatives {
            if a.reason.is_empty() {
                let _ = writeln!(x, "- **{}**", a.option);
            } else {
                let _ = writeln!(x, "- **{}**: {}", a.option, a.reason);
            }
        }
        x.push('\n');
    }
    if !d.consequences.trim().is_empty() {
        let _ = writeln!(x, "**Consequences**\n\n{}\n", d.consequences.trim());
    }
    if !d.related.is_empty() {
        let _ = writeln!(x, "Related: {}\n", d.related.join(", "));
    }
    x
}

pub fn decisions_markdown(doc: &EngramDocument) -> String {
    let mut x = format!("# Decision Log: {}\n\n", doc.project.name);
    if doc.decisions.is_empty() {
        x.push_str("_No decisions recorded yet._\n");
        return x;
    }
    let active = doc.decisions.iter().filter(|d| d.is_active()).count();
    let _ = writeln!(x, "{active} active, {} deprecated.\n", doc.decisions.len() - active);
    for d in doc
        .decisions
        .iter()
        .filter(|d| d.is_active())
        .chain(doc.decisions.iter().filter(|d| !d.is_active()))
    {
        x.push_str(&decision_markdown(d));
    }
    x
}

pub fn handoff_markdown(doc: &EngramDocument) -> String {
    let h = &doc.handoff;
    let mut x = format!("# Agent Handoff: {}\n\n", doc.project.name);
    if h.is_empty() {
        x.push_str("_No handoff recorded yet. Call update_handoff before ending a session._\n");
        return x;
    }
    if let Some(at) = &h.updated_at {
        let _ = write!(x, "_Updated {at}");
        if let Some(by) = &h.updated_by {
            let _ = write!(x, " by {by}");
        }
        x.push_str("_\n\n");
    }
    let _ = writeln!(x, "## Active Objective\n\n{}\n", h.objective);
    x.push_str("## Verified Progress\n\n");
    if h.progress.is_empty() {
        x.push_str("_None yet._\n");
    }
    for p in &h.progress {
        let _ = write!(x, "- [{}] {}", if p.verified { "x" } else { " " }, p.description);
        if !p.files.is_empty() {
            let _ = write!(
                x,
                " ({})",
                p.files.iter().map(|f| format!("`{f}`")).collect::<Vec<_>>().join(", ")
            );
        }
        x.push('\n');
    }
    let wm = &h.working_memory;
    x.push_str("\n## Working Memory\n\n");
    for s in &wm.hypotheses {
        let _ = writeln!(x, "- Hypothesis: {s}");
    }
    for s in &wm.blockers {
        let _ = writeln!(x, "- Blocker: {s}");
    }
    if !wm.notes.trim().is_empty() {
        let _ = writeln!(x, "\n{}", wm.notes.trim());
    }
    if wm.hypotheses.is_empty() && wm.blockers.is_empty() && wm.notes.trim().is_empty() {
        x.push_str("_Empty._\n");
    }
    let _ = writeln!(x, "\n## Next Atomic Step\n\n{}", h.next_step);
    x
}

pub fn handoff_xml(doc: &EngramDocument) -> String {
    handoff_document_xml(doc)
}

pub fn architecture_json(doc: &EngramDocument) -> Value {
    let g = Graph::new(&doc.architecture);
    let nodes: Vec<Value> = doc
        .architecture
        .nodes
        .iter()
        .map(|n| {
            json!({
                "id": n.id,
                "type": n.kind,
                "label": n.display_name(),
                "description": n.description.as_str(),
                "path": n.path,
                "depends_on": n.depends_on,
                "used_by": g.dependents(&n.id),
            })
        })
        .collect();
    let edges: Vec<Value> = doc
        .architecture
        .nodes
        .iter()
        .flat_map(|n| n.depends_on.iter().map(move |d| json!({ "from": n.id, "to": d })))
        .collect();
    json!({
        "project": doc.project.name,
        "revision": doc.revision,
        "edge_semantics": "from depends on to",
        "nodes": nodes,
        "edges": edges,
        "build_order": g.topological_order(),
        "cycles": g.cycles(),
        "missing_dependencies": g.missing_edges(),
    })
}

pub fn prompts_json(doc: &EngramDocument) -> Value {
    let prompts: Vec<Value> = doc
        .prompts
        .iter()
        .map(|p| {
            json!({
                "name": p.name,
                "category": p.category,
                "description": p.description,
                "variables": p.variables(),
                "body": p.body.as_str(),
            })
        })
        .collect();
    json!({ "project": doc.project.name, "prompts": prompts })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops::Op;

    #[test]
    fn renders_superseded_entries_last_with_tag() {
        let mut d = EngramDocument::new("P");
        for v in [
            json!({"op":"upsert_memory","key":"a","category":"rule","content":"Old"}),
            json!({"op":"upsert_memory","key":"b","category":"rule","content":"New","supersedes_key":"a"}),
            json!({"op":"log_decision","context":"c","decision":"d","rejected_alternatives":["X: slow"]}),
        ] {
            serde_json::from_value::<Op>(v).unwrap().apply(&mut d).unwrap();
        }
        let md = memory_markdown(&d);
        let (active, history) = md.split_once("## Superseded").unwrap();
        assert!(active.contains("### b") && !active.contains("### a"));
        assert!(history.contains("### [SUPERSEDED] a → replaced by `b`"));
        assert!(decisions_markdown(&d).contains("- **X**: slow"));
        assert!(handoff_markdown(&d).contains("No handoff recorded yet"));
        assert_eq!(architecture_json(&d)["build_order"], json!([]));
    }
}
