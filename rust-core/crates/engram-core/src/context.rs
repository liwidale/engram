//! Context Builder: ranks project memory, ADRs and architecture nodes against a task and packs
//! the highest-signal items into a token budget, emitted as an XML context bundle.
//!
//! Ranking = BM25 over item text, plus graph propagation (a matched node lifts its neighbours
//! and every entry that mentions it), scaled by per-kind weights. Pinned and explicitly
//! selected items are always included. Superseded memory and deprecated ADRs never are.

use crate::analysis::{stem, terms};
use crate::graph::Graph;
use crate::schema::*;
use crate::tokens::{count_tokens, TOKENIZER};
use crate::time::now_rfc3339;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;

pub const DEFAULT_MAX_TOKENS: usize = 4000;
const DEFAULT_MIN_SCORE: f64 = 0.12;

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Selection {
    #[serde(default)]
    pub memory: Vec<String>,
    #[serde(default)]
    pub decisions: Vec<String>,
    #[serde(default)]
    pub nodes: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ContextRequest {
    #[serde(default, alias = "task_description")]
    pub task: String,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: usize,
    /// Always included (subject to never including superseded/deprecated items).
    #[serde(default)]
    pub include: Selection,
    /// Never included.
    #[serde(default)]
    pub exclude: Selection,
    #[serde(default)]
    pub prompt: Option<String>,
    #[serde(default)]
    pub prompt_vars: serde_json::Map<String, serde_json::Value>,
    #[serde(default = "yes")]
    pub include_handoff: bool,
    /// When false only pinned and explicitly included items are used.
    #[serde(default = "yes")]
    pub auto_rank: bool,
    #[serde(default)]
    pub min_score: Option<f64>,
}

impl ContextRequest {
    pub fn for_task(task: impl Into<String>, max_tokens: usize) -> Self {
        Self {
            task: task.into(),
            max_tokens,
            include: Selection::default(),
            exclude: Selection::default(),
            prompt: None,
            prompt_vars: Default::default(),
            include_handoff: true,
            auto_rank: true,
            min_score: None,
        }
    }
}

fn default_max_tokens() -> usize {
    DEFAULT_MAX_TOKENS
}
fn yes() -> bool {
    true
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemKind {
    Memory,
    Decision,
    Node,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Reason {
    Pinned,
    Selected,
    Ranked,
    OverBudget,
    BelowThreshold,
    Excluded,
}

#[derive(Clone, Debug, Serialize)]
pub struct RankedItem {
    pub kind: ItemKind,
    pub id: String,
    pub title: String,
    pub score: f64,
    pub tokens: usize,
    pub included: bool,
    pub reason: Reason,
    /// Query terms that matched this item.
    pub matched: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct PromptUsage {
    pub name: String,
    pub missing: Vec<String>,
    pub found: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct ContextBundle {
    pub xml: String,
    pub tokens: usize,
    pub max_tokens: usize,
    pub over_budget: bool,
    pub tokenizer: &'static str,
    pub items: Vec<RankedItem>,
    pub dead_ends: usize,
    pub prompt: Option<PromptUsage>,
}

struct Candidate<'a> {
    kind: ItemKind,
    id: &'a str,
    title: String,
    weight: f64,
    terms: Vec<String>,
    mentions: Vec<&'a str>,
    fragment: String,
    forced: Option<Reason>,
    excluded: bool,
}

pub fn build_context(doc: &EngramDocument, req: &ContextRequest) -> ContextBundle {
    let graph = Graph::new(&doc.architecture);
    let node_ids: HashSet<&str> = doc.architecture.nodes.iter().map(|n| n.id.as_str()).collect();
    let has = |list: &[String], id: &str| list.iter().any(|x| x.eq_ignore_ascii_case(id));
    let mut cands: Vec<Candidate> = Vec::new();

    for m in doc.memory.iter().filter(|m| m.is_active()) {
        let text = format!("{} {} {} {}", m.key, m.category.as_str(), m.tags.join(" "), m.body());
        let t = terms(&text);
        cands.push(Candidate {
            kind: ItemKind::Memory,
            id: &m.key,
            title: first_line(m.body(), &m.key),
            weight: m.category.weight(),
            mentions: mentions(&t, &node_ids),
            terms: t,
            fragment: memory_xml(m),
            forced: if has(&req.include.memory, &m.key) {
                Some(Reason::Selected)
            } else if m.pinned {
                Some(Reason::Pinned)
            } else {
                None
            },
            excluded: has(&req.exclude.memory, &m.key),
        });
    }
    for d in doc.decisions.iter().filter(|d| d.is_active()) {
        let alts: Vec<String> = d.rejected_alternatives.iter().map(|a| format!("{} {}", a.option, a.reason)).collect();
        let text = format!("{} {} {} {} {} {}", d.id, d.title, d.context, d.decision, alts.join(" "), d.related.join(" "));
        let t = terms(&text);
        let mut ms = mentions(&t, &node_ids);
        ms.extend(d.related.iter().map(String::as_str).filter(|r| node_ids.contains(r)));
        cands.push(Candidate {
            kind: ItemKind::Decision,
            id: &d.id,
            title: d.title.clone(),
            weight: 1.25,
            mentions: ms,
            terms: t,
            fragment: decision_xml(d),
            forced: has(&req.include.decisions, &d.id).then_some(Reason::Selected),
            excluded: has(&req.exclude.decisions, &d.id),
        });
    }
    for n in &doc.architecture.nodes {
        // The node type is deliberately not indexed: "service" in a task would otherwise match
        // every service node and drag unrelated neighbours in through propagation.
        let text = format!("{} {} {} {}", n.id, n.display_name(), n.description, n.path.as_deref().unwrap_or(""));
        cands.push(Candidate {
            kind: ItemKind::Node,
            id: &n.id,
            title: n.display_name().to_owned(),
            weight: 0.8,
            mentions: vec![],
            terms: terms(&text),
            fragment: node_xml(n, &graph),
            forced: has(&req.include.nodes, &n.id).then_some(Reason::Selected),
            excluded: has(&req.exclude.nodes, &n.id),
        });
    }

    // --- Lexical relevance (BM25) ---------------------------------------------------------
    let query: Vec<String> = {
        let mut seen = HashSet::new();
        let mut text = req.task.clone();
        for v in req.prompt_vars.values() {
            if let Some(s) = v.as_str() {
                text.push(' ');
                text.push_str(s);
            }
        }
        terms(&text).into_iter().filter(|t| seen.insert(t.clone())).collect()
    };
    let n_docs = cands.len().max(1) as f64;
    let avg_len = cands.iter().map(|c| c.terms.len()).sum::<usize>() as f64 / n_docs;
    let df: HashMap<&str, usize> = query
        .iter()
        .map(|q| (q.as_str(), cands.iter().filter(|c| c.terms.iter().any(|t| t == q)).count()))
        .collect();
    let (k1, b) = (1.2, 0.75);
    let mut lexical: Vec<f64> = Vec::with_capacity(cands.len());
    let mut matched: Vec<Vec<String>> = Vec::with_capacity(cands.len());
    for c in &cands {
        let len = c.terms.len() as f64;
        let mut score = 0.0;
        let mut hits = Vec::new();
        for q in &query {
            let tf = c.terms.iter().filter(|t| *t == q).count() as f64;
            if tf == 0.0 {
                continue;
            }
            let dfq = df[q.as_str()] as f64;
            let idf = (1.0 + (n_docs - dfq + 0.5) / (dfq + 0.5)).ln();
            score += idf * tf * (k1 + 1.0) / (tf + k1 * (1.0 - b + b * len / avg_len.max(1.0)));
            hits.push(q.clone());
        }
        lexical.push(score);
        matched.push(hits);
    }
    let max_lex = lexical.iter().copied().fold(0.0, f64::max);
    let norm: Vec<f64> = lexical.iter().map(|s| if max_lex > 0.0 { s / max_lex } else { 0.0 }).collect();

    // --- Graph propagation ----------------------------------------------------------------
    // Only nodes with a meaningful direct match propagate relevance.
    let node_score: HashMap<&str, f64> = cands
        .iter()
        .zip(&norm)
        .filter(|(c, s)| c.kind == ItemKind::Node && **s >= 0.25)
        .map(|(c, s)| (c.id, *s))
        .collect();
    let mut scores: Vec<f64> = Vec::with_capacity(cands.len());
    for (i, c) in cands.iter().enumerate() {
        let propagated = match c.kind {
            ItemKind::Node => {
                let mut best: f64 = 0.0;
                for nb in graph.dependencies(c.id).iter().chain(&graph.dependents(c.id)) {
                    best = best.max(node_score.get(nb.as_str()).copied().unwrap_or(0.0));
                }
                0.5 * best
            }
            _ => 0.4 * c.mentions.iter().filter_map(|m| node_score.get(m)).copied().fold(0.0, f64::max),
        };
        let base = if query.is_empty() { 0.25 } else { norm[i] + propagated };
        scores.push(base * c.weight);
    }

    // --- Budget packing -------------------------------------------------------------------
    let handoff_xml = (req.include_handoff && !doc.handoff.is_empty()).then(|| handoff_xml(&doc.handoff));
    let prompt = req.prompt.as_deref().filter(|p| !p.trim().is_empty()).map(|name| match doc.prompt(name) {
        Some(p) => {
            let r = p.render(&req.prompt_vars);
            (PromptUsage { name: p.name.clone(), missing: r.missing, found: true }, Some(prompt_xml(&p.name, &r.text)))
        }
        None => (PromptUsage { name: name.to_owned(), missing: vec![], found: false }, None),
    });
    let prompt_fragment = prompt.as_ref().and_then(|(_, x)| x.clone());
    let skeleton = assemble(doc, req, handoff_xml.as_deref(), &[], &[], &[], &[], prompt_fragment.as_deref());
    let mut used = count_tokens(&skeleton) + 12; // section wrappers
    let threshold = req.min_score.unwrap_or(DEFAULT_MIN_SCORE);

    let token_cost: Vec<usize> = cands.iter().map(|c| count_tokens(&c.fragment) + 1).collect();
    let mut order: Vec<usize> = (0..cands.len()).collect();
    order.sort_by(|&a, &b| {
        let forced = |i: usize| cands[i].forced.is_some() && !cands[i].excluded;
        forced(b).cmp(&forced(a)).then(scores[b].total_cmp(&scores[a])).then(a.cmp(&b))
    });
    let mut reason: Vec<Reason> = vec![Reason::BelowThreshold; cands.len()];
    let mut included = vec![false; cands.len()];
    for &i in &order {
        let c = &cands[i];
        reason[i] = if c.excluded {
            Reason::Excluded
        } else if let Some(r) = c.forced {
            used += token_cost[i];
            included[i] = true;
            r
        } else if !req.auto_rank || scores[i] < threshold {
            Reason::BelowThreshold
        } else if used + token_cost[i] > req.max_tokens {
            Reason::OverBudget
        } else {
            used += token_cost[i];
            included[i] = true;
            Reason::Ranked
        };
    }

    let pick = |kind: ItemKind| -> Vec<&str> {
        let mut ix: Vec<usize> = (0..cands.len()).filter(|&i| included[i] && cands[i].kind == kind).collect();
        if kind == ItemKind::Memory {
            ix.sort_by_key(|&i| (category_rank(doc.memory_entry(cands[i].id).map(|m| m.category)), i));
        }
        ix.into_iter().map(|i| cands[i].fragment.as_str()).collect()
    };
    let (mem, dec, nodes) = (pick(ItemKind::Memory), pick(ItemKind::Decision), pick(ItemKind::Node));

    // Dead ends: rejected alternatives of active ADRs that did not make it in full.
    let included_adrs: HashSet<&str> =
        (0..cands.len()).filter(|&i| included[i] && cands[i].kind == ItemKind::Decision).map(|i| cands[i].id).collect();
    let mut dead_ends: Vec<String> = Vec::new();
    if req.auto_rank {
        for d in doc.decisions.iter().filter(|d| d.is_active() && !included_adrs.contains(d.id.as_str())) {
            if has(&req.exclude.decisions, &d.id) {
                continue;
            }
            for a in &d.rejected_alternatives {
                let line = format!(
                    "    <rejected adr=\"{}\" option=\"{}\">{}</rejected>\n",
                    attr(&d.id),
                    attr(&a.option),
                    esc(&a.reason)
                );
                let cost = count_tokens(&line);
                if used + cost <= req.max_tokens {
                    used += cost;
                    dead_ends.push(line);
                }
            }
        }
    }

    let xml = assemble(doc, req, handoff_xml.as_deref(), &mem, &nodes, &dec, &dead_ends, prompt_fragment.as_deref());
    let tokens = count_tokens(&xml);
    let mut items: Vec<RankedItem> = order
        .iter()
        .map(|&i| RankedItem {
            kind: cands[i].kind,
            id: cands[i].id.to_owned(),
            title: cands[i].title.clone(),
            score: (scores[i] * 1000.0).round() / 1000.0,
            tokens: token_cost[i],
            included: included[i],
            reason: reason[i],
            matched: std::mem::take(&mut matched[i]),
        })
        .collect();
    items.sort_by(|a, b| b.included.cmp(&a.included).then(b.score.total_cmp(&a.score)));
    ContextBundle {
        tokens,
        over_budget: tokens > req.max_tokens,
        max_tokens: req.max_tokens,
        tokenizer: TOKENIZER,
        dead_ends: dead_ends.len(),
        items,
        prompt: prompt.map(|(u, _)| u),
        xml,
    }
}

fn mentions<'a>(terms: &[String], node_ids: &HashSet<&'a str>) -> Vec<&'a str> {
    let mut out: Vec<&'a str> = Vec::new();
    for t in terms {
        if let Some(id) = node_ids.iter().find(|id| id.eq_ignore_ascii_case(t) || stem(&id.to_lowercase()) == *t) {
            if !out.contains(id) {
                out.push(id);
            }
        }
    }
    out
}

fn category_rank(c: Option<MemoryCategory>) -> usize {
    match c {
        Some(MemoryCategory::Invariant) => 0,
        Some(MemoryCategory::Rule) => 1,
        Some(MemoryCategory::TechStack) => 2,
        Some(MemoryCategory::Convention) => 3,
        _ => 4,
    }
}

fn first_line(s: &str, fallback: &str) -> String {
    let line = s.lines().map(|l| l.trim().trim_start_matches(['#', '-', '*', ' '])).find(|l| !l.is_empty()).unwrap_or(fallback);
    let mut out: String = line.chars().take(90).collect();
    if line.chars().count() > 90 {
        out.push('…');
    }
    out
}

#[allow(clippy::too_many_arguments)]
fn assemble(
    doc: &EngramDocument,
    req: &ContextRequest,
    handoff: Option<&str>,
    memory: &[&str],
    nodes: &[&str],
    decisions: &[&str],
    dead_ends: &[String],
    prompt: Option<&str>,
) -> String {
    let mut x = String::with_capacity(4096);
    let _ = writeln!(
        x,
        "<engram_context project=\"{}\" revision=\"{}\" generated_at=\"{}\">",
        attr(&doc.project.name),
        doc.revision,
        now_rfc3339()
    );
    if !req.task.trim().is_empty() {
        let _ = writeln!(x, "  <task>{}</task>", esc(req.task.trim()));
    }
    if let Some(h) = handoff {
        x.push_str(h);
    }
    let section = |x: &mut String, tag: &str, items: &[&str]| {
        if !items.is_empty() {
            let _ = writeln!(x, "  <{tag}>");
            items.iter().for_each(|i| x.push_str(i));
            let _ = writeln!(x, "  </{tag}>");
        }
    };
    section(&mut x, "project_memory", memory);
    section(&mut x, "architecture", nodes);
    section(&mut x, "decisions", decisions);
    if !dead_ends.is_empty() {
        x.push_str("  <dead_ends note=\"Rejected approaches. Do not propose these again.\">\n");
        dead_ends.iter().for_each(|d| x.push_str(d));
        x.push_str("  </dead_ends>\n");
    }
    if let Some(p) = prompt {
        x.push_str(p);
    }
    x.push_str("</engram_context>\n");
    x
}

fn memory_xml(m: &MemoryEntry) -> String {
    let mut x = format!("    <entry key=\"{}\" category=\"{}\"", attr(&m.key), m.category.as_str());
    if m.pinned {
        x.push_str(" pinned=\"true\"");
    }
    if !m.files.is_empty() {
        let _ = write!(x, " files=\"{}\"", attr(&m.files.join(" ")));
    }
    let _ = writeln!(x, ">{}</entry>", esc(m.body().trim()));
    x
}

fn decision_xml(d: &Decision) -> String {
    let mut x = format!("    <adr id=\"{}\" status=\"{}\" title=\"{}\">\n", attr(&d.id), d.status.as_str(), attr(&d.title));
    let _ = writeln!(x, "      <context>{}</context>", esc(d.context.trim()));
    let _ = writeln!(x, "      <decision>{}</decision>", esc(d.decision.trim()));
    for a in &d.rejected_alternatives {
        let _ = writeln!(x, "      <rejected option=\"{}\">{}</rejected>", attr(&a.option), esc(&a.reason));
    }
    if !d.consequences.trim().is_empty() {
        let _ = writeln!(x, "      <consequences>{}</consequences>", esc(d.consequences.trim()));
    }
    x.push_str("    </adr>\n");
    x
}

fn node_xml(n: &ArchNode, g: &Graph) -> String {
    let mut x = format!("    <node id=\"{}\" type=\"{}\"", attr(&n.id), n.kind.as_str());
    if n.label.as_deref().is_some_and(|l| l != n.id) {
        let _ = write!(x, " label=\"{}\"", attr(n.display_name()));
    }
    if let Some(p) = &n.path {
        let _ = write!(x, " path=\"{}\"", attr(p));
    }
    if !n.depends_on.is_empty() {
        let _ = write!(x, " depends_on=\"{}\"", attr(&n.depends_on.join(" ")));
    }
    let dependents = g.dependents(&n.id);
    if !dependents.is_empty() {
        let _ = write!(x, " used_by=\"{}\"", attr(&dependents.join(" ")));
    }
    if n.description.trim().is_empty() {
        x.push_str("/>\n");
    } else {
        let _ = writeln!(x, ">{}</node>", esc(n.description.trim()));
    }
    x
}

fn handoff_xml(h: &Handoff) -> String {
    let mut x = String::from("  <handoff");
    if let Some(at) = &h.updated_at {
        let _ = write!(x, " updated_at=\"{}\"", attr(at));
    }
    if let Some(by) = &h.updated_by {
        let _ = write!(x, " updated_by=\"{}\"", attr(by));
    }
    x.push_str(">\n");
    let _ = writeln!(x, "    <objective>{}</objective>", esc(&h.objective));
    if !h.progress.is_empty() {
        x.push_str("    <verified_progress>\n");
        for p in &h.progress {
            let _ = write!(x, "      <item verified=\"{}\"", p.verified);
            if !p.files.is_empty() {
                let _ = write!(x, " files=\"{}\"", attr(&p.files.join(" ")));
            }
            let _ = writeln!(x, ">{}</item>", esc(&p.description));
        }
        x.push_str("    </verified_progress>\n");
    }
    let wm = &h.working_memory;
    if !(wm.hypotheses.is_empty() && wm.blockers.is_empty() && wm.notes.is_empty()) {
        x.push_str("    <working_memory>\n");
        wm.hypotheses.iter().for_each(|s| {
            let _ = writeln!(x, "      <hypothesis>{}</hypothesis>", esc(s));
        });
        wm.blockers.iter().for_each(|s| {
            let _ = writeln!(x, "      <blocker>{}</blocker>", esc(s));
        });
        if !wm.notes.trim().is_empty() {
            let _ = writeln!(x, "      <notes>{}</notes>", esc(wm.notes.trim()));
        }
        x.push_str("    </working_memory>\n");
    }
    let _ = writeln!(x, "    <next_step>{}</next_step>", esc(&h.next_step));
    x.push_str("  </handoff>\n");
    x
}

fn prompt_xml(name: &str, text: &str) -> String {
    format!("  <prompt name=\"{}\">{}</prompt>\n", attr(name), esc(text.trim()))
}

/// Standalone handoff checkpoint (also used by the `engram://handoff` resource).
pub fn handoff_document_xml(doc: &EngramDocument) -> String {
    format!(
        "<engram_handoff project=\"{}\" revision=\"{}\">\n{}</engram_handoff>\n",
        attr(&doc.project.name),
        doc.revision,
        handoff_xml(&doc.handoff)
    )
}

pub fn esc(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            _ => o.push(c),
        }
    }
    o
}

