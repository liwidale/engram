use crate::i18n::{tr, trf};
use crate::theme;
use crate::widgets::{self, chip, muted};
use crate::workspace::{Dialog, Module, ToastKind, Workspace};
use eframe::egui::{self, RichText, Ui};
use engram_core::health::{HealthReport, Severity};
use serde_json::json;

fn severity_title(s: Severity) -> &'static str {
    match s {
        Severity::Error => tr("Error"),
        Severity::Warning => tr("Warning"),
        Severity::Info => tr("Info"),
    }
}

pub fn kind_title(kind: &str) -> &'static str {
    match kind {
        "missing_repo" => tr("Missing repository"),
        "broken_path" => tr("Broken path"),
        "zombie_reference" => tr("Zombie reference"),
        "deprecated_reference" => tr("Deprecated reference"),
        "deprecated_decision" => tr("Deprecated decision"),
        "contradiction" => tr("Contradiction"),
        "rejected_alternative_endorsed" => tr("Rejected alternative endorsed"),
        "superseded_tag_mismatch" => tr("Superseded tag mismatch"),
        "dependency_cycle" => tr("Dependency cycle"),
        "stale_handoff" => tr("Stale handoff"),
        "empty_content" => tr("Empty content"),
        _ => tr("Issue"),
    }
}

fn kind_name(kind: &impl serde::Serialize) -> String {
    serde_json::to_value(kind)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default()
}

pub fn show(ui: &mut Ui, ws: &mut Workspace) {
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        muted(ui, tr("Codebase"));
        ui.label(
            RichText::new(ws.resolved_repo_root().display().to_string())
                .monospace()
                .small(),
        );
        if ui.link(tr("Change in Project settings")).clicked() {
            ws.dialog = Some(Dialog::Project(Box::new(ws.project_draft())));
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button(tr("Check again")).clicked() {
                ws.run_audit(ui.ctx());
            }
            if ws.auditing.is_some() {
                ui.spinner();
            }
        });
    });
    ui.separator();
    let Some(report) = ws.health.clone() else {
        ui.centered_and_justified(|ui| {
            ui.spinner();
        });
        return;
    };
    egui::ScrollArea::vertical()
        .auto_shrink(false)
        .show(ui, |ui| summary(ui, ws, &report));
}

