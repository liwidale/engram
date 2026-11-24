use crate::i18n::{tr, trf};
use crate::settings;
use crate::theme;
use crate::widgets::{self, muted};
use crate::workspace::{Draft, HandoffDraft, ToastKind, Workspace};
use eframe::egui::{self, RichText, Ui};
use serde_json::json;

pub fn commit(ws: &mut Workspace) -> bool {
    let Some(draft) = ws
        .handoff_draft
        .as_mut()
        .filter(|d| d.dirty() && d.value.valid() && d.conflict.is_none())
    else {
        return false;
    };
    let v = draft.value.clone();
    draft.mark_saved();
    let progress: Vec<serde_json::Value> = v
        .progress
        .iter()
        .filter(|(d, _, _)| !d.trim().is_empty())
        .map(|(d, files, verified)| json!({"description": d.trim(), "files": files, "verified": verified}))
        .collect();
    ws.perform_json(json!({
        "op": "update_handoff",
        "objective": v.objective,
        "progress": progress,
        "working_memory": {
            "hypotheses": widgets::lines_to_list(&v.hypotheses),
            "blockers": widgets::lines_to_list(&v.blockers),
            "notes": v.notes,
        },
        "next_step": v.next_step,
        "updated_by": settings::get().author(),
    }))
    .is_some()
}

pub fn show(ui: &mut Ui, ws: &mut Workspace) {
    if ws.handoff_draft.is_none() {
        ws.handoff_draft = Some(Draft::new(HandoffDraft::from(&ws.doc.handoff)));
    }
    let mut changed = false;
    let mut copy: Option<bool> = None;
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
                    _ => tr("A checkpoint agents overwrite with update_handoff before they stop.").to_owned(),
                },
            );
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button(tr("Copy as Markdown")).clicked() {
                copy = Some(false);
            }
            if ui.button(tr("Copy as XML")).clicked() {
                copy = Some(true);
            }
            let d = ws.handoff_draft.as_ref().expect("draft");
            if d.dirty() && !d.value.valid() {
                ui.label(
                    RichText::new(tr("Add an objective and a next step to save"))
                        .small()
                        .color(theme::WARNING),
                );
            } else {
                widgets::save_status(ui, d.dirty());
            }
        });
    });
    if ws.handoff_draft.as_ref().is_some_and(|d| d.conflict.is_some()) {
        match widgets::conflict_bar(ui) {
            Some(true) => ws.handoff_draft.as_mut().expect("draft").load_external(),
            Some(false) => ws.handoff_draft.as_mut().expect("draft").keep_mine(),
            None => {}
        }
    }
    ui.separator();
    let queries: Vec<(String, Vec<String>)> = (0..ws.handoff_draft.as_ref().expect("draft").value.progress.len())
        .map(|i| {
            let q = ws.path_query.get(&format!("progress-{i}")).cloned().unwrap_or_default();
            let hits = if q.trim().is_empty() {
                Vec::new()
            } else {
                ws.repo_files(&q)
            };
            (q, hits)
        })
        .collect();
    let mut new_queries: Vec<String> = Vec::new();
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
                muted(ui, tr("Finished, checkable work with the files it touched."));
            }
            let mut remove = None;
            for (i, ((desc, files, verified), (q, hits))) in
                d.progress.iter_mut().zip(queries.iter().cloned()).enumerate()
            {
                let mut q = q;
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
                        changed |= widgets::file_list(ui, &format!("progress-files-{i}"), files, &mut q, |_| hits);
                    });
                });
                if ui.small_button(tr("Remove")).clicked() {
                    remove = Some(i);
                }
                new_queries.push(q);
                ui.add_space(4.0);
            }
            if let Some(i) = remove {
                d.progress.remove(i);
                changed = true;
            }
            if ui.button(tr("Add progress")).clicked() {
                d.progress.push((String::new(), Vec::new(), true));
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
    for (i, q) in new_queries.into_iter().enumerate() {
        ws.path_query.insert(format!("progress-{i}"), q);
    }
    if changed {
        ws.handoff_draft.as_mut().expect("draft").touch();
    }
    if let Some(xml) = copy {
        commit(ws);
        let text = if xml {
            engram_core::render::handoff_xml(&ws.doc)
        } else {
            engram_core::render::handoff_markdown(&ws.doc)
        };
        ui.ctx().copy_text(text);
        ws.toast(ToastKind::Success, tr("Handoff copied"));
    }
}
