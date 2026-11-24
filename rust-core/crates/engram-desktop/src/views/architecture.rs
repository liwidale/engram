use crate::i18n::{tr, trf};
use crate::settings;
use crate::theme;
use crate::widgets::{self, chip, muted};
use crate::workspace::{Dialog, Draft, NodeDraft, Workspace};
use eframe::egui::epaint::CubicBezierShape;
use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontId, Id, Pos2, Rect, RichText, Sense, Shape, Stroke, Ui, Vec2,
};
use engram_core::{ArchNode, NodeKind};
use serde_json::json;
use std::collections::{BTreeSet, HashMap};

const NODE: Vec2 = Vec2::new(184.0, 60.0);

pub fn kind_title(k: NodeKind) -> &'static str {
    match k {
        NodeKind::Module => tr("Module"),
        NodeKind::Service => tr("Service"),
        NodeKind::Database => tr("Database"),
        NodeKind::Library => tr("Library"),
        NodeKind::Queue => tr("Queue"),
        NodeKind::Ui => tr("UI"),
        NodeKind::External => tr("External"),
    }
}

pub fn new_node(ws: &mut Workspace) {
    ws.dialog = Some(Dialog::NewNode {
        id: String::new(),
        label: String::new(),
        kind: NodeKind::Module,
        path: String::new(),
        deps: BTreeSet::new(),
    });
}

pub fn show(ui: &mut Ui, ws: &mut Workspace) {
    if let Some(node) = ws.node.clone().and_then(|id| ws.doc.node(&id).cloned()) {
        egui::Panel::right("node-inspector")
            .resizable(true)
            .default_size(320.0)
            .show(ui, |ui| inspector(ui, ws, node));
    } else if ws.node_draft.is_some() {
        commit(ws);
        ws.node_draft = None;
    }
    egui::CentralPanel::default().show(ui, |ui| {
        if ws.layout.nodes.is_empty() {
            if widgets::empty_state(
                ui,
                tr("Architecture map"),
                tr("Draw the modules, services and databases of this project and what depends on what."),
                Some(tr("Agents add nodes with add_architecture_node.")),
                Some(tr("Add node")),
            ) {
                new_node(ws);
            }
            return;
        }
        toolbar(ui, ws);
        canvas(ui, ws);
    });
}

fn bounds(ws: &Workspace, ids: Option<&BTreeSet<String>>) -> Rect {
    let mut r = Rect::NOTHING;
    for n in ws.layout.nodes.iter().filter(|n| ids.is_none_or(|s| s.contains(&n.id))) {
        r = r.union(Rect::from_center_size(
            Pos2::new(n.position.x as f32, n.position.y as f32),
            NODE,
        ));
    }
    r.expand(60.0)
}

fn toolbar(ui: &mut Ui, ws: &mut Workspace) {
    ui.horizontal_wrapped(|ui| {
        let kinds: BTreeSet<&str> = ws.layout.nodes.iter().map(|n| n.kind.as_str()).collect();
        for k in NodeKind::ALL.iter().filter(|k| kinds.contains(k.as_str())) {
            chip(ui, kind_title(*k), theme::kind(*k));
        }
        muted(ui, tr("A → B means A depends on B"));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button(tr("Add node")).clicked() {
                new_node(ws);
            }
            ui.menu_button(tr("More"), |ui| {
                if ui.button(tr("Reset all to automatic layout")).clicked() {
                    ws.perform_json(json!({"op": "reset_all_node_positions"}));
                    ws.graph.scene_rect = None;
                    ui.close();
                }
                if ui.button(tr("Copy as Mermaid")).clicked() {
                    ui.ctx().copy_text(engram_core::interop::architecture_mermaid(&ws.doc));
                    ui.close();
                }
            });
            if !ws.layout.missing.is_empty() {
                let label = trf("{n} missing dependencies", &[("n", &ws.layout.missing.len())]);
                if widgets::chip_button(ui, label, theme::WARNING, false)
                    .on_hover_text(tr("Show the nodes that point at missing dependencies"))
                    .clicked()
                {
                    let ids: BTreeSet<String> = ws.layout.missing.iter().map(|m| m.from.clone()).collect();
                    focus(ws, ids);
                }
            }
            for cycle in ws.layout.cycles.clone() {
                let label = trf("Cycle: {nodes}", &[("nodes", &cycle.join(" → "))]);
                if widgets::chip_button(ui, label, theme::ERROR, false)
                    .on_hover_text(tr("Show the nodes in this cycle"))
                    .clicked()
                {
                    focus(ws, cycle.into_iter().collect());
                }
            }
        });
    });
    ui.separator();
}

