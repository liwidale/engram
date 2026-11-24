use crate::app::Action;
use crate::i18n::tr;
use crate::theme;
use crate::workspace::{Module, Workspace};
use eframe::egui::{self, Id, Key, RichText};
use std::path::PathBuf;

#[derive(Default)]
pub struct Palette {
    pub query: String,
    pub cursor: usize,
}

struct Entry {
    title: String,
    subtitle: String,
    group: &'static str,
    action: Action,
}

pub fn score(query: &str, candidate: &str) -> i32 {
    let (q, c) = (query.to_lowercase(), candidate.to_lowercase());
    if q.is_empty() {
        return 1;
    }
    if let Some(pos) = c.find(&q) {
        return 1000 - pos as i32 * 2 - (c.len() - q.len()) as i32;
    }
    let (qc, cc): (Vec<char>, Vec<char>) = (q.chars().collect(), c.chars().collect());
    let (mut qi, mut streak, mut total) = (0, 0, 0);
    for (i, ch) in cc.iter().enumerate() {
        if qi == qc.len() {
            break;
        }
        if *ch == qc[qi] {
            let boundary = i == 0 || " -_./·".contains(cc[i - 1]);
            streak += 1;
            total += 10 + streak * 5 + if boundary { 25 } else { 0 };
            qi += 1;
        } else {
            streak = 0;
        }
    }
    if qi == qc.len() {
        total
    } else {
        0
    }
}

