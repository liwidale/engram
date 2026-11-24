use crate::i18n::{tr, trf};
use crate::markdown;
use crate::settings::{self, EditorMode};
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
    egui::CentralPanel::default().show(ui, |ui| {
        match ws.memory.clone().and_then(|k| ws.doc.memory_entry(&k).cloned()) {
            Some(entry) => editor(ui, ws, entry),
            None if ws.is_empty() => onboarding(ui, ws),
            None => {
                if widgets::empty_state(
                    ui,
                    tr("Project memory"),
                    tr("The rules, stack and conventions an agent should never forget. Select an entry, or add one."),
                    Some(tr("Agents read this through engram://memory.")),
                    Some(tr("New entry")),
                ) {
                    new_entry(ws, None);
                }
            }
        }
    });
}

fn onboarding(ui: &mut Ui, ws: &mut Workspace) {
    ui.vertical_centered(|ui| {
        ui.add_space((ui.available_height() * 0.2).max(24.0));
        widgets::heading(ui, tr("Start this project's memory"));
        ui.set_max_width(460.0);
        ui.label(
            RichText::new(tr("Engram can read your build files and agent instructions and draft the first entries. You review everything before it is added."))
                .color(theme::MUTED),
        );
        ui.add_space(14.0);
        ui.horizontal(|ui| {
            ui.add_space((ui.available_width() - 330.0).max(0.0) / 2.0);
            if ui.add_sized([160.0, 32.0], widgets::primary_button(tr("Scan repository"))).clicked() {
                scan(ws);
            }
            if ui.add_sized([160.0, 32.0], egui::Button::new(tr("Start blank"))).clicked() {
                new_entry(ws, None);
            }
        });
        ui.add_space(8.0);
        muted(ui, ws.resolved_repo_root().display().to_string());
    });
}

pub fn scan(ws: &mut Workspace) {
    let b = engram_core::interop::bootstrap(&ws.resolved_repo_root(), &ws.doc);
    if b.memory.is_empty() && b.prompts.is_empty() {
        ws.toast(
            crate::workspace::ToastKind::Info,
            tr("No build files or agent instructions found in this folder."),
        );
        return;
    }
    ws.dialog = Some(Dialog::Review {
        title: tr("Review the draft").to_owned(),
        memory: b.memory.into_iter().map(|m| (!m.exists, m)).collect(),
        prompts: b.prompts.into_iter().map(|p| (!p.exists, p)).collect(),
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
                tokens: 0,
            }
        }
        None => Dialog::NewMemory {
            key: String::new(),
            category: MemoryCategory::Rule,
            content: String::new(),
            supersedes: None,
            tokens: 0,
        },
    });
}

