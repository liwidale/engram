use crate::i18n::{tr, trf};
use crate::theme;
use crate::widgets::{self, chip, muted};
use crate::workspace::{Dialog, Draft, PromptDraft, ToastKind, Workspace};
use eframe::egui::{self, RichText, Ui};
use engram_core::Prompt;
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};

pub fn unique_name(ws: &Workspace, base: &str) -> String {
    let mut name = base.to_owned();
    let mut n = 2;
    while ws.doc.prompt(&name).is_some() {
        name = format!("{base}-{n}");
        n += 1;
    }
    name
}

pub fn new_prompt(ws: &mut Workspace, from: Option<&Prompt>) {
    commit(ws);
    ws.dialog = Some(match from {
        Some(p) => Dialog::NewPrompt {
            name: unique_name(ws, &format!("{}-copy", p.name)),
            category: p.category.clone(),
            description: p.description.clone(),
            body: p.body.to_string(),
        },
        None => Dialog::NewPrompt {
            name: String::new(),
            category: String::new(),
            description: String::new(),
            body: String::new(),
        },
    });
}

pub fn show(ui: &mut Ui, ws: &mut Workspace) {
    egui::Panel::left("prompt-list")
        .resizable(true)
        .default_size(260.0)
        .show(ui, |ui| list(ui, ws));
    egui::CentralPanel::default().show(ui, |ui| {
        match ws.prompt.clone().and_then(|n| ws.doc.prompt(&n).cloned()) {
            Some(p) => editor(ui, ws, p),
            None => {
                if widgets::empty_state(
                ui,
                tr("Prompt vault"),
                tr("Reusable prompts for this project. Use {{variable}} placeholders and fill them in when you copy."),
                Some(tr("Agents fetch prompts with get_prompt.")),
                Some(tr("New prompt")),
            ) {
                new_prompt(ws, None);
            }
            }
        }
    });
}

fn list(ui: &mut Ui, ws: &mut Workspace) {
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(&mut ws.prompt_filter)
                .hint_text(tr("Filter prompts"))
                .desired_width(ui.available_width() - 70.0),
        );
        if ui.button(tr("New")).clicked() {
            new_prompt(ws, None);
        }
    });
    ui.separator();
    let q = ws.prompt_filter.to_lowercase();
    let prompts: Vec<Prompt> = ws
        .doc
        .prompts
        .iter()
        .filter(|p| q.is_empty() || p.name.to_lowercase().contains(&q) || p.body.to_lowercase().contains(&q))
        .cloned()
        .collect();
    let uncategorized = tr("Uncategorized");
    let categories: BTreeSet<String> = prompts
        .iter()
        .map(|p| {
            if p.category.is_empty() {
                uncategorized.to_owned()
            } else {
                p.category.clone()
            }
        })
        .collect();
    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        if prompts.is_empty() && !ws.doc.prompts.is_empty() {
            muted(ui, tr("No prompts match"));
        }
        for cat in categories {
            ui.add_space(4.0);
            ui.label(RichText::new(&cat).small().strong().color(theme::MUTED));
            for p in prompts.iter().filter(|p| {
                (if p.category.is_empty() {
                    uncategorized
                } else {
                    &p.category
                }) == cat
            }) {
                let selected = ws.prompt.as_deref() == Some(p.name.as_str());
                let resp = ui.selectable_label(selected, &p.name);
                let resp = if p.description.is_empty() {
                    resp
                } else {
                    resp.on_hover_text(&p.description)
                };
                if resp.clicked() && !selected {
                    commit(ws);
                    ws.prompt = Some(p.name.clone());
                    ws.prompt_draft = None;
                }
                resp.context_menu(|ui| {
                    if ui.button(tr("Copy template")).clicked() {
                        ui.ctx().copy_text(p.body.to_string());
                        ui.close();
                    }
                    if ui.button(tr("Duplicate")).clicked() {
                        new_prompt(ws, Some(p));
                        ui.close();
                    }
                    if ui.button(tr("Delete")).clicked() {
                        ws.delete(p.name.clone(), json!({"op": "delete_prompt", "name": p.name}));
                        ui.close();
                    }
                });
            }
        }
    });
}

pub fn commit(ws: &mut Workspace) -> bool {
    let Some(draft) = ws.prompt_draft.as_mut().filter(|d| d.dirty() && d.conflict.is_none()) else {
        return false;
    };
    let v = draft.value.clone();
    let name = v.name.trim().to_owned();
    if name.is_empty() {
        draft.value.name = draft.base.name.clone();
        return false;
    }
    let rename = (name != v.original).then(|| v.original.clone());
    let op = json!({
        "op": "upsert_prompt", "name": name, "body": v.body, "category": v.category,
        "description": v.description, "rename_from": rename
    });
    if ws.perform_json(op).is_some() {
        ws.prompt = Some(name.clone());
        let saved = PromptDraft {
            original: name.clone(),
            name,
            ..v
        };
        if let Some(d) = ws.prompt_draft.as_mut() {
            d.value = saved.clone();
            d.base = saved;
        }
        true
    } else {
        if let Some(d) = ws.prompt_draft.as_mut() {
            d.value.name = d.base.name.clone();
        }
        false
    }
}

