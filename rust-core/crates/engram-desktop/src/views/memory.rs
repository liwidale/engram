use crate::i18n::{tr, trf};
use crate::markdown;
use crate::theme;
use crate::widgets::{self, chip, muted};
use crate::workspace::{Dialog, Draft, MemoryDraft, Workspace};
use eframe::egui::{self, RichText, Ui};
use engram_core::{MemoryCategory, MemoryEntry};
use serde_json::json;

pub fn category_title(c: MemoryCategory) -> &'static str {
    match c {
        MemoryCategory::Invariant => tr("Invariants"),
        MemoryCategory::Rule => tr("Rules"),
        MemoryCategory::TechStack => tr("Tech stack"),
        MemoryCategory::Convention => tr("Conventions"),
        MemoryCategory::Note => tr("Notes"),
    }
}

const ORDER: [MemoryCategory; 5] = [
    MemoryCategory::Invariant,
    MemoryCategory::Rule,
    MemoryCategory::TechStack,
    MemoryCategory::Convention,
    MemoryCategory::Note,
];

pub fn show(ui: &mut Ui, ws: &mut Workspace) {
    egui::Panel::left("memory-list")
        .resizable(true)
        .default_size(290.0)
        .show(ui, |ui| list(ui, ws));
    egui::CentralPanel::default().show(ui, |ui| match ws.memory.clone().and_then(|k| ws.doc.memory_entry(&k).cloned()) {
        Some(entry) => editor(ui, ws, entry),
        None => {
            if widgets::empty_state(
                ui,
                tr("Project memory"),
                tr("Rules, tech stack, architecture invariants and conventions for this codebase. Agents read them through engram://memory."),
                Some(tr("New entry")),
            ) {
                new_entry(ws, None);
            }
        }
    });
}

pub fn new_entry(ws: &mut Workspace, supersede: Option<&MemoryEntry>) {
    ws.dialog = Some(match supersede {
        Some(e) => {
            let base = e
                .key
                .trim_end_matches(|c: char| c.is_ascii_digit())
                .trim_end_matches("-v")
                .to_owned();
            let mut n = 2;
            while ws.doc.memory_entry(&format!("{base}-v{n}")).is_some() {
                n += 1;
            }
            Dialog::NewMemory {
                key: format!("{base}-v{n}"),
                category: e.category,
                content: e.body().to_owned(),
                supersedes: Some(e.key.clone()),
            }
        }
        None => Dialog::NewMemory {
            key: String::new(),
            category: MemoryCategory::Rule,
            content: String::new(),
            supersedes: None,
        },
    });
}

fn list(ui: &mut Ui, ws: &mut Workspace) {
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(&mut ws.memory_filter)
                .hint_text(tr("Filter memory"))
                .desired_width(ui.available_width() - 70.0),
        );
        if ui.button(tr("New")).on_hover_text(tr("New memory entry")).clicked() {
            new_entry(ws, None);
        }
    });
    ui.checkbox(&mut ws.show_superseded, tr("Show superseded"));
    ui.separator();
    let q = ws.memory_filter.to_lowercase();
    let entries: Vec<MemoryEntry> = ws
        .doc
        .memory
        .iter()
        .filter(|e| ws.show_superseded || e.is_active())
        .filter(|e| q.is_empty() || e.key.to_lowercase().contains(&q) || e.content.to_lowercase().contains(&q))
        .cloned()
        .collect();
    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        if entries.is_empty() {
            muted(
                ui,
                if ws.doc.memory.is_empty() {
                    tr("No entries yet")
                } else {
                    tr("No matches")
                },
            );
        }
        for cat in ORDER {
            let mut group: Vec<&MemoryEntry> = entries.iter().filter(|e| e.category == cat).collect();
            if group.is_empty() {
                continue;
            }
            group.sort_by_key(|e| (!e.is_active(), e.key.clone()));
            ui.add_space(4.0);
            ui.label(
                RichText::new(category_title(cat))
                    .small()
                    .strong()
                    .color(theme::category(cat)),
            );
            for e in group {
                let selected = ws.memory.as_deref() == Some(&e.key);
                let mut title = RichText::new(&e.key).monospace();
                if !e.is_active() {
                    title = title.strikethrough().color(theme::MUTED);
                }
                let resp = ui.selectable_label(selected, title).on_hover_text(summary(e));
                if resp.clicked() && !selected {
                    commit(ws);
                    ws.memory = Some(e.key.clone());
                    ws.memory_draft = None;
                }
                resp.context_menu(|ui| {
                    if ui.button(tr("Copy as Markdown")).clicked() {
                        ui.ctx().copy_text(format!("### {}\n\n{}", e.key, e.content));
                        ui.close();
                    }
                    if e.is_active() && ui.button(tr("Supersede with new entry")).clicked() {
                        new_entry(ws, Some(e));
                        ui.close();
                    }
                    if ui.button(tr("Delete")).clicked() {
                        ws.perform_json(json!({"op": "delete_memory", "key": e.key}));
                        ui.close();
                    }
                });
            }
        }
    });
}

