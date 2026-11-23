use crate::i18n::{tr, trf};
use crate::theme;
use crate::widgets::{self, chip, muted};
use crate::workspace::{Dialog, Workspace};
use eframe::egui::{self, Align2, CornerRadius, FontId, Id, Pos2, Rect, RichText, Sense, Shape, Stroke, Ui, Vec2};
use engram_core::{ArchNode, NodeKind};
use serde_json::json;
use std::collections::BTreeSet;

const NODE: Vec2 = Vec2::new(176.0, 58.0);

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
    }
    egui::CentralPanel::default().show(ui, |ui| {
        if ws.layout.nodes.is_empty() {
            if widgets::empty_state(
                ui,
                tr("Architecture map"),
                tr("Model modules, services and databases with directional dependencies. Agents add nodes with add_architecture_node."),
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

fn toolbar(ui: &mut Ui, ws: &mut Workspace) {
    ui.horizontal(|ui| {
        let kinds: BTreeSet<&str> = ws.layout.nodes.iter().map(|n| n.kind.as_str()).collect();
        for k in NodeKind::ALL.iter().filter(|k| kinds.contains(k.as_str())) {
            chip(ui, kind_title(*k), theme::kind(*k));
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button(tr("Add node")).clicked() {
                new_node(ws);
            }
            if ui.button(tr("Fit")).clicked() {
                ws.graph.scene_rect = None;
            }
            if ui
                .button(tr("Auto layout"))
                .on_hover_text(tr("Reset every node to the automatic position"))
                .clicked()
            {
                let manual: Vec<String> = ws
                    .layout
                    .nodes
                    .iter()
                    .filter(|n| n.manual)
                    .map(|n| n.id.clone())
                    .collect();
                for id in manual {
                    ws.perform_json(json!({"op": "set_node_position", "id": id}));
                }
                ws.graph.scene_rect = None;
            }
            if !ws.layout.missing.is_empty() {
                chip(
                    ui,
                    trf("{n} missing dependency", &[("n", &ws.layout.missing.len())]),
                    theme::WARNING,
                );
            }
            if !ws.layout.cycles.is_empty() {
                chip(ui, trf("{n} cycle", &[("n", &ws.layout.cycles.len())]), theme::ERROR);
            }
        });
    });
    ui.separator();
}

fn bounds(ws: &Workspace) -> Rect {
    let mut r = Rect::NOTHING;
    for n in &ws.layout.nodes {
        r = r.union(Rect::from_center_size(
            Pos2::new(n.position.x as f32, n.position.y as f32),
            NODE,
        ));
    }
    r.expand(60.0)
}

fn canvas(ui: &mut Ui, ws: &mut Workspace) {
    let mut scene_rect = ws.graph.scene_rect.unwrap_or_else(|| bounds(ws));
    let selected = ws.node.clone();
    let neighbors: BTreeSet<String> = selected
        .as_ref()
        .map(|id| {
            let mut s: BTreeSet<String> = ws
                .layout
                .edges
                .iter()
                .filter(|e| &e.from == id)
                .map(|e| e.to.clone())
                .collect();
            s.extend(ws.layout.edges.iter().filter(|e| &e.to == id).map(|e| e.from.clone()));
            s.insert(id.clone());
            s
        })
        .unwrap_or_default();
    let mut clicked: Option<String> = None;
    let mut moved: Option<(String, Pos2)> = None;

    egui::Scene::new()
        .zoom_range(0.2..=2.5)
        .show(ui, &mut scene_rect, |ui| {
            let positions: std::collections::HashMap<&str, Pos2> = ws
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
            for e in &ws.layout.edges {
                let (Some(a), Some(b)) = (positions.get(e.from.as_str()), positions.get(e.to.as_str())) else {
                    continue;
                };
                let hot = selected.as_deref().is_some_and(|s| s == e.from || s == e.to);
                let color = if e.in_cycle {
                    theme::ERROR
                } else if hot {
                    theme::ACCENT
                } else {
                    ui.visuals().weak_text_color()
                };
                arrow(
                    &painter,
                    clip(*a, *b),
                    clip(*b, *a),
                    Stroke::new(if hot { 2.0 } else { 1.3 }, color),
                );
            }
            let bg = ui.visuals().extreme_bg_color;
            let text = ui.visuals().strong_text_color();
            for n in &ws.layout.nodes {
                let center = positions[n.id.as_str()];
                let rect = Rect::from_center_size(center, NODE);
                let resp = ui.interact(rect, Id::unique(("node", &n.id)), Sense::click_and_drag());
                let color = theme::kind(n.kind);
                let is_sel = selected.as_deref() == Some(n.id.as_str());
                let dim = selected.is_some() && !neighbors.contains(&n.id);
                let alpha: f32 = if dim { 0.35 } else { 1.0 };
                painter.rect(
                    rect,
                    CornerRadius::same(11),
                    bg.gamma_multiply(alpha.max(0.9)),
                    Stroke::new(
                        if is_sel { 2.0 } else { 1.0 },
                        if is_sel {
                            theme::ACCENT
                        } else {
                            ui.visuals().widgets.noninteractive.bg_stroke.color
                        },
                    ),
                    egui::StrokeKind::Inside,
                );
                painter.rect_filled(
                    Rect::from_min_size(rect.min, Vec2::new(4.0, rect.height())),
                    CornerRadius {
                        nw: 11,
                        sw: 11,
                        ne: 0,
                        se: 0,
                    },
                    color.gamma_multiply(alpha),
                );
                let missing = ws.layout.missing.iter().any(|m| m.from == n.id);
                painter.text(
                    rect.left_center() + Vec2::new(16.0, -9.0),
                    Align2::LEFT_CENTER,
                    &n.label,
                    FontId::proportional(14.0),
                    text.gamma_multiply(alpha),
                );
                let sub = ws
                    .doc
                    .node(&n.id)
                    .and_then(|d| d.path.clone())
                    .unwrap_or_else(|| kind_title(n.kind).to_owned());
                painter.text(
                    rect.left_center() + Vec2::new(16.0, 10.0),
                    Align2::LEFT_CENTER,
                    sub,
                    FontId::monospace(10.5),
                    theme::MUTED.gamma_multiply(alpha),
                );
                if missing {
                    painter.circle_filled(rect.right_top() + Vec2::new(-10.0, 10.0), 4.0, theme::WARNING);
                }
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
    ws.graph.scene_rect = Some(scene_rect);
    if let Some((id, p)) = moved {
        ws.graph.dragging = None;
        ws.perform_json(json!({"op": "set_node_position", "id": id, "position": {"x": p.x, "y": p.y}}));
    }
    if let Some(sel) = clicked {
        if ws.node.as_ref() != Some(&sel) {
            ws.node_edit = None;
        }
        ws.node = Some(sel);
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

fn arrow(painter: &egui::Painter, a: Pos2, b: Pos2, stroke: Stroke) {
    painter.line_segment([a, b], stroke);
    let dir = (b - a).normalized();
    let normal = Vec2::new(-dir.y, dir.x);
    let size = 8.0;
    painter.add(Shape::convex_polygon(
        vec![
            b,
            b - dir * size + normal * size * 0.55,
            b - dir * size - normal * size * 0.55,
        ],
        stroke.color,
        Stroke::NONE,
    ));
}

fn inspector(ui: &mut Ui, ws: &mut Workspace, node: ArchNode) {
    if ws.node_edit.as_ref().is_none_or(|e| e[0] != node.id) {
        ws.node_edit = Some([
            node.id.clone(),
            node.label.clone().unwrap_or_default(),
            node.path.clone().unwrap_or_default(),
            node.description.to_string(),
        ]);
    }
    let mut ops: Vec<serde_json::Value> = Vec::new();
    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            widgets::heading(ui, node.display_name());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(tr("Close")).clicked() {
                    ws.node = None;
                }
            });
        });
        muted(ui, &node.id);
        ui.add_space(6.0);
        widgets::card(ui, Some(tr("Details")), |ui| {
            let mut kind = node.kind;
            egui::ComboBox::from_label(tr("Type")).selected_text(kind_title(kind)).show_ui(ui, |ui| {
                for k in NodeKind::ALL {
                    ui.selectable_value(&mut kind, k, kind_title(k));
                }
            });
            if kind != node.kind {
                ops.push(json!({"op": "add_architecture_node", "id": node.id, "type": kind.as_str()}));
            }
            let edit = ws.node_edit.as_mut().expect("node edit");
            ui.label(RichText::new(tr("Label")).small());
            ui.text_edit_singleline(&mut edit[1]);
            ui.label(RichText::new(tr("Path in repository")).small());
            ui.add(egui::TextEdit::singleline(&mut edit[2]).font(egui::TextStyle::Monospace));
            ui.label(RichText::new(tr("Description")).small());
            widgets::multiline(ui, &mut edit[3], "", 3, false);
            let changed = edit[1] != node.label.clone().unwrap_or_default()
                || edit[2] != node.path.clone().unwrap_or_default()
                || edit[3] != node.description.as_str();
            if ui.add_enabled(changed, egui::Button::new(tr("Save"))).clicked() {
                ops.push(json!({"op": "add_architecture_node", "id": node.id, "label": edit[1], "path": edit[2], "description": edit[3]}));
            }
        });
        widgets::card(ui, Some(tr("Depends on")), |ui| {
            let others: Vec<(String, String)> =
                ws.doc.architecture.nodes.iter().filter(|n| n.id != node.id).map(|n| (n.id.clone(), n.display_name().to_owned())).collect();
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
            let users: Vec<String> =
                ws.doc.architecture.nodes.iter().filter(|n| n.depends_on.contains(&node.id)).map(|n| n.id.clone()).collect();
            if users.is_empty() {
                muted(ui, tr("Nothing depends on this node."));
            }
            for u in users {
                if ui.link(&u).clicked() {
                    ws.node = Some(u);
                    ws.node_edit = None;
                }
            }
        });
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if node.position.is_some() && ui.button(tr("Reset position")).clicked() {
                ops.push(json!({"op": "set_node_position", "id": node.id}));
            }
            if ui.add(egui::Button::new(RichText::new(tr("Delete node")).color(theme::ERROR))).clicked() {
                ops.push(json!({"op": "remove_architecture_node", "id": node.id}));
                ws.node = None;
            }
        });
    });
    for op in ops {
        ws.perform_json(op);
    }
}