fn focus(ws: &mut Workspace, ids: BTreeSet<String>) {
    ws.graph.scene_rect = Some(bounds(ws, Some(&ids)).expand(60.0));
    ws.graph.highlight = ids;
}

fn canvas(ui: &mut Ui, ws: &mut Workspace) {
    let s = settings::get();
    let mut scene_rect = ws.graph.scene_rect.unwrap_or_else(|| bounds(ws, None));
    let focus_set: BTreeSet<String> = if !ws.graph.highlight.is_empty() {
        ws.graph.highlight.clone()
    } else {
        ws.node
            .as_ref()
            .map(|id| {
                let mut set: BTreeSet<String> = ws
                    .layout
                    .edges
                    .iter()
                    .filter(|e| &e.from == id)
                    .map(|e| e.to.clone())
                    .collect();
                set.extend(ws.layout.edges.iter().filter(|e| &e.to == id).map(|e| e.from.clone()));
                set.insert(id.clone());
                set
            })
            .unwrap_or_default()
    };
    let mut clicked: Option<String> = None;
    let mut moved: Option<(String, Pos2)> = None;
    let mut link_done: Option<(String, Pos2)> = None;
    let overlay = ui.max_rect();

    egui::Scene::new()
        .zoom_range(0.2..=2.5)
        .show(ui, &mut scene_rect, |ui| {
            let positions: HashMap<&str, Pos2> = ws
                .layout
                .nodes
                .iter()
                .map(|n| {
                    let mut p = Pos2::new(n.position.x as f32, n.position.y as f32);
                    if let Some((id, delta)) = &ws.graph.dragging {
                        if id == &n.id {
                            p += *delta;
                        }
                    }
                    (n.id.as_str(), p)
                })
                .collect();
            let painter = ui.painter().clone();
            let visible = ui.clip_rect();
            if s.map_grid {
                let step = 28.0;
                let color = ui.visuals().weak_text_color().gamma_multiply(0.35);
                let mut x = (visible.left() / step).floor() * step;
                while x < visible.right() {
                    let mut y = (visible.top() / step).floor() * step;
                    while y < visible.bottom() {
                        painter.circle_filled(Pos2::new(x, y), 0.9, color);
                        y += step;
                    }
                    x += step;
                }
            }
            for e in &ws.layout.edges {
                let (Some(a), Some(b)) = (positions.get(e.from.as_str()), positions.get(e.to.as_str())) else {
                    continue;
                };
                let hot = !focus_set.is_empty() && focus_set.contains(&e.from) && focus_set.contains(&e.to);
                let dim = !focus_set.is_empty() && !hot;
                let color = if e.in_cycle {
                    theme::ERROR
                } else if hot {
                    theme::ACCENT
                } else {
                    ui.visuals()
                        .weak_text_color()
                        .gamma_multiply(if dim { 0.4 } else { 1.0 })
                };
                edge(
                    &painter,
                    *a,
                    *b,
                    Stroke::new(if hot { 2.0 } else { 1.3 }, color),
                    e.in_cycle,
                    s.map_curved,
                );
            }
            if let Some((from, to)) = &ws.graph.linking {
                if let Some(a) = positions.get(from.as_str()) {
                    let start = *a + Vec2::new(0.0, NODE.y / 2.0);
                    painter.add(Shape::dashed_line(
                        &[start, *to],
                        Stroke::new(1.5, theme::ACCENT),
                        5.0,
                        4.0,
                    ));
                }
            }
            let bg = ui.visuals().extreme_bg_color;
            let text = ui.visuals().strong_text_color();
            for n in &ws.layout.nodes {
                let center = positions[n.id.as_str()];
                let rect = Rect::from_center_size(center, NODE);
                let resp = ui.interact(rect, Id::unique(("node", &n.id)), Sense::click_and_drag());
                let color = theme::kind(n.kind);
                let selected = ws.node.as_deref() == Some(n.id.as_str());
                let alpha: f32 = if !focus_set.is_empty() && !focus_set.contains(&n.id) {
                    0.35
                } else {
                    1.0
                };
                let radius = if n.kind == NodeKind::Database { 16 } else { 10 };
                let fill = if n.kind == NodeKind::External {
                    ui.visuals().panel_fill
                } else {
                    bg
                };
                painter.rect_filled(rect, CornerRadius::same(radius), fill.gamma_multiply(alpha.max(0.85)));
                let border = if selected {
                    Stroke::new(2.0, theme::ACCENT)
                } else {
                    ui.visuals().widgets.noninteractive.bg_stroke
                };
                if n.kind == NodeKind::External && !selected {
                    let pts = vec![
                        rect.left_top(),
                        rect.right_top(),
                        rect.right_bottom(),
                        rect.left_bottom(),
                        rect.left_top(),
                    ];
                    painter.add(Shape::dashed_line(&pts, Stroke::new(1.0, theme::MUTED), 5.0, 3.0));
                } else {
                    painter.rect_stroke(rect, CornerRadius::same(radius), border, egui::StrokeKind::Inside);
                }
                let badge = Rect::from_center_size(rect.left_center() + Vec2::new(22.0, 0.0), Vec2::splat(28.0));
                painter.rect_filled(badge, CornerRadius::same(7), color.gamma_multiply(0.16 * alpha));
                painter.text(
                    badge.center(),
                    Align2::CENTER_CENTER,
                    kind_initial(n.kind),
                    FontId::proportional(13.0),
                    color.gamma_multiply(alpha),
                );
                let label_x = badge.right() + 8.0;
                let sub = if s.map_paths {
                    ws.doc
                        .node(&n.id)
                        .and_then(|d| d.path.clone())
                        .unwrap_or_else(|| kind_title(n.kind).to_owned())
                } else {
                    kind_title(n.kind).to_owned()
                };
                painter.text(
                    Pos2::new(label_x, center.y - 9.0),
                    Align2::LEFT_CENTER,
                    &n.label,
                    FontId::proportional(14.0),
                    text.gamma_multiply(alpha),
                );
                painter.text(
                    Pos2::new(label_x, center.y + 10.0),
                    Align2::LEFT_CENTER,
                    sub,
                    FontId::monospace(10.5),
                    theme::MUTED.gamma_multiply(alpha),
                );
                if ws.layout.missing.iter().any(|m| m.from == n.id) {
                    painter.circle_filled(rect.right_top() + Vec2::new(-10.0, 10.0), 4.0, theme::WARNING);
                }
                let port = Rect::from_center_size(rect.center_bottom(), Vec2::splat(12.0));
                let port_resp = ui.interact(port, Id::unique(("port", &n.id)), Sense::drag());
                if selected || resp.hovered() || port_resp.hovered() || port_resp.dragged() {
                    painter.circle(port.center(), 5.0, theme::ACCENT, Stroke::new(1.5, Color32::WHITE));
                }
                if port_resp.dragged() {
                    if let Some(pos) = port_resp.interact_pointer_pos() {
                        ws.graph.linking = Some((n.id.clone(), pos));
                    }
                }
                if port_resp.drag_stopped() {
                    if let Some((_, pos)) = ws.graph.linking.take() {
                        link_done = Some((n.id.clone(), pos));
                    }
                }
                port_resp.on_hover_text(tr("Drag onto another node to add a dependency"));
                if resp.clicked() {
                    clicked = Some(n.id.clone());
                }
                if resp.dragged() {
                    let d = ws.graph.dragging.get_or_insert_with(|| (n.id.clone(), Vec2::ZERO));
                    if d.0 == n.id {
                        d.1 += resp.drag_delta();
                    }
                }
                if resp.drag_stopped() {
                    moved = Some((n.id.clone(), center));
                }
                resp.on_hover_text(format!("{} · {}", n.id, kind_title(n.kind)));
            }
        });

    let zoom = egui::Area::new(Id::unique("map-zoom"))
        .fixed_pos(overlay.right_bottom() - Vec2::new(170.0, 40.0))
        .show(ui.ctx(), |ui| {
            egui::Frame::popup(ui.style())
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let mut factor = None;
                        if ui.small_button("−").on_hover_text(tr("Zoom out")).clicked() {
                            factor = Some(1.25);
                        }
                        if ui.small_button("+").on_hover_text(tr("Zoom in")).clicked() {
                            factor = Some(0.8);
                        }
                        let fit = ui.small_button(tr("Fit")).on_hover_text(tr("Fit to window")).clicked();
                        (factor, fit)
                    })
                    .inner
                })
                .inner
        })
        .inner;
    match zoom {
        (Some(f), _) => scene_rect = Rect::from_center_size(scene_rect.center(), scene_rect.size() * f),
        (None, true) => {
            ws.graph.highlight.clear();
            scene_rect = bounds(ws, None);
        }
        _ => {}
    }
    ws.graph.scene_rect = Some(scene_rect);
    if let Some((id, p)) = moved {
        ws.graph.dragging = None;
        ws.perform_json(json!({"op": "set_node_position", "id": id, "position": {"x": p.x, "y": p.y}}));
    }
    if let Some((from, pos)) = link_done {
        let target = ws
            .layout
            .nodes
            .iter()
            .find(|n| Rect::from_center_size(Pos2::new(n.position.x as f32, n.position.y as f32), NODE).contains(pos))
            .map(|n| n.id.clone());
        if let Some(to) = target.filter(|t| *t != from) {
            if !ws.doc.node(&from).is_some_and(|n| n.depends_on.contains(&to)) {
                ws.perform_json(json!({"op": "add_dependency", "from": from, "to": to}));
            }
        }
    }
    if let Some(sel) = clicked {
        if ws.node.as_ref() != Some(&sel) {
            commit(ws);
            ws.node_draft = None;
        }
        ws.graph.highlight.clear();
        ws.node = Some(sel);
    }
}

