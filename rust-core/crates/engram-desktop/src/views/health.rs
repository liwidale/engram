use crate::i18n::{tr, trf};
use crate::theme;
use crate::widgets::{self, chip, muted};
use crate::workspace::{Module, Workspace};
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

fn kind_title(kind: &impl serde::Serialize) -> String {
    let raw = serde_json::to_value(kind)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default();
    let mut s = raw.replace('_', " ");
    if let Some(first) = s.get_mut(..1) {
        first.make_ascii_uppercase();
    }
    s
}

pub fn show(ui: &mut Ui, ws: &mut Workspace) {
    let stale = ws.health.as_ref().is_none_or(|r| r.revision != ws.doc.revision);
    if stale && ws.auditing.is_none() {
        ws.run_audit(ui.ctx());
    }
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            muted(ui, tr("Linked codebase"));
            ui.label(RichText::new(ws.resolved_repo_root().display().to_string()).monospace());
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.add(widgets::primary_button(tr("Run audit"))).clicked() {
                ws.health = None;
                ws.run_audit(ui.ctx());
            }
            if ws.doc.project.repo_root.is_some() && ui.button(tr("Use .engram folder")).clicked() {
                ws.perform_json(json!({"op": "update_project", "repo_root": ""}));
            }
            if ui.button(tr("Change folder")).clicked() {
                choose_root(ws);
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
    widgets::card(ui, None, |ui| {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(r.score.to_string())
                    .size(44.0)
                    .strong()
                    .color(theme::score(r.score)),
            );
            ui.vertical(|ui| {
                ui.label(
                    RichText::new(match r.grade {
                        "healthy" => tr("Healthy"),
                        "needs_attention" => tr("Needs attention"),
                        _ => tr("Critical"),
                    })
                    .size(18.0)
                    .strong(),
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
                    chip(ui, trf("{n} errors", &[("n", &r.summary.errors)]), theme::ERROR);
                    chip(ui, trf("{n} warnings", &[("n", &r.summary.warnings)]), theme::WARNING);
                    chip(ui, trf("{n} info", &[("n", &r.summary.infos)]), theme::INFO);
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
    if r.issues.is_empty() {
        ui.add_space(30.0);
        ui.vertical_centered(|ui| {
            ui.label(RichText::new(tr("No issues. Agents get clean, current context.")).color(theme::SUCCESS));
        });
        return;
    }
    for issue in &r.issues {
        widgets::card(ui, None, |ui| {
            ui.horizontal(|ui| {
                chip(ui, severity_title(issue.severity), theme::severity(issue.severity));
                chip(ui, kind_title(&issue.kind), theme::MUTED);
                muted(ui, format!("{} · {}", issue.subject.kind, issue.subject.id));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if issue.subject.kind != "project" && ui.link(tr("Open")).clicked() {
                        reveal(ws, issue.subject.kind, &issue.subject.id);
                    }
                    if let Some(rel) = &issue.related {
                        if ui.link(trf("Open {id}", &[("id", &rel.id)])).clicked() {
                            reveal(ws, rel.kind, &rel.id);
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
}

pub fn reveal(ws: &mut Workspace, kind: &str, id: &str) {
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
            ws.node_edit = None;
        }
        "handoff" => ws.module = Module::Handoff,
        _ => {}
    }
}

fn choose_root(ws: &mut Workspace) {
    let base = ws.store.base_dir();
    let Some(dir) = rfd::FileDialog::new()
        .set_directory(ws.resolved_repo_root())
        .set_title(tr("Choose the codebase this project memory describes"))
        .pick_folder()
    else {
        return;
    };
    let base = std::fs::canonicalize(&base).unwrap_or(base);
    let dir = std::fs::canonicalize(&dir).unwrap_or(dir);
    let value = if dir == base {
        String::new()
    } else if let Ok(rel) = dir.strip_prefix(&base) {
        rel.to_string_lossy().replace('\\', "/")
    } else {
        engram_core::client_config::display_path(&dir)
    };
    ws.perform_json(json!({"op": "update_project", "repo_root": value}));
}
