use crate::i18n::{tr, trf};
use crate::theme;
use crate::widgets::{self, chip, muted};
use crate::workspace::{ToastKind, Workspace};
use eframe::egui::{self, text::LayoutJob, FontId, RichText, TextFormat, Ui};
use engram_core::context::{build_context, ContextRequest, ItemKind, Reason, Selection};
use std::time::{Duration, Instant};

fn kind_key(k: ItemKind) -> &'static str {
    match k {
        ItemKind::Memory => "memory",
        ItemKind::Decision => "decision",
        ItemKind::Node => "node",
    }
}

fn selection(set: &std::collections::BTreeSet<String>) -> Selection {
    let pick = |kind: &str| {
        set.iter()
            .filter_map(|s| s.strip_prefix(&format!("{kind}:")).map(str::to_owned))
            .collect()
    };
    Selection {
        memory: pick("memory"),
        decisions: pick("decision"),
        nodes: pick("node"),
    }
}

pub fn rebuild_if_needed(ws: &mut Workspace) {
    let c = &mut ws.context;
    if !c.stale || c.changed_at.elapsed() < Duration::from_millis(200) {
        return;
    }
    let req = ContextRequest {
        task: c.task.clone(),
        max_tokens: c.max_tokens,
        include: selection(&c.forced),
        exclude: selection(&c.excluded),
        prompt: c.prompt.clone(),
        prompt_vars: c
            .vars
            .iter()
            .filter(|(_, v)| !v.is_empty())
            .map(|(k, v)| (k.clone(), v.clone().into()))
            .collect(),
        include_handoff: c.include_handoff,
        auto_rank: c.auto_rank,
        min_score: None,
        format: engram_core::context::ContextFormat::Xml,
    };
    c.bundle = Some(build_context(&ws.doc, &req));
    c.stale = false;
}

pub fn show(ui: &mut Ui, ws: &mut Workspace) {
    rebuild_if_needed(ws);
    let mut touched = false;
    egui::Panel::left("context-controls")
        .resizable(true)
        .default_size(420.0)
        .show(ui, |ui| {
            egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                ui.add_space(6.0);
                let c = &mut ws.context;
                widgets::card(ui, Some(tr("Task")), |ui| {
                    touched |=
                        widgets::multiline(ui, &mut c.task, tr("Describe what the agent is about to do"), 3, false)
                            .changed();
                });
                widgets::card(ui, Some(tr("Budget")), |ui| {
                    touched |= ui
                        .add(
                            egui::Slider::new(&mut c.max_tokens, 500..=32_000)
                                .step_by(250.0)
                                .suffix(format!(" {}", tr("tokens"))),
                        )
                        .changed();
                    ui.horizontal(|ui| {
                        for n in [2000, 4000, 8000, 16_000, 32_000] {
                            if ui
                                .selectable_label(c.max_tokens == n, format!("{}k", n / 1000))
                                .clicked()
                            {
                                c.max_tokens = n;
                                touched = true;
                            }
                        }
                    });
                    touched |= ui.checkbox(&mut c.auto_rank, tr("Rank against the task")).changed();
                    touched |= ui
                        .checkbox(&mut c.include_handoff, tr("Include agent handoff"))
                        .changed();
                });
                widgets::card(ui, Some(tr("Prompt")), |ui| {
                    let current = c.prompt.clone().unwrap_or_default();
                    egui::ComboBox::from_id_salt("context-prompt")
                        .selected_text(if current.is_empty() {
                            tr("None").to_owned()
                        } else {
                            current.clone()
                        })
                        .show_ui(ui, |ui| {
                            if ui.selectable_label(current.is_empty(), tr("None")).clicked() {
                                c.prompt = None;
                                touched = true;
                            }
                            for p in &ws.doc.prompts {
                                if ui.selectable_label(current == p.name, &p.name).clicked() {
                                    c.prompt = Some(p.name.clone());
                                    touched = true;
                                }
                            }
                        });
                    if let Some(p) = c.prompt.as_deref().and_then(|n| ws.doc.prompt(n)) {
                        for v in p.variables() {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(&v).monospace().small());
                                touched |= ui.text_edit_singleline(c.vars.entry(v).or_default()).changed();
                            });
                        }
                    }
                    if let Some(missing) = c
                        .bundle
                        .as_ref()
                        .and_then(|b| b.prompt.as_ref())
                        .map(|p| p.missing.clone())
                        .filter(|m| !m.is_empty())
                    {
                        ui.label(
                            RichText::new(trf("Unfilled: {list}", &[("list", &missing.join(", "))]))
                                .small()
                                .color(theme::WARNING),
                        );
                    }
                });
                widgets::card(ui, Some(tr("Candidates")), |ui| {
                    let Some(items) = c.bundle.as_ref().map(|b| b.items.clone()) else {
                        return;
                    };
                    if items.is_empty() {
                        muted(
                            ui,
                            tr("Add memory, decisions or architecture nodes to build context from."),
                        );
                    }
                    if (!c.forced.is_empty() || !c.excluded.is_empty()) && ui.link(tr("Reset selection")).clicked() {
                        c.forced.clear();
                        c.excluded.clear();
                        touched = true;
                    }
                    for item in items {
                        let key = format!("{}:{}", kind_key(item.kind), item.id);
                        ui.horizontal(|ui| {
                            let mut on = item.included;
                            if ui.checkbox(&mut on, "").changed() {
                                c.forced.remove(&key);
                                c.excluded.remove(&key);
                                if on {
                                    c.forced.insert(key.clone());
                                } else {
                                    c.excluded.insert(key.clone());
                                }
                                touched = true;
                            }
                            ui.vertical(|ui| {
                                ui.horizontal(|ui| {
                                    let id = RichText::new(&item.id).monospace().small();
                                    ui.label(if item.included {
                                        id.strong()
                                    } else {
                                        id.color(theme::MUTED)
                                    });
                                    match item.reason {
                                        Reason::Pinned => drop(chip(ui, tr("pinned"), theme::ACCENT)),
                                        Reason::Selected => drop(chip(ui, tr("forced"), theme::ACCENT)),
                                        Reason::OverBudget => drop(chip(ui, tr("over budget"), theme::WARNING)),
                                        Reason::Excluded => drop(chip(ui, tr("excluded"), theme::MUTED)),
                                        Reason::Ranked | Reason::BelowThreshold => {}
                                    }
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        muted(ui, format!("{} · {:.2}", item.tokens, item.score));
                                    });
                                });
                                muted(ui, &item.title);
                                if !item.matched.is_empty() {
                                    ui.label(RichText::new(item.matched.join(" · ")).small().color(theme::ACCENT));
                                }
                            });
                        });
                    }
                });
            });
        });
    egui::CentralPanel::default().show(ui, |ui| preview(ui, ws));
    if touched {
        ws.context.stale = true;
        ws.context.changed_at = Instant::now();
    }
}

