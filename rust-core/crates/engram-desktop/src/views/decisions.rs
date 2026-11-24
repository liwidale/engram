use crate::i18n::{tr, trf};
use crate::markdown;
use crate::theme;
use crate::widgets::{self, chip, muted};
use crate::workspace::{DecisionDraft, Dialog, Draft, Workspace};
use eframe::egui::{self, RichText, Ui};
use engram_core::{Decision, DecisionStatus};
use serde_json::json;

pub fn new_decision(ws: &mut Workspace, supersede: Option<&Decision>) {
    ws.dialog = Some(match supersede {
        Some(d) => Dialog::NewDecision {
            title: String::new(),
            context: trf("Supersedes {id} ({title}).", &[("id", &d.id), ("title", &d.title)]) + "\n\n" + &d.context,
            decision: String::new(),
            rejected: vec![(d.title.clone(), String::new())],
            supersedes: Some(d.id.clone()),
        },
        None => Dialog::NewDecision {
            title: String::new(),
            context: String::new(),
            decision: String::new(),
            rejected: vec![(String::new(), String::new())],
            supersedes: None,
        },
    });
}

pub fn show(ui: &mut Ui, ws: &mut Workspace) {
    egui::Panel::left("decision-list")
        .resizable(true)
        .default_size(300.0)
        .show(ui, |ui| list(ui, ws));
    egui::CentralPanel::default().show(ui, |ui| {
        match ws.decision.clone().and_then(|id| ws.doc.decision(&id).cloned()) {
            Some(d) => editor(ui, ws, d),
            None => {
                if widgets::empty_state(
                    ui,
                    tr("Decision log"),
                    tr("Record what you decided and which options you ruled out, so nobody proposes them again."),
                    Some(tr("Agents add records with log_decision.")),
                    Some(tr("Log decision")),
                ) {
                    new_decision(ws, None);
                }
            }
        }
    });
}

fn list(ui: &mut Ui, ws: &mut Workspace) {
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(&mut ws.decision_filter)
                .hint_text(tr("Filter decisions"))
                .desired_width(ui.available_width() - 70.0),
        );
        if ui.button(tr("New")).on_hover_text(tr("Log decision")).clicked() {
            new_decision(ws, None);
        }
    });
    ui.separator();
    let q = ws.decision_filter.to_lowercase();
    let mut items: Vec<Decision> = ws
        .doc
        .decisions
        .iter()
        .filter(|d| {
            q.is_empty()
                || format!("{} {} {} {}", d.id, d.title, d.context, d.decision)
                    .to_lowercase()
                    .contains(&q)
                || d.rejected_alternatives
                    .iter()
                    .any(|a| a.option.to_lowercase().contains(&q))
        })
        .cloned()
        .collect();
    items.sort_by(|a, b| (!a.is_active(), std::cmp::Reverse(&b.id)).cmp(&(!b.is_active(), std::cmp::Reverse(&a.id))));
    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        if items.is_empty() && !ws.doc.decisions.is_empty() {
            muted(ui, tr("No decisions match"));
        }
        for d in items {
            let selected = ws.decision.as_deref() == Some(d.id.as_str());
            let resp = egui::Frame::new()
                .fill(if selected {
                    theme::ACCENT.gamma_multiply(0.15)
                } else {
                    egui::Color32::TRANSPARENT
                })
                .corner_radius(6)
                .inner_margin(egui::Margin::symmetric(6, 4))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(&d.id).monospace().strong().color(theme::ACCENT));
                        if d.is_active() {
                            chip(ui, tr("Active"), theme::MUTED);
                        } else {
                            chip(
                                ui,
                                d.superseded_by
                                    .as_ref()
                                    .map(|s| format!("→ {s}"))
                                    .unwrap_or_else(|| tr("Deprecated").into()),
                                theme::SUPERSEDED,
                            );
                        }
                    });
                    let title = RichText::new(&d.title);
                    ui.label(if d.is_active() {
                        title
                    } else {
                        title.strikethrough().color(theme::MUTED)
                    });
                    if !d.rejected_alternatives.is_empty() {
                        let names: Vec<&str> = d.rejected_alternatives.iter().map(|a| a.option.as_str()).collect();
                        muted(ui, trf("Rejected: {list}", &[("list", &names.join(", "))]));
                    }
                })
                .response
                .interact(egui::Sense::click());
            if resp.clicked() && !selected {
                commit(ws);
                ws.decision = Some(d.id.clone());
                ws.decision_draft = None;
            }
            resp.context_menu(|ui| {
                if ui.button(tr("Copy as Markdown")).clicked() {
                    ui.ctx().copy_text(engram_core::render::decision_markdown(&d));
                    ui.close();
                }
                if d.is_active() {
                    if ui.button(tr("Supersede with new decision")).clicked() {
                        new_decision(ws, Some(&d));
                        ui.close();
                    }
                    if ui.button(tr("Deprecate")).clicked() {
                        ws.perform_json(json!({"op": "update_decision", "id": d.id, "status": "deprecated"}));
                        ui.close();
                    }
                } else if ui.button(tr("Reactivate")).clicked() {
                    ws.perform_json(json!({"op": "update_decision", "id": d.id, "status": "active"}));
                    ui.close();
                }
                ui.separator();
                if ui.button(tr("Delete")).clicked() {
                    ws.delete(d.id.clone(), json!({"op": "delete_decision", "id": d.id}));
                    ui.close();
                }
            });
        }
    });
}

