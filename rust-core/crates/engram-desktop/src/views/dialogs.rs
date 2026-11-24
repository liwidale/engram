use crate::i18n::{tr, trf};
use crate::theme;
use crate::views::architecture::kind_title;
use crate::views::memory::category_title;
use crate::widgets::{self, muted};
use crate::workspace::{Dialog, ProjectDraft, ToastKind, Workspace};
use eframe::egui::{self, Id, RichText};
use engram_core::client_config::{self, McpClient};
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
    let mut extra: Option<Dialog> = None;
    let modal = egui::Modal::new(Id::unique("engram-dialog")).show(ctx, |ui| {
        ui.set_width(560.0);
        body(ui, ws, &mut dialog, &mut extra);
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let (label, enabled) = submit_label(&dialog);
                if let Some(label) = label {
                    let pressed = ui.add_enabled(enabled, widgets::primary_button(label)).clicked()
                        || (enabled && ui.input(|i| i.modifiers.command && i.key_pressed(egui::Key::Enter)));
                    if pressed {
                        outcome = Outcome::Submit;
                    }
                }
                let close = if label.is_some() { tr("Cancel") } else { tr("Done") };
                if ui.button(close).clicked() {
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
    if let Some(next) = extra {
        ws.dialog = Some(next);
    }
}

fn submit_label(dialog: &Dialog) -> (Option<&'static str>, bool) {
    match dialog {
        Dialog::NewMemory {
            key,
            content,
            supersedes,
            ..
        } => (
            Some(if supersedes.is_some() {
                tr("Supersede")
            } else {
                tr("Create")
            }),
            !key.trim().is_empty() && !content.trim().is_empty(),
        ),
        Dialog::NewDecision {
            context,
            decision,
            supersedes,
            ..
        } => (
            Some(if supersedes.is_some() {
                tr("Supersede")
            } else {
                tr("Log decision")
            }),
            !context.trim().is_empty() && !decision.trim().is_empty(),
        ),
        Dialog::NewNode { id, .. } => (
            Some(tr("Add")),
            !id.trim().is_empty() && !id.contains(char::is_whitespace),
        ),
        Dialog::NewPrompt { name, .. } => (Some(tr("Create")), !name.trim().is_empty()),
        Dialog::FillPrompt { .. } => (Some(tr("Copy prompt")), true),
        Dialog::Confirm { .. } => (Some(tr("Delete")), true),
        Dialog::Review { memory, prompts, .. } => (
            Some(tr("Add to project")),
            memory.iter().any(|(on, _)| *on) || prompts.iter().any(|(on, _)| *on),
        ),
        Dialog::Project(d) => (Some(tr("Done")), !d.name.trim().is_empty()),
        Dialog::Connect { .. } | Dialog::Shortcuts => (None, false),
    }
}

fn body(ui: &mut egui::Ui, ws: &mut Workspace, dialog: &mut Dialog, extra: &mut Option<Dialog>) {
    match dialog {
        Dialog::NewMemory {
            key,
            category,
            content,
            supersedes,
            tokens,
        } => {
            widgets::heading(
                ui,
                match supersedes {
                    Some(old) => trf("Supersede {key}", &[("key", old)]),
                    None => tr("New memory entry").to_owned(),
                },
            );
            if supersedes.is_some() {
                muted(
                    ui,
                    tr("The old entry is marked superseded, linked to this one and left out of future agent context."),
                );
            }
            ui.add_space(6.0);
            ui.label(RichText::new(tr("Key")).small());
            ui.add(
                egui::TextEdit::singleline(key)
                    .hint_text("db-engine")
                    .font(egui::TextStyle::Monospace),
            );
            egui::ComboBox::from_label(tr("Category"))
                .selected_text(category_title(*category))
                .show_ui(ui, |ui| {
                    for c in MemoryCategory::ALL {
                        ui.selectable_value(category, c, category_title(c));
                    }
                });
            if widgets::multiline(ui, content, tr("Write in Markdown"), 8, true).changed() {
                *tokens = engram_core::tokens::count_tokens_cached(content);
            }
            widgets::tokens(ui, *tokens, None);
        }
        Dialog::NewDecision {
            title,
            context,
            decision,
            rejected,
            supersedes,
        } => {
            widgets::heading(
                ui,
                match supersedes {
                    Some(old) => trf("Supersede {id}", &[("id", old)]),
                    None => tr("Log decision").to_owned(),
                },
            );
            ui.add(
                egui::TextEdit::singleline(title)
                    .hint_text(tr("Title (taken from the decision if empty)"))
                    .desired_width(f32::INFINITY),
            );
            ui.label(RichText::new(tr("Context")).small());
            widgets::multiline(ui, context, tr("What problem, forces and constraints?"), 3, false);
            ui.label(RichText::new(tr("Decision")).small());
            widgets::multiline(ui, decision, tr("What you will do."), 3, false);
            ui.label(RichText::new(tr("Rejected alternatives")).small());
            for (option, reason) in rejected.iter_mut() {
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(option)
                            .hint_text(tr("Option"))
                            .desired_width(150.0),
                    );
                    ui.add(
                        egui::TextEdit::singleline(reason)
                            .hint_text(tr("Why not"))
                            .desired_width(f32::INFINITY),
                    );
                });
            }
            if ui.button(tr("Add alternative")).clicked() {
                rejected.push((String::new(), String::new()));
            }
        }
        Dialog::NewNode {
            id,
            label,
            kind,
            path,
            deps,
        } => {
            widgets::heading(ui, tr("Add architecture node"));
            ui.label(RichText::new(tr("Id")).small());
            ui.add(
                egui::TextEdit::singleline(id)
                    .hint_text("billing-api")
                    .font(egui::TextStyle::Monospace),
            );
            ui.label(RichText::new(tr("Label")).small());
            ui.add(egui::TextEdit::singleline(label).hint_text("Billing API"));
            egui::ComboBox::from_label(tr("Type"))
                .selected_text(kind_title(*kind))
                .show_ui(ui, |ui| {
                    for k in NodeKind::ALL {
                        ui.selectable_value(kind, k, kind_title(k));
                    }
                });
            ui.label(RichText::new(tr("Path")).small());
            let hits = if path.trim().is_empty() {
                Vec::new()
            } else {
                ws.repo_files(path)
            };
            widgets::path_field(ui, "new-node-path", path, |_| hits, "services/billing");
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
        Dialog::NewPrompt {
            name,
            category,
            description,
            body,
        } => {
            widgets::heading(ui, tr("New prompt"));
            ui.label(RichText::new(tr("Name")).small());
            ui.add(
                egui::TextEdit::singleline(name)
                    .hint_text("review-diff")
                    .font(egui::TextStyle::Monospace),
            );
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(category)
                        .hint_text(tr("Category"))
                        .desired_width(160.0),
                );
                ui.add(
                    egui::TextEdit::singleline(description)
                        .hint_text(tr("Short description"))
                        .desired_width(f32::INFINITY),
                );
            });
            widgets::multiline(
                ui,
                body,
                tr("Write the prompt. Use {{variable}} for placeholders."),
                10,
                true,
            );
        }
        Dialog::FillPrompt { name, values } => {
            widgets::heading(ui, trf("Fill variables for {name}", &[("name", name)]));
            for (var, value) in values.iter_mut() {
                ui.label(RichText::new(var).monospace().small());
                ui.add(
                    egui::TextEdit::multiline(value)
                        .desired_rows(1)
                        .desired_width(f32::INFINITY),
                );
            }
        }
        Dialog::Confirm { title, .. } => {
            widgets::heading(ui, trf("Delete {name}?", &[("name", title)]));
            muted(ui, tr("You can undo this with Ctrl+Z."));
        }
        Dialog::Review { title, memory, prompts } => {
            widgets::heading(ui, title.as_str());
            muted(ui, tr("Choose what to add. Entries that already exist are unchecked."));
            egui::ScrollArea::vertical().max_height(380.0).show(ui, |ui| {
                if !memory.is_empty() {
                    ui.label(RichText::new(tr("Memory")).small().strong());
                }
                for (on, c) in memory.iter_mut() {
                    ui.horizontal(|ui| {
                        ui.checkbox(on, "");
                        ui.vertical(|ui| {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(&c.key).monospace().strong());
                                widgets::chip(ui, category_title(c.category), theme::category(c.category));
                                if c.exists {
                                    widgets::chip(ui, tr("exists"), theme::MUTED);
                                }
                            });
                            muted(ui, c.content.lines().take(3).collect::<Vec<_>>().join(" "));
                        });
                    });
                }
                if !prompts.is_empty() {
                    ui.add_space(6.0);
                    ui.label(RichText::new(tr("Prompts")).small().strong());
                }
                for (on, p) in prompts.iter_mut() {
                    ui.horizontal(|ui| {
                        ui.checkbox(on, "");
                        ui.label(RichText::new(&p.name).monospace().strong());
                        muted(ui, &p.description);
                    });
                }
            });
        }
        Dialog::Project(d) => project_body(ui, ws, d, extra),
        Dialog::Connect { client, check, written } => connect_body(ui, ws, client, check, written),
        Dialog::Shortcuts => {
            widgets::heading(ui, tr("Keyboard shortcuts"));
            egui::Grid::new("shortcuts")
                .num_columns(2)
                .spacing([24.0, 6.0])
                .show(ui, |ui| {
                    for (keys, what) in [
                        ("Ctrl+K", tr("Command palette")),
                        ("Ctrl+1 – Ctrl+7", tr("Switch module")),
                        ("Ctrl+S", tr("Save pending edits")),
                        ("Ctrl+Z / Ctrl+Shift+Z", tr("Undo and redo")),
                        ("Ctrl+R", tr("Reload from disk")),
                        ("Ctrl+O / Ctrl+N", tr("Open or create a file")),
                        ("Ctrl+W", tr("Close tab")),
                        ("Ctrl+Shift+N", tr("New memory entry")),
                        ("Ctrl+Shift+D", tr("Log decision")),
                        ("Ctrl+Shift+H", tr("Run health check")),
                        ("Ctrl+Shift+M", tr("Connect agent")),
                        ("Ctrl+,", tr("Settings")),
                        ("Ctrl+/", tr("Keyboard shortcuts")),
                    ] {
                        ui.label(RichText::new(keys).monospace().color(theme::MUTED));
                        ui.label(what);
                        ui.end_row();
                    }
                });
        }
    }
}

