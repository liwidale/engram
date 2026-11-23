use crate::i18n::{tr, trf};
use crate::views::architecture::kind_title;
use crate::views::memory::category_title;
use crate::widgets::{self, muted};
use crate::workspace::{Dialog, ToastKind, Workspace};
use eframe::egui::{self, Id, RichText};
use engram_core::{MemoryCategory, NodeKind};
use serde_json::json;

enum Outcome {
    Keep,
    Close,
    Submit,
}

pub fn show(ctx: &egui::Context, ws: &mut Workspace) {
    let Some(mut dialog) = ws.dialog.take() else { return };
    let mut outcome = Outcome::Keep;
    let modal = egui::Modal::new(Id::unique("engram-dialog")).show(ctx, |ui| {
        ui.set_width(520.0);
        match &mut dialog {
            Dialog::NewMemory { key, category, content, supersedes } => {
                widgets::heading(ui, match supersedes {
                    Some(old) => trf("Supersede {key}", &[("key", old)]),
                    None => tr("New memory entry").to_owned(),
                });
                if supersedes.is_some() {
                    muted(ui, tr("The old entry is tagged [SUPERSEDED], linked to this one and excluded from future agent context."));
                }
                ui.add_space(6.0);
                ui.label(RichText::new(tr("Key")).small());
                ui.add(egui::TextEdit::singleline(key).hint_text("db-engine").font(egui::TextStyle::Monospace));
                egui::ComboBox::from_label(tr("Category")).selected_text(category_title(*category)).show_ui(ui, |ui| {
                    for c in MemoryCategory::ALL {
                        ui.selectable_value(category, c, category_title(c));
                    }
                });
                widgets::multiline(ui, content, tr("Markdown content"), 8, true);
                widgets::tokens(ui, engram_core::tokens::count_tokens(content), None);
            }
            Dialog::NewDecision { title, context, decision, rejected, supersedes } => {
                widgets::heading(ui, match supersedes {
                    Some(old) => trf("Supersede {id}", &[("id", old)]),
                    None => tr("Log decision").to_owned(),
                });
                ui.add(egui::TextEdit::singleline(title).hint_text(tr("Title (taken from the decision if empty)")).desired_width(f32::INFINITY));
                ui.label(RichText::new(tr("Context")).small());
                widgets::multiline(ui, context, tr("What problem, forces and constraints?"), 3, false);
                ui.label(RichText::new(tr("Decision")).small());
                widgets::multiline(ui, decision, tr("What you will do."), 3, false);
                ui.label(RichText::new(tr("Rejected alternatives")).small());
                for (option, reason) in rejected.iter_mut() {
                    ui.horizontal(|ui| {
                        ui.add(egui::TextEdit::singleline(option).hint_text(tr("Option")).desired_width(150.0));
                        ui.add(egui::TextEdit::singleline(reason).hint_text(tr("Why not")).desired_width(f32::INFINITY));
                    });
                }
                if ui.button(tr("Add alternative")).clicked() {
                    rejected.push((String::new(), String::new()));
                }
            }
            Dialog::NewNode { id, label, kind, path, deps } => {
                widgets::heading(ui, tr("Add architecture node"));
                ui.label(RichText::new(tr("Id")).small());
                ui.add(egui::TextEdit::singleline(id).hint_text("billing-api").font(egui::TextStyle::Monospace));
                ui.label(RichText::new(tr("Label")).small());
                ui.add(egui::TextEdit::singleline(label).hint_text("Billing API"));
                egui::ComboBox::from_label(tr("Type")).selected_text(kind_title(*kind)).show_ui(ui, |ui| {
                    for k in NodeKind::ALL {
                        ui.selectable_value(kind, k, kind_title(k));
                    }
                });
                ui.label(RichText::new(tr("Path")).small());
                ui.add(egui::TextEdit::singleline(path).hint_text("services/billing").font(egui::TextStyle::Monospace));
                if !ws.doc.architecture.nodes.is_empty() {
                    ui.label(RichText::new(tr("Depends on")).small());
                    egui::ScrollArea::vertical().max_height(160.0).show(ui, |ui| {
                        for n in &ws.doc.architecture.nodes {
                            let mut on = deps.contains(&n.id);
                            if ui.checkbox(&mut on, n.display_name()).changed() {
                                if on {
                                    deps.insert(n.id.clone());
                                } else {
                                    deps.remove(&n.id);
                                }
                            }
                        }
                    });
                }
            }
            Dialog::FillPrompt { name, values } => {
                widgets::heading(ui, trf("Fill variables for {name}", &[("name", name)]));
                for (var, value) in values.iter_mut() {
                    ui.label(RichText::new(var).monospace().small());
                    ui.add(egui::TextEdit::multiline(value).desired_rows(1).desired_width(f32::INFINITY));
                }
            }
        }
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let label = match &dialog {
                    Dialog::NewMemory { supersedes: Some(_), .. } | Dialog::NewDecision { supersedes: Some(_), .. } => tr("Supersede"),
                    Dialog::NewMemory { .. } => tr("Create"),
                    Dialog::NewDecision { .. } => tr("Log decision"),
                    Dialog::NewNode { .. } => tr("Add"),
                    Dialog::FillPrompt { .. } => tr("Copy prompt"),
                };
                let enabled = match &dialog {
                    Dialog::NewMemory { key, content, .. } => !key.trim().is_empty() && !content.trim().is_empty(),
                    Dialog::NewDecision { context, decision, .. } => !context.trim().is_empty() && !decision.trim().is_empty(),
                    Dialog::NewNode { id, .. } => !id.trim().is_empty(),
                    Dialog::FillPrompt { .. } => true,
                };
                if ui.add_enabled(enabled, widgets::primary_button(label)).clicked() || (enabled && ui.input(|i| i.modifiers.command && i.key_pressed(egui::Key::Enter))) {
                    outcome = Outcome::Submit;
                }
                if ui.button(tr("Cancel")).clicked() {
                    outcome = Outcome::Close;
                }
            });
        });
    });
    if modal.should_close() && matches!(outcome, Outcome::Keep) {
        outcome = Outcome::Close;
    }
    match outcome {
        Outcome::Keep => ws.dialog = Some(dialog),
        Outcome::Close => {}
        Outcome::Submit => {
            if !submit(ctx, ws, &dialog) {
                ws.dialog = Some(dialog);
            }
        }
    }
}

