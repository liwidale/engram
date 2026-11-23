use crate::i18n::{tr, trf};
use crate::theme;
use crate::widgets::{self, muted};
use crate::workspace::{Draft, HandoffDraft, ToastKind, Workspace};
use eframe::egui::{self, RichText, Ui};
use serde_json::json;
use std::time::Instant;

fn draft_from(ws: &Workspace) -> HandoffDraft {
    let h = &ws.doc.handoff;
    HandoffDraft {
        objective: h.objective.clone(),
        progress: h
            .progress
            .iter()
            .map(|p| (p.description.clone(), p.files.join(", "), p.verified))
            .collect(),
        hypotheses: h.working_memory.hypotheses.join("\n"),
        blockers: h.working_memory.blockers.join("\n"),
        notes: h.working_memory.notes.to_string(),
        next_step: h.next_step.clone(),
    }
}

fn valid(d: &HandoffDraft) -> bool {
    !d.objective.trim().is_empty() && !d.next_step.trim().is_empty()
}

pub fn commit(ws: &mut Workspace) {
    let Some(draft) = ws.handoff_draft.as_ref().filter(|d| d.dirty() && valid(&d.value)) else {
        return;
    };
    let v = draft.value.clone();
    let progress: Vec<serde_json::Value> = v
        .progress
        .iter()
        .filter(|(d, _, _)| !d.trim().is_empty())
        .map(|(d, files, verified)| {
            let files: Vec<&str> = files.split(',').map(str::trim).filter(|f| !f.is_empty()).collect();
            json!({"description": d.trim(), "files": files, "verified": verified})
        })
        .collect();
    let user = std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_default();
    let op = json!({
        "op": "update_handoff",
        "objective": v.objective,
        "progress": progress,
        "working_memory": {
            "hypotheses": widgets::lines_to_list(&v.hypotheses),
            "blockers": widgets::lines_to_list(&v.blockers),
            "notes": v.notes,
        },
        "next_step": v.next_step,
        "updated_by": if user.is_empty() { "Engram".to_owned() } else { format!("Engram ({user})") },
    });
    if ws.perform_json(op).is_some() {
        ws.handoff_draft = Some(Draft::new(draft_from(ws)));
    }
}

pub fn show(ui: &mut Ui, ws: &mut Workspace) {
    if ws.handoff_draft.is_none() {
        ws.handoff_draft = Some(Draft::new(draft_from(ws)));
    }
    let mut changed = false;
    let mut copy: Option<&str> = None;
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            widgets::heading(ui, tr("Agent handoff"));
            let h = &ws.doc.handoff;
            muted(
                ui,
                match (&h.updated_at, &h.updated_by) {
                    (Some(at), Some(by)) => trf(
                        "Updated {when} by {who}",
                        &[("when", &widgets::relative_time(Some(at))), ("who", by)],
                    ),
                    (Some(at), None) => trf("Updated {when}", &[("when", &widgets::relative_time(Some(at)))]),
                    _ => tr("Agents overwrite this with update_handoff before they stop.").to_owned(),
                },
            );
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let d = ws.handoff_draft.as_ref().expect("draft");
            if ui
                .add_enabled(d.dirty() && valid(&d.value), egui::Button::new(tr("Save")))
                .clicked()
            {
                copy = Some("save");
            }
            if ui.button(tr("Copy as Markdown")).clicked() {
                copy = Some("md");
            }
            if ui.button(tr("Copy as XML")).clicked() {
                copy = Some("xml");
            }
            if d.dirty() && !valid(&d.value) {
                ui.label(
                    RichText::new(tr("Objective and next step are required"))
                        .small()
                        .color(theme::WARNING),
                );
            }
        });
    });
    ui.separator();
    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        let d = &mut ws.handoff_draft.as_mut().expect("draft").value;
        widgets::card(ui, Some(tr("Active objective")), |ui| {
            changed |= widgets::multiline(
                ui,
                &mut d.objective,
                tr("What is the agent trying to achieve?"),
                2,
                false,
            )
            .changed();
        });
        widgets::card(ui, Some(tr("Verified progress")), |ui| {
            if d.progress.is_empty() {
                muted(ui, tr("Completed, checkable work with the files it touched."));
            }
            let mut remove = None;
            for (i, (desc, files, verified)) in d.progress.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    changed |= ui.checkbox(verified, "").on_hover_text(tr("Verified")).changed();
                    ui.vertical(|ui| {
                        changed |= ui
                            .add(
                                egui::TextEdit::singleline(desc)
                                    .hint_text(tr("What was done"))
                                    .desired_width(f32::INFINITY),
                            )
                            .changed();
                        changed |= ui
                            .add(
                                egui::TextEdit::singleline(files)
                                    .hint_text(tr("files, comma separated"))
                                    .font(egui::TextStyle::Monospace)
                                    .desired_width(f32::INFINITY),
                            )
                            .changed();
                    });
                });
                if ui.small_button(tr("Remove")).clicked() {
                    remove = Some(i);
                }
                ui.add_space(4.0);
            }
            if let Some(i) = remove {
                d.progress.remove(i);
                changed = true;
            }
            if ui.button(tr("Add progress")).clicked() {
                d.progress.push((String::new(), String::new(), true));
            }
        });
        ui.columns(2, |cols| {
            widgets::card(&mut cols[0], Some(tr("Hypotheses")), |ui| {
                changed |= widgets::multiline(ui, &mut d.hypotheses, tr("One per line"), 3, false).changed();
            });
            widgets::card(&mut cols[1], Some(tr("Blockers")), |ui| {
                changed |= widgets::multiline(ui, &mut d.blockers, tr("One per line"), 3, false).changed();
            });
        });
        widgets::card(ui, Some(tr("Working notes")), |ui| {
            changed |= widgets::multiline(
                ui,
                &mut d.notes,
                tr("Notes that should outlive the context window"),
                3,
                false,
            )
            .changed();
        });
        widgets::card(ui, Some(tr("Next atomic step")), |ui| {
            changed |= ui
                .add(
                    egui::TextEdit::multiline(&mut d.next_step)
                        .hint_text(tr("The single next action, small enough to verify"))
                        .desired_rows(2)
                        .desired_width(f32::INFINITY)
                        .font(egui::TextStyle::Heading),
                )
                .changed();
        });
    });
    if changed {
        if let Some(x) = ws.handoff_draft.as_mut() {
            x.edited_at = Instant::now();
        }
    }
    match copy {
        Some("save") => commit(ws),
        Some(kind) => {
            commit(ws);
            let text = if kind == "xml" {
                engram_core::render::handoff_xml(&ws.doc)
            } else {
                engram_core::render::handoff_markdown(&ws.doc)
            };
            ui.ctx().copy_text(text);
            ws.toast(ToastKind::Success, tr("Handoff copied"));
        }
        None => {}
    }
}