fn summary(e: &MemoryEntry) -> String {
    e.body()
        .lines()
        .map(|l| l.trim_start_matches(['#', '-', '*', '>', ' ']))
        .find(|l| !l.is_empty())
        .unwrap_or("")
        .to_owned()
}

fn ensure_draft(ws: &mut Workspace, entry: &MemoryEntry) {
    if ws.memory_draft.as_ref().is_some_and(|d| d.base.key == entry.key) {
        return;
    }
    ws.memory_draft = Some(Draft::new(MemoryDraft {
        key: entry.key.clone(),
        content: entry.content.to_string(),
        tags: entry.tags.join(", "),
        files: entry.files.join("\n"),
    }));
}

pub fn commit(ws: &mut Workspace) {
    let Some(draft) = ws.memory_draft.as_ref().filter(|d| d.dirty()) else {
        return;
    };
    let (value, base) = (draft.value.clone(), draft.base.clone());
    let new_key = value.key.trim().to_owned();
    if new_key != base.key && !new_key.is_empty() {
        if ws
            .perform_json(json!({"op": "rename_memory", "key": base.key, "new_key": new_key}))
            .is_none()
        {
            if let Some(d) = ws.memory_draft.as_mut() {
                d.value.key = base.key.clone();
            }
            return;
        }
        ws.memory = Some(new_key.clone());
    }
    let key = if new_key.is_empty() { base.key.clone() } else { new_key };
    let tags: Vec<String> = value
        .tags
        .split(',')
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(str::to_owned)
        .collect();
    let files = widgets::lines_to_list(&value.files);
    let result = ws.perform_json(json!({
        "op": "upsert_memory", "key": key, "content": value.content, "tags": tags, "files": files
    }));
    if result.is_some() {
        let mut saved = value;
        saved.key = key;
        ws.memory_draft = Some(Draft::new(saved));
    }
}

type Deferred = Box<dyn FnOnce(&mut Workspace)>;