fn project_body(ui: &mut egui::Ui, ws: &mut Workspace, d: &mut ProjectDraft, extra: &mut Option<Dialog>) {
    widgets::heading(ui, tr("Project settings"));
    egui::ScrollArea::vertical().max_height(480.0).show(ui, |ui| {
        widgets::card(ui, Some(tr("Project")), |ui| {
            ui.label(RichText::new(tr("Name")).small());
            ui.text_edit_singleline(&mut d.name);
            ui.label(RichText::new(tr("Description")).small());
            widgets::multiline(
                ui,
                &mut d.description,
                tr("What this project is, in a sentence or two"),
                2,
                false,
            );
        });
        widgets::card(ui, Some(tr("Codebase")), |ui| {
            let base = ws.store.base_dir();
            let shown = if d.repo_root.is_empty() {
                base.display().to_string()
            } else {
                d.repo_root.clone()
            };
            ui.label(RichText::new(shown).monospace().small());
            ui.horizontal(|ui| {
                if ui.button(tr("Choose folder")).clicked() {
                    if let Some(dir) = rfd::FileDialog::new()
                        .set_directory(ws.resolved_repo_root())
                        .set_title(tr("Choose the codebase this project memory describes"))
                        .pick_folder()
                    {
                        d.repo_root = crate::views::health::relative_root(&base, &dir);
                    }
                }
                if !d.repo_root.is_empty() && ui.button(tr("Use .engram folder")).clicked() {
                    d.repo_root.clear();
                }
            });
        });
        widgets::card(ui, Some(tr("Context health")), |ui| {
            ui.checkbox(&mut d.custom_days, tr("Custom stale handoff threshold"));
            if d.custom_days {
                ui.add(egui::Slider::new(&mut d.days, 1..=90).text(tr("days")));
            }
            ui.label(RichText::new(tr("Ignored folders")).small());
            let mut remove = None;
            for (i, dir) in d.ignored_dirs.iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(dir).monospace());
                    if ui.small_button(tr("Remove")).clicked() {
                        remove = Some(i);
                    }
                });
            }
            if let Some(i) = remove {
                d.ignored_dirs.remove(i);
            }
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut d.new_dir)
                        .hint_text("generated")
                        .font(egui::TextStyle::Monospace),
                );
                let dir = d.new_dir.trim().trim_matches('/').to_owned();
                if ui.add_enabled(!dir.is_empty(), egui::Button::new(tr("Add"))).clicked()
                    && !d.ignored_dirs.contains(&dir)
                {
                    d.ignored_dirs.push(dir);
                    d.new_dir.clear();
                }
            });
            ui.label(RichText::new(tr("Ignored issues")).small());
            if d.ignored_issues.is_empty() {
                muted(ui, tr("Issues you ignore in Context health appear here."));
            }
            let mut unignore = None;
            for (i, issue) in d.ignored_issues.iter().enumerate() {
                let text = ws
                    .health
                    .as_ref()
                    .and_then(|r| r.issues.iter().find(|x| &x.id == issue).map(|x| x.message.clone()))
                    .unwrap_or_else(|| issue.clone());
                ui.horizontal(|ui| {
                    ui.label(text);
                    if ui.small_button(tr("Stop ignoring")).clicked() {
                        unignore = Some(i);
                    }
                });
            }
            if let Some(i) = unignore {
                d.ignored_issues.remove(i);
            }
        });
        widgets::card(ui, Some(tr("Context and MCP")), |ui| {
            ui.checkbox(&mut d.custom_score, tr("Custom relevance threshold"));
            if d.custom_score {
                ui.add(
                    egui::Slider::new(&mut d.min_score, 0.0..=1.0)
                        .step_by(0.01)
                        .text(tr("minimum score")),
                );
            }
            ui.label(RichText::new(tr("MCP server name")).small());
            ui.add(
                egui::TextEdit::singleline(&mut d.server_name)
                    .hint_text(client_config::server_name(&d.name))
                    .font(egui::TextStyle::Monospace),
            );
        });
        widgets::card(ui, Some(tr("Import and export")), |ui| {
            if ui
                .button(tr("Import from CLAUDE.md, AGENTS.md or .cursorrules"))
                .clicked()
            {
                if let Some(path) = rfd::FileDialog::new()
                    .set_directory(ws.resolved_repo_root())
                    .pick_file()
                {
                    match engram_core::interop::import_file(&path, &ws.doc) {
                        Ok(memory) => {
                            let name = path
                                .file_name()
                                .map(|f| f.to_string_lossy().into_owned())
                                .unwrap_or_default();
                            *extra = Some(Dialog::Review {
                                title: trf("Import from {file}", &[("file", &name)]),
                                memory: memory.into_iter().map(|m| (!m.exists, m)).collect(),
                                prompts: Vec::new(),
                            });
                        }
                        Err(e) => ws.toast(ToastKind::Error, e.to_string()),
                    }
                }
            }
            let file = ws
                .path()
                .file_name()
                .map(|f| f.to_string_lossy().into_owned())
                .unwrap_or_default();
            let pointer =
                engram_core::interop::agents_pointer(&ws.doc, &file, &client_config::server_name_for(&ws.doc));
            let mermaid = engram_core::interop::architecture_mermaid(&ws.doc);
            for (label, text, name) in [
                (tr("Export AGENTS.md snippet"), pointer, "AGENTS.md"),
                (tr("Export architecture as Mermaid"), mermaid, "architecture.mmd"),
            ] {
                ui.horizontal(|ui| {
                    ui.label(label);
                    if ui.button(tr("Copy")).clicked() {
                        ui.ctx().copy_text(text.clone());
                        ws.toast(ToastKind::Success, trf("Copied {name}", &[("name", &name)]));
                    }
                    if ui.button(tr("Save")).clicked() {
                        if let Some(path) = rfd::FileDialog::new()
                            .set_directory(ws.resolved_repo_root())
                            .set_file_name(name)
                            .save_file()
                        {
                            match std::fs::write(&path, &text) {
                                Ok(()) => {
                                    ws.toast(ToastKind::Success, trf("Saved {name}", &[("name", &path.display())]))
                                }
                                Err(e) => ws.toast(ToastKind::Error, e.to_string()),
                            }
                        }
                    }
                });
            }
        });
    });
}