fn summary(ui: &mut Ui, ws: &mut Workspace, r: &HealthReport) {
    let mut apply: Vec<serde_json::Value> = Vec::new();
    let mut ignore: Option<String> = None;
    widgets::card(ui, None, |ui| {
        ui.horizontal(|ui| {
            widgets::score_gauge(ui, r.score, 96.0);
            ui.vertical(|ui| {
                ui.label(
                    RichText::new(match r.grade {
                        "healthy" => tr("Healthy"),
                        "needs_attention" => tr("Needs attention"),
                        _ => tr("Critical"),
                    })
                    .size(18.0)
                    .strong()
                    .color(theme::score(r.score)),
                );
                muted(
                    ui,
                    trf(
                        "{paths} paths and {refs} references checked at revision {rev}",
                        &[
                            ("paths", &r.summary.paths_checked),
                            ("refs", &r.summary.references_checked),
                            ("rev", &r.revision),
                        ],
                    ),
                );
                ui.horizontal(|ui| {
                    for (s, label) in [
                        (Severity::Error, trf("{n} errors", &[("n", &r.summary.errors)])),
                        (Severity::Warning, trf("{n} warnings", &[("n", &r.summary.warnings)])),
                        (Severity::Info, trf("{n} info", &[("n", &r.summary.infos)])),
                    ] {
                        let active = ws.health_filter == Some(s);
                        if widgets::chip_button(ui, label, theme::severity(s), active)
                            .on_hover_text(tr("Show only this severity"))
                            .clicked()
                        {
                            ws.health_filter = if active { None } else { Some(s) };
                        }
                    }
                    if r.summary.ignored > 0 && ui.link(trf("{n} ignored", &[("n", &r.summary.ignored)])).clicked() {
                        ws.dialog = Some(Dialog::Project(Box::new(ws.project_draft())));
                    }
                });
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(tr("Copy report")).clicked() {
                    let mut md = format!(
                        "# Context health: {}\n\nScore {}/100 ({})\n\n",
                        ws.doc.project.name, r.score, r.grade
                    );
                    for i in &r.issues {
                        md.push_str(&format!("- {}: {}\n", severity_title(i.severity), i.message));
                    }
                    ui.ctx().copy_text(md);
                }
                let fixable: Vec<serde_json::Value> = r
                    .issues
                    .iter()
                    .filter_map(|i| i.fix.as_ref().map(|f| f.op.clone()))
                    .collect();
                if fixable.len() > 1
                    && ui
                        .add(widgets::primary_button(&trf("Fix all {n}", &[("n", &fixable.len())])))
                        .clicked()
                {
                    apply.push(json!({"op": "batch", "ops": fixable}));
                }
            });
        });
    });
    if !r.repo_found {
        ui.label(
            RichText::new(tr(
                "The linked codebase folder was not found, so file paths were not checked.",
            ))
            .color(theme::ERROR),
        );
    }
    let issues: Vec<_> = r
        .issues
        .iter()
        .filter(|i| ws.health_filter.is_none_or(|f| i.severity == f))
        .collect();
    if issues.is_empty() {
        ui.add_space(30.0);
        ui.vertical_centered(|ui| {
            let text = if r.issues.is_empty() {
                tr("No issues. Agents get clean, current context.")
            } else {
                tr("No issues at this severity.")
            };
            ui.label(RichText::new(text).color(theme::SUCCESS));
        });
    }
    for issue in issues {
        widgets::card(ui, None, |ui| {
            ui.horizontal(|ui| {
                chip(ui, severity_title(issue.severity), theme::severity(issue.severity));
                chip(ui, kind_title(&kind_name(&issue.kind)), theme::MUTED);
                muted(ui, format!("{} · {}", issue.subject.kind, issue.subject.id));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .link(tr("Ignore"))
                        .on_hover_text(tr("Hide this issue in future checks"))
                        .clicked()
                    {
                        ignore = Some(issue.id.clone());
                    }
                    if issue.subject.kind != "project" && ui.link(tr("Open")).clicked() {
                        reveal(ws, issue.subject.kind, &issue.subject.id);
                    }
                    if let Some(rel) = &issue.related {
                        if ui.link(trf("Open {id}", &[("id", &rel.id)])).clicked() {
                            reveal(ws, rel.kind, &rel.id);
                        }
                    }
                    if let Some(fix) = &issue.fix {
                        if ui.add(widgets::primary_button(&fix.label)).clicked() {
                            apply.push(fix.op.clone());
                        }
                    }
                });
            });
            ui.label(&issue.message);
            if let Some(s) = &issue.suggestion {
                muted(ui, s);
            }
        });
    }
    let ctx = ui.ctx().clone();
    for op in apply {
        if ws.perform_json(op).is_some() {
            ws.toast(ToastKind::Success, tr("Fix applied"));
            ws.run_audit(&ctx);
        }
    }
    if let Some(id) = ignore {
        let mut ignored = ws.doc.project.settings.health_ignore_issues.clone();
        if !ignored.contains(&id) {
            ignored.push(id);
            if ws
                .perform_json(json!({"op": "update_project", "health_ignore_issues": ignored}))
                .is_some()
            {
                ws.toast(ToastKind::Success, tr("Issue ignored"));
                ws.run_audit(&ctx);
            }
        }
    }
}

pub fn reveal(ws: &mut Workspace, kind: &str, id: &str) {
    ws.flush();
    match kind {
        "memory" => {
            ws.module = Module::Memory;
            ws.memory = Some(id.to_owned());
            ws.memory_draft = None;
        }
        "decision" => {
            ws.module = Module::Decisions;
            ws.decision = ws.doc.decision(id).map(|d| d.id.clone());
            ws.decision_draft = None;
        }
        "node" => {
            ws.module = Module::Architecture;
            ws.node = Some(id.to_owned());
            ws.node_draft = None;
        }
        "prompt" => {
            ws.module = Module::Prompts;
            ws.prompt = Some(id.to_owned());
            ws.prompt_draft = None;
        }
        "handoff" => ws.module = Module::Handoff,
        _ => {}
    }
}

pub fn relative_root(base: &std::path::Path, dir: &std::path::Path) -> String {
    let base = std::fs::canonicalize(base).unwrap_or_else(|_| base.to_path_buf());
    let dir = std::fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());
    if dir == base {
        String::new()
    } else if let Ok(rel) = dir.strip_prefix(&base) {
        rel.to_string_lossy().replace('\\', "/")
    } else {
        engram_core::client_config::display_path(&dir)
    }
}