fn list(ui: &mut Ui, ws: &mut Workspace) {
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(&mut ws.memory_filter)
                .hint_text(tr("Filter by key, text or tag"))
                .desired_width(ui.available_width() - 70.0),
        );
        if ui.button(tr("New")).on_hover_text(tr("New memory entry")).clicked() {
            new_entry(ws, None);
        }
    });
    ui.horizontal_wrapped(|ui| {
        if widgets::chip_button(ui, tr("All"), theme::ACCENT, ws.category_filter.is_none()).clicked() {
            ws.category_filter = None;
        }
        for c in ORDER {
            if widgets::chip_button(ui, category_title(c), theme::category(c), ws.category_filter == Some(c)).clicked()
            {
                ws.category_filter = if ws.category_filter == Some(c) { None } else { Some(c) };
            }
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
        .filter(|e| ws.category_filter.is_none_or(|c| e.category == c))
        .filter(|e| {
            q.is_empty()
                || e.key.to_lowercase().contains(&q)
                || e.content.to_lowercase().contains(&q)
                || e.tags.iter().any(|t| t.to_lowercase().contains(&q))
        })
        .cloned()
        .collect();
    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        if entries.is_empty() && !ws.doc.memory.is_empty() {
            muted(ui, tr("No entries match"));
        }
        for cat in ORDER {
            let mut group: Vec<&MemoryEntry> = entries.iter().filter(|e| e.category == cat).collect();
            if group.is_empty() {
                continue;
            }
            group.sort_by_key(|e| (!e.is_active(), !e.pinned, e.key.clone()));
            ui.add_space(4.0);
            ui.label(RichText::new(category_title(cat)).small().strong().color(theme::category(cat)));
            for e in group {
                let selected = ws.memory.as_deref() == Some(&e.key);
                let mut title = RichText::new(&e.key).monospace();
                if !e.is_active() {
                    title = title.strikethrough().color(theme::MUTED);
                }
                let resp = ui
                    .horizontal(|ui| {
                        let r = ui.selectable_label(selected, title);
                        if e.pinned {
                            ui.label(RichText::new(tr("pinned")).small().color(theme::PINNED));
                        }
                        r
                    })
                    .inner
                    .on_hover_text(summary(e));
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
                    let pin = if e.pinned { tr("Unpin") } else { tr("Pin to every context") };
                    if ui.button(pin).clicked() {
                        ws.perform_json(json!({"op": "upsert_memory", "key": e.key, "content": e.content.as_str(), "pinned": !e.pinned}));
                        ui.close();
                    }
                    if e.is_active() && ui.button(tr("Supersede with new entry")).clicked() {
                        new_entry(ws, Some(e));
                        ui.close();
                    }
                    if ui.button(tr("Delete")).clicked() {
                        ws.delete(e.key.clone(), json!({"op": "delete_memory", "key": e.key}));
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

pub fn commit(ws: &mut Workspace) -> bool {
    let Some(draft) = ws.memory_draft.as_ref().filter(|d| d.dirty() && d.conflict.is_none()) else {
        return false;
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
            return false;
        }
        ws.memory = Some(new_key.clone());
        if let Some(d) = ws.memory_draft.as_mut() {
            d.base.key = new_key.clone();
        }
    }
    let key = if new_key.is_empty() { base.key.clone() } else { new_key };
    let tags: Vec<String> = value
        .tags
        .split(',')
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(str::to_owned)
        .collect();
    if let Some(d) = ws.memory_draft.as_mut() {
        d.value.key = key.clone();
        d.mark_saved();
    }
    ws.perform_json(json!({
        "op": "upsert_memory", "key": key, "content": value.content, "tags": tags, "files": value.files
    }))
    .is_some()
}

type Deferred = Box<dyn FnOnce(&mut Workspace)>;

fn editor(ui: &mut Ui, ws: &mut Workspace, entry: MemoryEntry) {
    if ws.memory_draft.as_ref().is_none_or(|d| d.base.key != entry.key) {
        ws.memory_draft = Some(Draft::new(MemoryDraft::from(&entry)));
    }
    let mut action: Option<Deferred> = None;
    let mut mode = settings::get().memory_mode;

    ui.add_space(6.0);
    ui.horizontal(|ui| {
        let draft = &mut ws.memory_draft.as_mut().expect("draft");
        if ui
            .add(
                egui::TextEdit::singleline(&mut draft.value.key)
                    .font(egui::TextStyle::Monospace)
                    .desired_width(240.0),
            )
            .changed()
        {
            draft.touch();
        }
        if !entry.is_active() {
            let text = entry
                .superseded_by
                .as_ref()
                .map(|b| format!("{} → {b}", tr("Superseded")))
                .unwrap_or_else(|| tr("Superseded").into());
            chip(ui, text, theme::SUPERSEDED);
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
                    if ui.button(tr("Retire as superseded")).clicked() {
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
                        ws.delete(key.clone(), json!({"op": "delete_memory", "key": key}))
                    }));
                    ui.close();
                }
            });
            for (m, label) in [
                (EditorMode::Preview, tr("Preview")),
                (EditorMode::Split, tr("Split")),
                (EditorMode::Edit, tr("Edit")),
            ] {
                if ui.selectable_label(mode == m, label).clicked() {
                    mode = m;
                    let mut s = settings::get();
                    s.memory_mode = m;
                    settings::set(s);
                }
            }
            let mut pinned = entry.pinned;
            if ui
                .checkbox(&mut pinned, tr("Pinned"))
                .on_hover_text(tr("Pinned entries are included in every built context"))
                .changed()
            {
                let key = entry.key.clone();
                action = Some(Box::new(move |ws| {
                    commit(ws);
                    let content = ws
                        .doc
                        .memory_entry(&key)
                        .map(|m| m.content.to_string())
                        .unwrap_or_default();
                    ws.perform_json(json!({"op": "upsert_memory", "key": key, "content": content, "pinned": pinned}));
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
                    let content = ws
                        .doc
                        .memory_entry(&key)
                        .map(|m| m.content.to_string())
                        .unwrap_or_default();
                    ws.perform_json(
                        json!({"op": "upsert_memory", "key": key, "category": category.as_str(), "content": content}),
                    );
                }));
            }
        });
    });
    if ws.memory_draft.as_ref().is_some_and(|d| d.conflict.is_some()) {
        match widgets::conflict_bar(ui) {
            Some(true) => ws.memory_draft.as_mut().expect("draft").load_external(),
            Some(false) => ws.memory_draft.as_mut().expect("draft").keep_mine(),
            None => {}
        }
    }
    ui.separator();

    let footer = 120.0;
    let body_height = (ui.available_height() - footer).max(160.0);
    let wide = ui.available_width() >= 520.0;
    let effective = if mode == EditorMode::Split && !wide {
        EditorMode::Edit
    } else {
        mode
    };
    let mut changed = false;
    {
        let draft = ws.memory_draft.as_mut().expect("draft");
        ui.allocate_ui(egui::vec2(ui.available_width(), body_height), |ui| {
            let edit = |ui: &mut Ui, text: &mut String| {
                egui::ScrollArea::vertical()
                    .id_salt("memory-edit")
                    .show(ui, |ui| {
                        widgets::code_edit(ui, text, tr("Write in Markdown"), body_height - 8.0).changed()
                    })
                    .inner
            };
            let preview = |ui: &mut Ui, text: &str| {
                egui::ScrollArea::vertical()
                    .id_salt("memory-preview")
                    .auto_shrink(false)
                    .show(ui, |ui| markdown::show(ui, text));
            };
            match effective {
                EditorMode::Edit => changed |= edit(ui, &mut draft.value.content),
                EditorMode::Preview => preview(ui, &draft.value.content),
                EditorMode::Split => {
                    ui.columns(2, |cols| {
                        changed |= edit(&mut cols[0], &mut draft.value.content);
                        preview(&mut cols[1], &draft.value.content);
                    });
                }
            }
        });
    }
    ui.separator();
    let detail_label = {
        let d = &ws.memory_draft.as_ref().expect("draft").value;
        let tags = d.tags.split(',').filter(|t| !t.trim().is_empty()).count();
        if tags == 0 && d.files.is_empty() {
            tr("Tags and linked files").to_owned()
        } else {
            trf(
                "{tags} tags · {files} linked files",
                &[("tags", &tags), ("files", &d.files.len())],
            )
        }
    };
    let open_default = !entry.tags.is_empty() || !entry.files.is_empty();
    egui::CollapsingHeader::new(RichText::new(detail_label).small())
        .id_salt(("memory-details", &entry.key))
        .default_open(open_default)
        .show(ui, |ui| {
            let mut query = ws.path_query.remove("memory").unwrap_or_default();
            let hits = if query.trim().is_empty() {
                Vec::new()
            } else {
                ws.repo_files(&query)
            };
            ui.columns(2, |cols| {
                cols[0].label(RichText::new(tr("Tags")).small().strong());
                let d = ws.memory_draft.as_mut().expect("draft");
                changed |= cols[0]
                    .add(egui::TextEdit::singleline(&mut d.value.tags).hint_text(tr("comma, separated")))
                    .changed();
                cols[1].label(
                    RichText::new(tr("Linked files, checked by Context health"))
                        .small()
                        .strong(),
                );
                changed |= widgets::file_list(&mut cols[1], "memory-files", &mut d.value.files, &mut query, |_| hits);
            });
            ws.path_query.insert("memory".into(), query);
        });

    let draft = ws.memory_draft.as_mut().expect("draft");
    if changed {
        draft.touch();
    }
    draft.refresh_tokens(&draft.value.content.clone());
    let (tokens, dirty) = (draft.tokens, draft.dirty());
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
            widgets::save_status(ui, dirty);
            widgets::tokens(ui, tokens, None);
        });
    });
    if let Some(a) = action {
        a(ws);
    }
}