fn connect_body(
    ui: &mut egui::Ui,
    ws: &mut Workspace,
    client: &mut McpClient,
    check: &mut Option<Result<String, String>>,
    written: &mut Option<String>,
) {
    widgets::heading(ui, tr("Connect an agent"));
    muted(
        ui,
        tr("Give an AI agent read and write access to this project memory through engram-mcp."),
    );
    ui.add_space(6.0);
    ui.horizontal_wrapped(|ui| {
        for c in McpClient::ALL {
            let title = if c == McpClient::Generic {
                tr("Custom JSON")
            } else {
                c.title()
            };
            if ui.selectable_label(*client == c, title).clicked() {
                *client = c;
                *written = None;
            }
        }
    });
    let configs = ws.client_configs();
    let Some(config) = configs.iter().find(|c| c.client == *client) else {
        return;
    };
    let snippet = |ui: &mut egui::Ui, text: &str| {
        egui::Frame::new()
            .fill(ui.visuals().faint_bg_color)
            .corner_radius(6)
            .inner_margin(8)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.add(egui::Label::new(RichText::new(text).monospace().small()).selectable(true));
            });
    };
    if let Some(command) = &config.command {
        ui.label(RichText::new(tr("Terminal command")).small().strong());
        snippet(ui, command);
    }
    ui.horizontal(|ui| {
        ui.label(RichText::new(tr("Config")).small().strong());
        if let Some(t) = &config.target {
            muted(ui, t);
        }
    });
    snippet(ui, &config.snippet);
    if let Some(path) = written {
        ui.label(RichText::new(trf("Updated {path}", &[("path", path)])).color(theme::SUCCESS));
    }
    match check {
        Some(Ok(v)) => {
            drop(ui.label(RichText::new(trf("Server works: {version}", &[("version", v)])).color(theme::SUCCESS)))
        }
        Some(Err(e)) => drop(ui.label(RichText::new(e.as_str()).color(theme::ERROR))),
        None => {}
    }
    ui.horizontal(|ui| {
        if ui.add(widgets::primary_button(tr("Copy"))).clicked() {
            ui.ctx()
                .copy_text(config.command.clone().unwrap_or_else(|| config.snippet.clone()));
            ws.toast(
                ToastKind::Success,
                trf("Copied the {client} config", &[("client", &config.title)]),
            );
        }
        if config.target.is_some() && ui.button(tr("Write to config file")).clicked() {
            match client_config::write_client_config(config) {
                Ok(outcome) => *written = Some(outcome.path),
                Err(e) => ws.toast(ToastKind::Error, e.to_string()),
            }
        }
        if ui.button(tr("Verify server")).clicked() {
            *check = Some(verify(&Workspace::mcp_binary()));
        }
    });
}