fn editor(ui: &mut Ui, ws: &mut Workspace, entry: MemoryEntry) {
    ensure_draft(ws, &entry);
    let mut changed = false;
    let mut action: Option<Deferred> = None;
    let mode_labels = [tr("Edit"), tr("Split"), tr("Preview")];

    ui.add_space(6.0);
    ui.horizontal(|ui| {
        let draft = &mut ws.memory_draft.as_mut().expect("draft").value;
        changed |= ui
            .add(egui::TextEdit::singleline(&mut draft.key).font(egui::TextStyle::Monospace).desired_width(260.0))
            .changed();
        if !entry.is_active() {
            chip(ui, entry.superseded_by.as_ref().map(|b| format!("SUPERSEDED → {b}")).unwrap_or("SUPERSEDED".into()), theme::WARNING);
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.menu_button(tr("More"), |ui| {
                if ui.button(tr("Copy as Markdown")).clicked() {
                    ui.ctx().copy_text(format!("### {}\n\n{}", entry.key, entry.content));
                    ui.close();
                }
                if entry.is_active() {
                    if ui.button(tr("Supersede with new entry")).clicked() {
                        let e = entry.clone();
                        action = Some(Box::new(move |ws| new_entry(ws, Some(&e))));
                        ui.close();
                    }
                    if ui.button(tr("Mark superseded")).clicked() {
                        let key = entry.key.clone();
                        action = Some(Box::new(move |ws| {
                            commit(ws);
                            ws.perform_json(json!({"op": "set_memory_status", "key": key, "status": "superseded"}));
                        }));
                        ui.close();
                    }
                } else if ui.button(tr("Restore as active")).clicked() {
                    let key = entry.key.clone();
                    action = Some(Box::new(move |ws| {
                        ws.perform_json(json!({"op": "set_memory_status", "key": key, "status": "active"}));
                    }));
                    ui.close();
                }
                ui.separator();
                if ui.button(tr("Delete")).clicked() {
                    let key = entry.key.clone();
                    action = Some(Box::new(move |ws| {
                        ws.memory_draft = None;
                        ws.memory = None;
                        ws.perform_json(json!({"op": "delete_memory", "key": key}));
                    }));
                    ui.close();
                }
            });
            for (i, label) in mode_labels.iter().enumerate().rev() {
                ui.selectable_value(&mut ws.memory_mode, i, *label);
            }
            let mut pinned = entry.pinned;
            if ui.checkbox(&mut pinned, tr("Pinned")).on_hover_text(tr("Pinned entries are included in every built context")).changed() {
                let key = entry.key.clone();
                action = Some(Box::new(move |ws| {
                    commit(ws);
                    ws.perform_json(json!({"op": "upsert_memory", "key": key, "content": ws.doc.memory_entry(&key).map(|m| m.content.to_string()).unwrap_or_default(), "pinned": pinned}));
                }));
            }
            let mut category = entry.category;
            egui::ComboBox::from_id_salt("memory-category")
                .selected_text(category_title(category))
                .show_ui(ui, |ui| {
                    for c in ORDER {
                        ui.selectable_value(&mut category, c, category_title(c));
                    }
                });
            if category != entry.category {
                let key = entry.key.clone();
                action = Some(Box::new(move |ws| {
                    commit(ws);
                    let content = ws.doc.memory_entry(&key).map(|m| m.content.to_string()).unwrap_or_default();
                    ws.perform_json(json!({"op": "upsert_memory", "key": key, "category": category.as_str(), "content": content}));
                }));
            }
        });
    });
    ui.separator();

    let footer_height = 150.0;
    let body_height = (ui.available_height() - footer_height).max(160.0);
    let draft = &mut ws.memory_draft.as_mut().expect("draft").value;
    let mode = ws.memory_mode;
    ui.allocate_ui(egui::vec2(ui.available_width(), body_height), |ui| {
        let edit = |ui: &mut Ui, text: &mut String| {
            egui::ScrollArea::vertical()
                .id_salt("memory-edit")
                .show(ui, |ui| {
                    ui.add_sized(
                        [ui.available_width(), body_height - 8.0],
                        egui::TextEdit::multiline(text)
                            .code_editor()
                            .hint_text(tr("Markdown. Start with [SUPERSEDED] to retire this entry.")),
                    )
                    .changed()
                })
                .inner
        };
        let preview = |ui: &mut Ui, text: &str| {
            egui::ScrollArea::vertical()
                .id_salt("memory-preview")
                .auto_shrink(false)
                .show(ui, |ui| markdown::show(ui, text));
        };
        match mode {
            0 => changed |= edit(ui, &mut draft.content),
            2 => preview(ui, &draft.content),
            _ => {
                ui.columns(2, |cols| {
                    changed |= edit(&mut cols[0], &mut draft.content);
                    preview(&mut cols[1], &draft.content);
                });
            }
        }
    });
    ui.separator();
    ui.columns(2, |cols| {
        cols[0].label(RichText::new(tr("Tags")).small().strong());
        changed |= cols[0]
            .add(egui::TextEdit::singleline(&mut draft.tags).hint_text(tr("comma, separated")))
            .changed();
        cols[1].label(RichText::new(tr("Linked files, one path per line")).small().strong());
        changed |= cols[1]
            .add(
                egui::TextEdit::multiline(&mut draft.files)
                    .code_editor()
                    .desired_rows(3)
                    .hint_text("src/payments/charge.rs"),
            )
            .changed();
    });
    let tokens = engram_core::tokens::count_tokens(&draft.content);
    ui.horizontal(|ui| {
        if let Some(s) = &entry.supersedes {
            if ui.link(trf("Supersedes {key}", &[("key", s)])).clicked() {
                let s = s.clone();
                action = Some(Box::new(move |ws| {
                    commit(ws);
                    ws.memory = Some(s);
                }));
            }
        }
        if let Some(b) = &entry.superseded_by {
            if ui.link(trf("Replaced by {key}", &[("key", b)])).clicked() {
                let b = b.clone();
                action = Some(Box::new(move |ws| {
                    commit(ws);
                    ws.memory = Some(b);
                }));
            }
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let dirty = ws.memory_draft.as_ref().is_some_and(Draft::dirty);
            muted(
                ui,
                if dirty {
                    tr("Edited").to_owned()
                } else {
                    trf(
                        "Saved {when}",
                        &[("when", &widgets::relative_time(Some(&entry.updated_at)))],
                    )
                },
            );
            widgets::tokens(ui, tokens, None);
        });
    });

    if changed {
        if let Some(d) = ws.memory_draft.as_mut() {
            d.edited_at = std::time::Instant::now();
        }
    }
    if let Some(a) = action {
        a(ws);
    }
}