fn kind_initial(k: NodeKind) -> &'static str {
    match k {
        NodeKind::Module => "M",
        NodeKind::Service => "S",
        NodeKind::Database => "D",
        NodeKind::Library => "L",
        NodeKind::Queue => "Q",
        NodeKind::Ui => "U",
        NodeKind::External => "E",
    }
}

fn clip(from: Pos2, toward: Pos2) -> Pos2 {
    let d = toward - from;
    if d == Vec2::ZERO {
        return from;
    }
    let half = NODE / 2.0 + Vec2::splat(4.0);
    let t = (half.x / d.x.abs().max(1e-3)).min(half.y / d.y.abs().max(1e-3));
    from + d * t
}

fn edge(painter: &egui::Painter, a: Pos2, b: Pos2, stroke: Stroke, cycle: bool, curved: bool) {
    let (points, tail) = if curved && b.y - a.y > NODE.y {
        let start = a + Vec2::new(0.0, NODE.y / 2.0);
        let end = b - Vec2::new(0.0, NODE.y / 2.0 + 4.0);
        let bend = ((end.y - start.y) / 2.0).max(30.0);
        let c2 = end - Vec2::new(0.0, bend);
        let curve = CubicBezierShape::from_points_stroke(
            [start, start + Vec2::new(0.0, bend), c2, end],
            false,
            Color32::TRANSPARENT,
            stroke,
        );
        (curve.flatten(Some(0.5)), c2)
    } else {
        let start = clip(a, b);
        let end = clip(b, a);
        (vec![start, end], start)
    };
    if cycle {
        painter.add(Shape::dashed_line(&points, stroke, 6.0, 4.0));
    } else {
        painter.add(Shape::line(points.clone(), stroke));
    }
    let end = *points.last().expect("edge has points");
    let dir = (end - tail).normalized();
    let normal = Vec2::new(-dir.y, dir.x);
    let size = 8.0;
    painter.add(Shape::convex_polygon(
        vec![
            end,
            end - dir * size + normal * size * 0.55,
            end - dir * size - normal * size * 0.55,
        ],
        stroke.color,
        Stroke::NONE,
    ));
}