pub fn attr(s: &str) -> String {
    esc(s).replace('"', "&quot;").replace('\n', "&#10;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ops::Op;
    use serde_json::json;

    fn fixture() -> EngramDocument {
        let mut d = EngramDocument::new("Shop");
        let ops = [
            json!({"op":"upsert_memory","key":"ts-strict","category":"rule","content":"TypeScript strict mode everywhere.","pinned":true}),
            json!({"op":"upsert_memory","key":"payments-idempotency","category":"invariant","content":"Every payment mutation carries an idempotency key; retries must never double-charge."}),
            json!({"op":"upsert_memory","key":"css","category":"convention","content":"Tailwind utility classes only, no CSS modules."}),
            json!({"op":"upsert_memory","key":"db-old","category":"tech_stack","content":"MySQL 5.7 for payments data."}),
            json!({"op":"upsert_memory","key":"db","category":"tech_stack","content":"Postgres 16 for payments data.","supersedes_key":"db-old"}),
            json!({"op":"add_architecture_node","id":"payments","type":"service","dependencies":["ledger-db"],"description":"Charges cards via Stripe."}),
            json!({"op":"add_architecture_node","id":"ledger-db","type":"database"}),
            json!({"op":"add_architecture_node","id":"storefront","type":"ui","dependencies":["payments"]}),
            json!({"op":"add_architecture_node","id":"search","type":"service"}),
            json!({"op":"log_decision","title":"Use Stripe for payments","context":"Need card payments.","decision":"Integrate Stripe Checkout in the payments service.","rejected_alternatives":[{"option":"Braintree","reason":"weaker SCA support"}],"related":["payments"]}),
            json!({"op":"log_decision","title":"Search engine","context":"Product search.","decision":"Use Meilisearch.","rejected_alternatives":[{"option":"Elasticsearch","reason":"ops burden"}]}),
            json!({"op":"upsert_prompt","name":"impl","body":"Implement {{feature}} following the context."}),
            json!({"op":"update_handoff","objective":"Add refunds","next_step":"Write refund handler","progress":["Mapped Stripe refund API"]}),
        ];
        for o in ops {
            serde_json::from_value::<Op>(o).unwrap().apply(&mut d).unwrap();
        }
        d
    }

    fn included(b: &ContextBundle) -> Vec<&str> {
        b.items.iter().filter(|i| i.included).map(|i| i.id.as_str()).collect()
    }

    #[test]
    fn ranks_relevant_items_and_excludes_superseded() {
        let doc = fixture();
        let b = build_context(&doc, &ContextRequest::for_task("Add refunds to the payments service without double charging", 4000));
        let inc = included(&b);
        assert!(inc.contains(&"payments-idempotency"), "{inc:?}");
        assert!(inc.contains(&"payments"));
        assert!(inc.contains(&"ADR-0001"));
        assert!(inc.contains(&"ts-strict"), "pinned always included");
        assert!(!inc.contains(&"db-old"), "superseded never included");
        assert!(!b.xml.contains("MySQL"));
        assert!(inc.contains(&"ledger-db"), "graph neighbour of a matched node");
        assert!(!inc.contains(&"css"), "irrelevant convention dropped: {inc:?}");
        assert!(b.xml.contains("<rejected option=\"Braintree\">weaker SCA support</rejected>"));
        assert!(b.xml.contains("<dead_ends"), "rejected Elasticsearch still surfaced as a dead end");
        assert!(b.xml.contains("<handoff"));
        assert!(!b.over_budget);
        assert_eq!(b.tokens, count_tokens(&b.xml));
    }

    #[test]
    fn respects_budget_but_keeps_forced_items() {
        let doc = fixture();
        let mut req = ContextRequest::for_task("payments refunds stripe ledger", 120);
        req.include_handoff = false;
        let b = build_context(&doc, &req);
        assert!(included(&b).contains(&"ts-strict"));
        assert!(b.items.iter().any(|i| i.reason == Reason::OverBudget));
        assert!(b.tokens <= 120 + 40, "close to budget: {}", b.tokens);

        req.max_tokens = 10;
        req.include.memory = vec!["css".into()];
        let b = build_context(&doc, &req);
        assert!(included(&b).contains(&"css"));
        assert!(b.over_budget);
    }

    #[test]
    fn manual_mode_prompt_rendering_and_exclusions() {
        let doc = fixture();
        let mut req = ContextRequest::for_task("payments", 4000);
        req.auto_rank = false;
        req.include.decisions = vec!["adr-0002".into()];
        req.exclude.memory = vec!["ts-strict".into()];
        req.prompt = Some("impl".into());
        req.prompt_vars = json!({"feature": "refunds & <voids>"}).as_object().unwrap().clone();
        let b = build_context(&doc, &req);
        assert_eq!(included(&b), vec!["ADR-0002"]);
        assert!(b.xml.contains("<prompt name=\"impl\">Implement refunds &amp; &lt;voids&gt; following the context.</prompt>"));
        assert_eq!(b.prompt.as_ref().unwrap().missing.len(), 0);
        assert!(!b.xml.contains("dead_ends"));
    }

    #[test]
    fn empty_task_falls_back_to_priority_order() {
        let doc = fixture();
        let b = build_context(&doc, &ContextRequest::for_task("", 4000));
        assert!(included(&b).contains(&"payments-idempotency"));
        assert!(!b.xml.contains("<task>"));
    }
}