pub fn copy_rendered(ctx: &egui::Context, ws: &mut Workspace, name: &str, values: &BTreeMap<String, String>) {
    let Some(p) = ws.doc.prompt(name) else { return };
    let vars: serde_json::Map<String, serde_json::Value> = values
        .iter()
        .filter(|(_, v)| !v.is_empty())
        .map(|(k, v)| (k.clone(), json!(v)))
        .collect();
    let r = p.render(&vars);
    ctx.copy_text(r.text);
    if r.missing.is_empty() {
        ws.toast(ToastKind::Success, tr("Prompt copied"));
    } else {
        ws.toast(
            ToastKind::Info,
            trf(
                "Copied with unfilled variables: {list}",
                &[("list", &r.missing.join(", "))],
            ),
        );
    }
}

fn editor(ui: &mut Ui, ws: &mut Workspace, p: Prompt) {
    if ws.prompt_draft.as_ref().is_none_or(|d| d.base.original != p.name) {
        ws.prompt_draft = Some(Draft::new(PromptDraft::from(&p)));
    }
    let mut changed = false;
    let mut copy = false;
    let mut delete = false;
    ui.add_space(6.0);
    {
        let draft = &mut ws.prompt_draft.as_mut().expect("draft").value;
        ui.horizontal(|ui| {
            changed |= ui
                .add(
                    egui::TextEdit::singleline(&mut draft.name)
                        .font(egui::TextStyle::Monospace)
                        .desired_width(280.0),
                )
                .changed();
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(tr("Delete")).clicked() {
                    delete = true;
                }
                if ui.button(tr("Copy template")).clicked() {
                    ui.ctx().copy_text(draft.body.clone());
                }
                if ui
                    .add(widgets::primary_button(tr("Copy")))
                    .on_hover_text(tr("Copy the prompt and fill its variables"))
                    .clicked()
                {
                    copy = true;
                }
            });
        });
        ui.horizontal(|ui| {
            changed |= ui
                .add(
                    egui::TextEdit::singleline(&mut draft.category)
                        .hint_text(tr("Category"))
                        .desired_width(160.0),
                )
                .changed();
            changed |= ui
                .add(
                    egui::TextEdit::singleline(&mut draft.description)
                        .hint_text(tr("Short description"))
                        .desired_width(f32::INFINITY),
                )
                .changed();
        });
    }
    if ws.prompt_draft.as_ref().is_some_and(|d| d.conflict.is_some()) {
        match widgets::conflict_bar(ui) {
            Some(true) => ws.prompt_draft.as_mut().expect("draft").load_external(),
            Some(false) => ws.prompt_draft.as_mut().expect("draft").keep_mine(),
            None => {}
        }
    }
    ui.separator();
    let height = (ui.available_height() - 40.0).max(160.0);
    let draft = ws.prompt_draft.as_mut().expect("draft");
    egui::ScrollArea::vertical().max_height(height).show(ui, |ui| {
        changed |= widgets::code_edit(
            ui,
            &mut draft.value.body,
            tr("Write the prompt. Use {{variable}} for placeholders."),
            height - 6.0,
        )
        .changed();
    });
    if changed {
        draft.touch();
    }
    draft.refresh_tokens(&draft.value.body.clone());
    let vars = Prompt {
        body: draft.value.body.clone().into(),
        ..p.clone()
    }
    .variables();
    let (tokens, dirty) = (draft.tokens, draft.dirty());
    ui.separator();
    ui.horizontal(|ui| {
        if vars.is_empty() {
            muted(ui, tr("No variables"));
        }
        for v in &vars {
            chip(ui, v, theme::ACCENT);
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            widgets::save_status(ui, dirty);
            widgets::tokens(ui, tokens, None);
        });
    });
    if copy {
        commit(ws);
        let name = ws.prompt.clone().unwrap_or(p.name.clone());
        if vars.is_empty() {
            copy_rendered(ui.ctx(), ws, &name, &BTreeMap::new());
        } else {
            ws.dialog = Some(Dialog::FillPrompt {
                name,
                values: vars.into_iter().map(|v| (v, String::new())).collect(),
            });
        }
    }
    if delete {
        ws.delete(p.name.clone(), json!({"op": "delete_prompt", "name": p.name}));
    }
}