fn catalog(ws: Option<&Workspace>, recents: &[PathBuf]) -> Vec<Entry> {
    let cmd = |title: &str, subtitle: &str, action: Action| Entry {
        title: title.into(),
        subtitle: subtitle.into(),
        group: tr("Command"),
        action,
    };
    let mut out = vec![
        cmd(tr("Open file"), "Ctrl+O", Action::Open),
        cmd(tr("New project memory"), "Ctrl+N", Action::New),
        cmd(tr("Settings"), "Ctrl+,", Action::Settings),
        cmd(tr("Keyboard shortcuts"), "Ctrl+/", Action::Shortcuts),
    ];
    let Some(ws) = ws else {
        out.extend(recents.iter().map(|p| {
            Entry {
                title: p
                    .file_name()
                    .map(|f| f.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                subtitle: p.display().to_string(),
                group: tr("Recent"),
                action: Action::OpenPath(p.clone()),
            }
        }));
        return out;
    };
    out.extend([
        cmd(tr("New memory entry"), "Ctrl+Shift+N", Action::NewMemory),
        cmd(tr("Log decision"), "Ctrl+Shift+D", Action::NewDecision),
        cmd(tr("New prompt"), "", Action::NewPrompt),
        cmd(tr("Add architecture node"), "", Action::NewNode),
        cmd(tr("Run health check"), "Ctrl+Shift+H", Action::Audit),
        cmd(tr("Connect agent"), "Ctrl+Shift+M", Action::ConnectAgent),
        cmd(tr("Project settings"), "", Action::ProjectSettings),
        cmd(tr("Scan the codebase for memory"), "", Action::Scan),
        cmd(tr("Undo"), "Ctrl+Z", Action::Undo),
        cmd(tr("Redo"), "Ctrl+Shift+Z", Action::Redo),
        cmd(tr("Copy project memory as Markdown"), "", Action::CopyMemory),
        cmd(tr("Copy agent handoff as XML"), "", Action::CopyHandoff),
        cmd(tr("Save"), "Ctrl+S", Action::Save),
        cmd(tr("Reload from disk"), "Ctrl+R", Action::Reload),
        cmd(tr("Close tab"), "Ctrl+W", Action::CloseTab),
    ]);
    for m in Module::ALL {
        out.push(Entry {
            title: m.title().into(),
            subtitle: String::new(),
            group: tr("Go to"),
            action: Action::Module(m),
        });
    }
    for m in &ws.doc.memory {
        let prefix = if m.is_active() { "" } else { "[SUPERSEDED] " };
        let first = m
            .body()
            .lines()
            .find(|l| !l.trim().is_empty())
            .unwrap_or("")
            .trim_start_matches(['#', '-', '*', ' ']);
        out.push(Entry {
            title: m.key.clone(),
            subtitle: format!("{prefix}{first}"),
            group: tr("Memory"),
            action: Action::Reveal("memory", m.key.clone()),
        });
    }
    for d in &ws.doc.decisions {
        out.push(Entry {
            title: format!("{} · {}", d.id, d.title),
            subtitle: d.status.as_str().into(),
            group: tr("Decision"),
            action: Action::Reveal("decision", d.id.clone()),
        });
    }
    for p in &ws.doc.prompts {
        out.push(Entry {
            title: p.name.clone(),
            subtitle: p.category.clone(),
            group: tr("Prompt"),
            action: Action::RevealPrompt(p.name.clone()),
        });
    }
    for n in &ws.doc.architecture.nodes {
        out.push(Entry {
            title: n.display_name().into(),
            subtitle: n.path.clone().unwrap_or_default(),
            group: tr("Node"),
            action: Action::Reveal("node", n.id.clone()),
        });
    }
    out
}

pub fn show(
    ctx: &egui::Context,
    state: &mut Palette,
    ws: Option<&Workspace>,
    recents: &[PathBuf],
) -> (bool, Option<Action>) {
    let entries = catalog(ws, recents);
    let mut ranked: Vec<(i32, Entry)> = entries
        .into_iter()
        .filter_map(|e| {
            let s = score(&state.query, &e.title).max(score(&state.query, &e.subtitle) - 20);
            (s > 0).then_some((s, e))
        })
        .collect();
    if !state.query.is_empty() {
        ranked.sort_by_key(|r| std::cmp::Reverse(r.0));
    }
    ranked.truncate(40);
    let mut close = false;
    let mut chosen: Option<usize> = None;
    ctx.input(|i| {
        if i.key_pressed(Key::ArrowDown) {
            state.cursor = (state.cursor + 1).min(ranked.len().saturating_sub(1));
        }
        if i.key_pressed(Key::ArrowUp) {
            state.cursor = state.cursor.saturating_sub(1);
        }
        if i.key_pressed(Key::Escape) {
            close = true;
        }
        if i.key_pressed(Key::Enter) && !ranked.is_empty() {
            chosen = Some(state.cursor.min(ranked.len() - 1));
        }
    });
    let modal = egui::Modal::new(Id::unique("command-palette")).show(ctx, |ui| {
        ui.set_width(600.0);
        let before = state.query.clone();
        let resp = ui.add(
            egui::TextEdit::singleline(&mut state.query)
                .hint_text(tr("Search memory, decisions, prompts and commands"))
                .desired_width(f32::INFINITY)
                .font(egui::TextStyle::Heading),
        );
        resp.request_focus();
        if state.query != before {
            state.cursor = 0;
        }
        ui.separator();
        egui::ScrollArea::vertical().max_height(380.0).show(ui, |ui| {
            if ranked.is_empty() {
                ui.label(RichText::new(tr("No matches")).color(theme::MUTED));
            }
            for (i, (_, e)) in ranked.iter().enumerate() {
                let selected = i == state.cursor;
                let r = egui::Frame::new()
                    .fill(if selected {
                        theme::ACCENT
                    } else {
                        egui::Color32::TRANSPARENT
                    })
                    .corner_radius(6)
                    .inner_margin(egui::Margin::symmetric(8, 4))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        let fg = if selected {
                            egui::Color32::WHITE
                        } else {
                            ui.visuals().strong_text_color()
                        };
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(&e.title).color(fg));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                ui.label(RichText::new(e.group).small().color(if selected {
                                    fg
                                } else {
                                    theme::MUTED
                                }));
                            });
                        });
                        if !e.subtitle.is_empty() {
                            ui.label(RichText::new(&e.subtitle).small().color(if selected {
                                fg
                            } else {
                                theme::MUTED
                            }));
                        }
                    })
                    .response
                    .interact(egui::Sense::click());
                if selected {
                    r.scroll_to_me(None);
                }
                if r.clicked() {
                    chosen = Some(i);
                }
            }
        });
    });
    if modal.should_close() {
        close = true;
    }
    match chosen {
        Some(i) => (true, Some(ranked.swap_remove(i).1.action)),
        None => (close, None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranks_substrings_above_scattered_matches() {
        assert!(score("adr", "ADR-0001 · Use Stripe") > score("adr", "Add architecture node"));
        assert!(score("cmc", "Copy MCP config") > 0);
        assert_eq!(score("zzz", "Copy MCP config"), 0);
    }
}