pub fn verify(binary: &std::path::Path) -> Result<String, String> {
    let out = std::process::Command::new(binary)
        .arg("--version")
        .output()
        .map_err(|e| {
            trf(
                "Could not start {path}: {error}",
                &[("path", &binary.display()), ("error", &e)],
            )
        })?;
    let text = String::from_utf8_lossy(&out.stdout).trim().to_owned();
    if out.status.success() && text.starts_with("engram-mcp") {
        Ok(text)
    } else {
        Err(tr("This binary is not engram-mcp.").to_owned())
    }
}

fn submit(ctx: &egui::Context, ws: &mut Workspace, dialog: &Dialog) -> bool {
    match dialog {
        Dialog::NewMemory {
            key,
            category,
            content,
            supersedes,
            ..
        } => {
            let key = key.trim().to_owned();
            if supersedes.is_none() && ws.doc.memory_entry(&key).is_some() {
                ws.toast(
                    ToastKind::Error,
                    trf(
                        "An entry named {key} already exists. Choose another key.",
                        &[("key", &key)],
                    ),
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
            ws.node_draft = None;
            ws.graph.scene_rect = None;
            true
        }
        Dialog::NewPrompt {
            name,
            category,
            description,
            body,
        } => {
            let name = name.trim().to_owned();
            if ws.doc.prompt(&name).is_some() {
                ws.toast(
                    ToastKind::Error,
                    trf(
                        "A prompt named {name} already exists. Choose another name.",
                        &[("name", &name)],
                    ),
                );
                return false;
            }
            let op = json!({"op": "upsert_prompt", "name": name, "body": body, "category": category, "description": description});
            if ws.perform_json(op).is_none() {
                return false;
            }
            ws.prompt = Some(name);
            ws.prompt_draft = None;
            true
        }
        Dialog::FillPrompt { name, values } => {
            crate::views::prompts::copy_rendered(ctx, ws, name, values);
            true
        }
        Dialog::Confirm { title, op } => {
            ws.delete_now(title, op.clone());
            true
        }
        Dialog::Review { memory, prompts, .. } => {
            let memory: Vec<_> = memory.iter().filter(|(on, _)| *on).map(|(_, c)| c.clone()).collect();
            let prompts: Vec<_> = prompts.iter().filter(|(on, _)| *on).map(|(_, c)| c.clone()).collect();
            let count = memory.len() + prompts.len();
            let op = engram_core::interop::apply_op(&memory, &prompts);
            if ws.perform(op, engram_core::Op::label("batch")).is_none() {
                return false;
            }
            ws.toast(ToastKind::Success, trf("Added {n} items", &[("n", &count)]));
            ws.run_audit(ctx);
            true
        }
        Dialog::Project(d) => {
            let op = json!({"op": "batch", "ops": [
                {"op": "update_project", "name": d.name, "description": d.description, "repo_root": d.repo_root},
                {"op": "update_project",
                 "stale_handoff_days": if d.custom_days { Some(d.days) } else { None },
                 "context_min_score": if d.custom_score { Some(d.min_score) } else { None },
                 "mcp_server_name": if d.server_name.trim().is_empty() { None } else { Some(d.server_name.trim()) },
                 "health_ignore_dirs": d.ignored_dirs,
                 "health_ignore_issues": d.ignored_issues}
            ]});
            if ws.perform_json(op).is_none() {
                return false;
            }
            ws.toast(ToastKind::Success, tr("Project settings saved"));
            ws.run_audit(ctx);
            true
        }
        Dialog::Connect { .. } | Dialog::Shortcuts => true,
    }
}