fn preview(ui: &mut Ui, ws: &mut Workspace) {
    let Some(bundle) = ws.context.bundle.clone() else {
        return;
    };
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        widgets::tokens(ui, bundle.tokens, Some(bundle.max_tokens));
        let ratio = (bundle.tokens as f32 / bundle.max_tokens.max(1) as f32).min(1.0);
        let color = if bundle.over_budget {
            theme::ERROR
        } else if ratio > 0.85 {
            theme::WARNING
        } else {
            theme::ACCENT
        };
        ui.add(egui::ProgressBar::new(ratio).desired_width(200.0).fill(color));
        if bundle.dead_ends > 0 {
            chip(ui, trf("{n} dead ends", &[("n", &bundle.dead_ends)]), theme::ERROR).on_hover_text(tr(
                "Rejected alternatives from decisions outside the bundle, listed so agents do not propose them again",
            ));
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.add(widgets::primary_button(tr("Copy as XML context"))).clicked() {
                ui.ctx().copy_text(bundle.output.clone());
                ws.toast(
                    ToastKind::Success,
                    trf("Context copied, {n} tokens", &[("n", &bundle.tokens)]),
                );
            }
        });
    });
    ui.separator();
    egui::ScrollArea::both().auto_shrink(false).show(ui, |ui| {
        ui.label(highlight(ui, &bundle.output));
    });
}

fn highlight(ui: &Ui, xml: &str) -> LayoutJob {
    let mut job = LayoutJob::default();
    let font = FontId::monospace(12.5);
    let text = TextFormat {
        font_id: font.clone(),
        color: ui.visuals().text_color(),
        ..Default::default()
    };
    let tag = TextFormat {
        font_id: font,
        color: theme::ACCENT,
        ..Default::default()
    };
    let mut rest = xml;
    while let Some(open) = rest.find('<') {
        job.append(&rest[..open], 0.0, text.clone());
        let Some(close) = rest[open..].find('>') else { break };
        job.append(&rest[open..=open + close], 0.0, tag.clone());
        rest = &rest[open + close + 1..];
    }
    job.append(rest, 0.0, text);
    job
}