pub fn commit(ws: &mut Workspace) -> bool {
    let Some(draft) = ws.node_draft.as_mut().filter(|d| d.dirty() && d.conflict.is_none()) else {
        return false;
    };
    let v = draft.value.clone();
    draft.mark_saved();
    ws.perform_json(json!({
        "op": "add_architecture_node", "id": v.id, "label": v.label, "path": v.path, "description": v.description
    }))
    .is_some()
}

fn inspector(ui: &mut Ui, ws: &mut Workspace, node: ArchNode) {
    if ws.node_draft.as_ref().is_none_or(|d| d.base.id != node.id) {
        commit(ws);
        ws.node_draft = Some(Draft::new(NodeDraft::from(&node)));
    }
    let mut ops: Vec<serde_json::Value> = Vec::new();
    let mut changed = false;
    let mut query = ws.path_query.remove("node").unwrap_or_default();
    let hits = if query.trim().is_empty() {
        Vec::new()
    } else {
        ws.repo_files(&query)
    };
    let dirty = ws.node_draft.as_ref().is_some_and(Draft::dirty);
    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            widgets::heading(ui, node.display_name());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(tr("Close")).clicked() {
                    ws.node = None;
                }
                widgets::save_status(ui, dirty);
            });
        });
        muted(ui, &node.id);
        if ws.node_draft.as_ref().is_some_and(|d| d.conflict.is_some()) {
            match widgets::conflict_bar(ui) {
                Some(true) => ws.node_draft.as_mut().expect("draft").load_external(),
                Some(false) => ws.node_draft.as_mut().expect("draft").keep_mine(),
                None => {}
            }
        }
        ui.add_space(6.0);
        widgets::card(ui, Some(tr("Details")), |ui| {
            let mut kind = node.kind;
            egui::ComboBox::from_label(tr("Type"))
                .selected_text(kind_title(kind))
                .show_ui(ui, |ui| {
                    for k in NodeKind::ALL {
                        ui.selectable_value(&mut kind, k, kind_title(k));
                    }
                });
            if kind != node.kind {
                ops.push(json!({"op": "add_architecture_node", "id": node.id, "type": kind.as_str()}));
            }
            let draft = &mut ws.node_draft.as_mut().expect("draft").value;
            ui.label(RichText::new(tr("Label")).small());
            changed |= ui.text_edit_singleline(&mut draft.label).changed();
            ui.label(RichText::new(tr("Path in repository")).small());
            let before = draft.path.clone();
            widgets::path_field(ui, "node-path", &mut draft.path, |_| hits, "services/billing");
            changed |= draft.path != before;
            query.clone_from(&draft.path);
            ui.label(RichText::new(tr("Description")).small());
            changed |= widgets::multiline(ui, &mut draft.description, "", 3, false).changed();
        });
        widgets::card(ui, Some(tr("Depends on")), |ui| {
            let others: Vec<(String, String)> = ws
                .doc
                .architecture
                .nodes
                .iter()
                .filter(|n| n.id != node.id)
                .map(|n| (n.id.clone(), n.display_name().to_owned()))
                .collect();
            if others.is_empty() {
                muted(ui, tr("Add more nodes to connect them."));
            }
            for (id, name) in others {
                let mut on = node.depends_on.contains(&id);
                if ui.checkbox(&mut on, name).changed() {
                    let op = if on { "add_dependency" } else { "remove_dependency" };
                    ops.push(json!({"op": op, "from": node.id, "to": id}));
                }
            }
            for missing in node.depends_on.iter().filter(|d| ws.doc.node(d).is_none()) {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(trf("{id} (missing)", &[("id", missing)])).color(theme::WARNING));
                    if ui.link(tr("Remove")).clicked() {
                        ops.push(json!({"op": "remove_dependency", "from": node.id, "to": missing}));
                    }
                });
            }
        });
        widgets::card(ui, Some(tr("Used by")), |ui| {
            let users: Vec<String> = ws
                .doc
                .architecture
                .nodes
                .iter()
                .filter(|n| n.depends_on.contains(&node.id))
                .map(|n| n.id.clone())
                .collect();
            if users.is_empty() {
                muted(ui, tr("Nothing depends on this node."));
            }
            for u in users {
                if ui.link(&u).clicked() {
                    ws.node = Some(u);
                }
            }
        });
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if node.position.is_some() && ui.button(tr("Reset position")).clicked() {
                ops.push(json!({"op": "set_node_position", "id": node.id}));
            }
            if ui
                .add(egui::Button::new(RichText::new(tr("Delete node")).color(theme::ERROR)))
                .clicked()
            {
                ws.delete(
                    node.id.clone(),
                    json!({"op": "remove_architecture_node", "id": node.id}),
                );
            }
        });
    });
    ws.path_query.insert("node".into(), query);
    if changed {
        if let Some(d) = ws.node_draft.as_mut() {
            d.touch();
        }
    }
    for op in ops {
        commit(ws);
        ws.perform_json(op);
    }
}