fn submit(ctx: &egui::Context, ws: &mut Workspace, dialog: &Dialog) -> bool {
    match dialog {
        Dialog::NewMemory {
            key,
            category,
            content,
            supersedes,
        } => {
            let key = key.trim().to_owned();
            if supersedes.is_none() && ws.doc.memory_entry(&key).is_some() {
                ws.toast(
                    ToastKind::Error,
                    trf("An entry named {key} already exists", &[("key", &key)]),
                );
                return false;
            }
            crate::views::memory::commit(ws);
            let op = json!({"op": "upsert_memory", "key": key, "category": category.as_str(), "content": content, "supersedes_key": supersedes});
            if ws.perform_json(op).is_none() {
                return false;
            }
            ws.memory = Some(key);
            ws.memory_draft = None;
            true
        }
        Dialog::NewDecision {
            title,
            context,
            decision,
            rejected,
            supersedes,
        } => {
            let rejected: Vec<serde_json::Value> = rejected
                .iter()
                .filter(|(o, _)| !o.trim().is_empty())
                .map(|(o, r)| json!({"option": o, "reason": r}))
                .collect();
            let op = json!({
                "op": "log_decision", "title": if title.trim().is_empty() { None } else { Some(title.trim()) },
                "context": context, "decision": decision, "rejected_alternatives": rejected,
                "related": supersedes.iter().collect::<Vec<_>>(), "supersedes": supersedes
            });
            crate::views::decisions::commit(ws);
            let Some(result) = ws.perform_json(op) else {
                return false;
            };
            if let Some(id) = result.get("id").and_then(|v| v.as_str()) {
                ws.decision = Some(id.to_owned());
                ws.decision_draft = None;
                ws.toast(ToastKind::Success, trf("Logged {id}", &[("id", &id)]));
            }
            true
        }
        Dialog::NewNode {
            id,
            label,
            kind,
            path,
            deps,
        } => {
            let order: Vec<&String> = ws
                .doc
                .architecture
                .nodes
                .iter()
                .map(|n| &n.id)
                .filter(|n| deps.contains(*n))
                .collect();
            let op = json!({
                "op": "add_architecture_node", "id": id.trim(), "type": kind.as_str(), "dependencies": order,
                "label": label, "path": path
            });
            if ws.perform_json(op).is_none() {
                return false;
            }
            ws.node = Some(id.trim().to_owned());
            ws.node_edit = None;
            ws.graph.scene_rect = None;
            true
        }
        Dialog::FillPrompt { name, values } => {
            crate::views::prompts::copy_rendered(ctx, ws, name, values);
            true
        }
    }
}