fn text_card(ui: &mut Ui, title: &str, text: &mut String, hint: &str, rows: usize, preview: bool) -> bool {
    widgets::card(ui, Some(title), |ui| {
        if preview {
            markdown::show(ui, if text.is_empty() { tr("Empty") } else { text });
            false
        } else {
            widgets::multiline(ui, text, hint, rows, false).changed()
        }
    })
}

pub fn commit(ws: &mut Workspace) -> bool {
    let Some(draft) = ws.decision_draft.as_mut().filter(|d| d.dirty() && d.conflict.is_none()) else {
        return false;
    };
    let v = draft.value.clone();
    draft.mark_saved();
    let rejected: Vec<serde_json::Value> = v
        .rejected
        .iter()
        .filter(|(o, _)| !o.trim().is_empty())
        .map(|(o, r)| json!({"option": o, "reason": r}))
        .collect();
    ws.perform_json(json!({
        "op": "update_decision", "id": v.id, "title": v.title, "context": v.context, "decision": v.decision,
        "rejected_alternatives": rejected, "consequences": v.consequences, "related": v.related, "files": v.files
    }))
    .is_some()
}

fn editor(ui: &mut Ui, ws: &mut Workspace, d: Decision) {
    if ws.decision_draft.as_ref().is_none_or(|x| x.base.id != d.id) {
        ws.decision_draft = Some(Draft::new(DecisionDraft::from(&d)));
    }
    let mut changed = false;
    let mut ops: Vec<serde_json::Value> = Vec::new();
    let mut supersede = false;
    let mut delete = false;
    let preview = ws.decision_preview;
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.label(RichText::new(&d.id).monospace().strong().color(theme::ACCENT));
        let draft = &mut ws.decision_draft.as_mut().expect("draft").value;
        changed |= ui
            .add(
                egui::TextEdit::singleline(&mut draft.title)
                    .font(egui::TextStyle::Heading)
                    .desired_width(420.0),
            )
            .changed();
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.menu_button(tr("More"), |ui| {
                if d.is_active() && ui.button(tr("Supersede with new decision")).clicked() {
                    supersede = true;
                    ui.close();
                }
                if ui.button(tr("Copy as Markdown")).clicked() {
                    ui.ctx().copy_text(engram_core::render::decision_markdown(&d));
                    ui.close();
                }
                if ui.button(tr("Delete")).clicked() {
                    delete = true;
                    ui.close();
                }
            });
            let mut p = preview;
            if ui.toggle_value(&mut p, tr("Preview")).changed() {
                ws.decision_preview = p;
            }
            let mut status = d.status;
            ui.selectable_value(&mut status, DecisionStatus::Deprecated, tr("Deprecated"));
            ui.selectable_value(&mut status, DecisionStatus::Active, tr("Active"));
            if status != d.status {
                ops.push(json!({"op": "update_decision", "id": d.id, "status": status.as_str()}));
            }
        });
    });
    if !d.is_active() {
        let text = match &d.superseded_by {
            Some(by) => trf(
                "Deprecated and replaced by {id}. Agents no longer receive this decision.",
                &[("id", by)],
            ),
            None => tr("Deprecated. Agents no longer receive this decision.").to_owned(),
        };
        ui.label(RichText::new(text).color(theme::SUPERSEDED));
    }
    if ws.decision_draft.as_ref().is_some_and(|x| x.conflict.is_some()) {
        match widgets::conflict_bar(ui) {
            Some(true) => ws.decision_draft.as_mut().expect("draft").load_external(),
            Some(false) => ws.decision_draft.as_mut().expect("draft").keep_mine(),
            None => {}
        }
    }
    ui.separator();
    let mut query = ws.path_query.remove("decision").unwrap_or_default();
    let hits = if query.trim().is_empty() {
        Vec::new()
    } else {
        ws.repo_files(&query)
    };
    let candidates: Vec<String> = ws
        .doc
        .memory
        .iter()
        .filter(|m| m.is_active())
        .map(|m| m.key.clone())
        .chain(ws.doc.architecture.nodes.iter().map(|n| n.id.clone()))
        .chain(ws.doc.decisions.iter().filter(|x| x.id != d.id).map(|x| x.id.clone()))
        .collect();
    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        let draft = &mut ws.decision_draft.as_mut().expect("draft").value;
        changed |= text_card(
            ui,
            tr("Context"),
            &mut draft.context,
            tr("What forces and constraints led here?"),
            3,
            preview,
        );
        changed |= text_card(
            ui,
            tr("Decision"),
            &mut draft.decision,
            tr("What did you decide?"),
            3,
            preview,
        );
        widgets::card(ui, Some(tr("Rejected alternatives")), |ui| {
            if draft.rejected.is_empty() {
                muted(
                    ui,
                    tr("Note what you considered and why it lost. Agents treat these as dead ends."),
                );
            }
            let mut remove = None;
            for (i, (option, reason)) in draft.rejected.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    changed |= ui
                        .add(
                            egui::TextEdit::singleline(option)
                                .hint_text(tr("Option"))
                                .desired_width(170.0),
                        )
                        .changed();
                    changed |= ui
                        .add(
                            egui::TextEdit::singleline(reason)
                                .hint_text(tr("Why it was rejected"))
                                .desired_width(ui.available_width() - 70.0),
                        )
                        .changed();
                    if ui.button(tr("Remove")).clicked() {
                        remove = Some(i);
                    }
                });
            }
            if let Some(i) = remove {
                draft.rejected.remove(i);
                changed = true;
            }
            if ui.button(tr("Add alternative")).clicked() {
                draft.rejected.push((String::new(), String::new()));
            }
        });
        changed |= text_card(
            ui,
            tr("Consequences"),
            &mut draft.consequences,
            tr("Trade-offs you accept (optional)"),
            2,
            preview,
        );
        widgets::card(ui, Some(tr("Affected files")), |ui| {
            changed |= widgets::file_list(ui, "decision-files", &mut draft.files, &mut query, |_| hits);
        });
        widgets::card(ui, Some(tr("Related")), |ui| {
            let mut remove = None;
            ui.horizontal_wrapped(|ui| {
                for (i, r) in draft.related.iter().enumerate() {
                    chip(ui, r, theme::ACCENT);
                    if ui.small_button("x").on_hover_text(tr("Remove")).clicked() {
                        remove = Some(i);
                    }
                }
            });
            if let Some(i) = remove {
                draft.related.remove(i);
                changed = true;
            }
            ui.menu_button(tr("Link memory, node or decision"), |ui| {
                egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
                    let open: Vec<&String> = candidates.iter().filter(|c| !draft.related.contains(c)).collect();
                    for c in open {
                        if ui.button(c).clicked() {
                            draft.related.push(c.clone());
                            changed = true;
                            ui.close();
                        }
                    }
                });
            });
        });
        ui.horizontal(|ui| {
            muted(
                ui,
                trf(
                    "Recorded {when}",
                    &[("when", &widgets::relative_time(Some(&d.created_at)))],
                ),
            );
        });
    });
    ws.path_query.insert("decision".into(), query);
    let draft = ws.decision_draft.as_mut().expect("draft");
    if changed {
        draft.touch();
    }
    let dirty = draft.dirty();
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            widgets::save_status(ui, dirty)
        });
    });
    if supersede {
        commit(ws);
        new_decision(ws, Some(&d));
    }
    if delete {
        ws.delete(d.id.clone(), json!({"op": "delete_decision", "id": d.id}));
    }
    for op in ops {
        commit(ws);
        ws.perform_json(op);
    }
}
