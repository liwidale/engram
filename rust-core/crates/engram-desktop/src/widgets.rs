use crate::i18n::tr;
use crate::theme;
use eframe::egui::{self, Color32, CornerRadius, Margin, RichText, Ui};

pub fn chip(ui: &mut Ui, text: impl Into<String>, color: Color32) -> egui::Response {
    egui::Frame::new()
        .fill(color.gamma_multiply(0.16))
        .corner_radius(CornerRadius::same(9))
        .inner_margin(Margin::symmetric(7, 1))
        .show(ui, |ui| {
            ui.label(RichText::new(text.into()).small().color(color).strong())
        })
        .response
}

pub fn card<R>(ui: &mut Ui, title: Option<&str>, add: impl FnOnce(&mut Ui) -> R) -> R {
    egui::Frame::new()
        .fill(ui.visuals().extreme_bg_color)
        .stroke(ui.visuals().widgets.noninteractive.bg_stroke)
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::same(12))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            if let Some(t) = title {
                ui.label(RichText::new(t.to_uppercase()).small().strong().color(theme::MUTED));
                ui.add_space(2.0);
            }
            add(ui)
        })
        .inner
}

pub fn empty_state(ui: &mut Ui, title: &str, message: &str, note: Option<&str>, action: Option<&str>) -> bool {
    let mut clicked = false;
    ui.vertical_centered(|ui| {
        ui.add_space((ui.available_height() * 0.3).max(24.0));
        ui.label(RichText::new(title).heading().strong());
        ui.add_space(4.0);
        ui.set_max_width(440.0);
        ui.label(RichText::new(message).color(theme::MUTED));
        if let Some(n) = note {
            ui.add_space(2.0);
            ui.label(RichText::new(n).small().color(theme::MUTED.gamma_multiply(0.8)));
        }
        if let Some(a) = action {
            ui.add_space(10.0);
            clicked = ui.add(primary_button(a)).clicked();
        }
    });
    clicked
}

pub fn primary_button(text: &str) -> egui::Button<'_> {
    egui::Button::new(RichText::new(text).color(Color32::WHITE).strong()).fill(theme::ACCENT)
}

pub fn tokens(ui: &mut Ui, tokens: usize, budget: Option<usize>) {
    let over = budget.is_some_and(|b| tokens > b);
    let text = match budget {
        Some(b) => format!("{tokens} / {b} {}", tr("tokens")),
        None => format!("{tokens} {}", tr("tokens")),
    };
    ui.label(
        RichText::new(text)
            .small()
            .color(if over { theme::ERROR } else { theme::MUTED }),
    );
}

pub fn muted(ui: &mut Ui, text: impl Into<String>) -> egui::Response {
    ui.label(RichText::new(text.into()).small().color(theme::MUTED))
}

pub fn heading(ui: &mut Ui, text: impl Into<String>) {
    ui.label(RichText::new(text.into()).size(18.0).strong());
}

pub fn multiline(ui: &mut Ui, text: &mut String, hint: &str, rows: usize, code: bool) -> egui::Response {
    if code {
        return code_edit(ui, text, hint, rows as f32 * 18.0);
    }
    ui.add(
        egui::TextEdit::multiline(text)
            .hint_text(hint)
            .desired_rows(rows)
            .desired_width(f32::INFINITY),
    )
}

pub fn code_edit(ui: &mut Ui, text: &mut String, hint: &str, min_height: f32) -> egui::Response {
    let s = crate::settings::get();
    let font = egui::FontId::monospace(s.mono_size);
    let wrap = s.soft_wrap;
    let color = ui.visuals().text_color();
    let job_font = font.clone();
    let mut layouter = move |ui: &Ui, buf: &dyn egui::TextBuffer, width: f32| {
        let width = if wrap { width } else { f32::INFINITY };
        let job = egui::text::LayoutJob::simple(buf.as_str().to_owned(), job_font.clone(), color, width);
        ui.ctx().fonts_mut(|f| f.layout_job(job))
    };
    let width = ui.available_width();
    let edit = egui::TextEdit::multiline(text)
        .hint_text(hint)
        .font(font)
        .desired_width(if wrap { width } else { f32::INFINITY })
        .layouter(&mut layouter);
    if wrap {
        ui.add_sized([width, min_height], edit)
    } else {
        egui::ScrollArea::horizontal()
            .id_salt(ui.next_auto_id())
            .show(ui, |ui| ui.add_sized([width, min_height], edit))
            .inner
    }
}

pub fn lines_to_list(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_owned)
        .collect()
}

pub fn relative_time(rfc3339: Option<&str>) -> String {
    let Some(at) = rfc3339.and_then(engram_core::time::parse_rfc3339) else {
        return tr("never").to_owned();
    };
    let secs = (engram_core::time::now_unix() - at).max(0);
    match secs {
        0..=44 => tr("a moment ago").to_owned(),
        45..=3599 => crate::i18n::trf("{n} min ago", &[("n", &(secs / 60).max(1))]),
        3600..=86_399 => crate::i18n::trf("{n} h ago", &[("n", &(secs / 3600))]),
        _ => crate::i18n::trf("{n} d ago", &[("n", &(secs / 86_400))]),
    }
}

pub fn conflict_bar(ui: &mut Ui) -> Option<bool> {
    let mut choice = None;
    egui::Frame::new()
        .fill(theme::WARNING.gamma_multiply(0.14))
        .corner_radius(6)
        .inner_margin(Margin::symmetric(10, 6))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(tr("Modified outside Engram while you were editing."))
                        .color(ui.visuals().strong_text_color()),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.add(primary_button(tr("Keep mine"))).clicked() {
                        choice = Some(false);
                    }
                    if ui.button(tr("Load external")).clicked() {
                        choice = Some(true);
                    }
                });
            });
        });
    choice
}

pub fn save_status(ui: &mut Ui, dirty: bool) {
    let autosave = crate::settings::get().autosave;
    let text = match (dirty, autosave) {
        (false, _) => tr("Saved"),
        (true, true) => tr("Edited"),
        (true, false) => tr("Unsaved · Ctrl+S"),
    };
    ui.label(RichText::new(text).small().color(if dirty && !autosave {
        theme::ACCENT
    } else {
        theme::MUTED
    }));
}

pub fn path_field(
    ui: &mut Ui,
    id: &str,
    value: &mut String,
    suggestions: impl FnOnce(&str) -> Vec<String>,
    hint: &str,
) -> bool {
    let response = ui.add(
        egui::TextEdit::singleline(value)
            .hint_text(hint)
            .font(egui::TextStyle::Monospace)
            .desired_width(ui.available_width().min(420.0))
            .id_salt(id),
    );
    let mut picked = response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
    if response.has_focus() && !value.trim().is_empty() {
        let hits: Vec<String> = suggestions(value).into_iter().filter(|h| h != value).collect();
        if !hits.is_empty() {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                for h in hits {
                    if ui
                        .add(egui::Button::new(RichText::new(&h).monospace().small()).frame(false))
                        .clicked()
                    {
                        *value = h;
                        picked = true;
                    }
                }
            });
        }
    }
    picked
}

pub fn file_list(
    ui: &mut Ui,
    id: &str,
    files: &mut Vec<String>,
    query: &mut String,
    suggestions: impl FnOnce(&str) -> Vec<String>,
) -> bool {
    let mut changed = false;
    let mut remove = None;
    for (i, f) in files.iter().enumerate() {
        ui.horizontal(|ui| {
            ui.label(RichText::new(f).monospace().small());
            if ui.small_button("x").on_hover_text(tr("Remove")).clicked() {
                remove = Some(i);
            }
        });
    }
    if let Some(i) = remove {
        files.remove(i);
        changed = true;
    }
    ui.horizontal(|ui| {
        let picked = path_field(ui, id, query, suggestions, tr("Add a file or folder"));
        let add = ui
            .add_enabled(!query.trim().is_empty(), egui::Button::new(tr("Add")))
            .clicked();
        if (picked || add) && !query.trim().is_empty() {
            let path = query.trim().to_owned();
            if !files.contains(&path) {
                files.push(path);
                changed = true;
            }
            query.clear();
        }
    });
    changed
}

pub fn chip_button(ui: &mut Ui, text: impl Into<String>, color: Color32, active: bool) -> egui::Response {
    let fill = if active { color } else { color.gamma_multiply(0.16) };
    let fg = if active { Color32::WHITE } else { color };
    egui::Frame::new()
        .fill(fill)
        .corner_radius(CornerRadius::same(9))
        .inner_margin(Margin::symmetric(7, 1))
        .show(ui, |ui| ui.label(RichText::new(text.into()).small().color(fg).strong()))
        .response
        .interact(egui::Sense::click())
}

pub fn score_gauge(ui: &mut Ui, score: u8, size: f32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
    let painter = ui.painter();
    let center = rect.center();
    let radius = size / 2.0 - 6.0;
    painter.circle_stroke(center, radius, egui::Stroke::new(8.0, ui.visuals().faint_bg_color));
    let color = theme::score(score);
    let steps = 64;
    let sweep = std::f32::consts::TAU * f32::from(score) / 100.0;
    let points: Vec<egui::Pos2> = (0..=steps)
        .map(|i| {
            let a = -std::f32::consts::FRAC_PI_2 + sweep * i as f32 / steps as f32;
            center + egui::vec2(a.cos(), a.sin()) * radius
        })
        .collect();
    if score > 0 {
        painter.add(egui::Shape::line(points, egui::Stroke::new(8.0, color)));
    }
    painter.text(
        center,
        egui::Align2::CENTER_CENTER,
        score.to_string(),
        egui::FontId::proportional(size * 0.3),
        ui.visuals().strong_text_color(),
    );
}

pub fn module_icon(ui: &mut Ui, module: crate::workspace::Module, color: Color32) {
    use crate::workspace::Module;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::hover());
    let p = ui.painter();
    let c = rect.center();
    let stroke = egui::Stroke::new(1.4, color);
    match module {
        Module::Memory => {
            p.rect_stroke(rect.shrink(2.5), 2.0, stroke, egui::StrokeKind::Inside);
            p.line_segment([c + egui::vec2(-3.0, -1.5), c + egui::vec2(3.0, -1.5)], stroke);
            p.line_segment([c + egui::vec2(-3.0, 1.5), c + egui::vec2(1.5, 1.5)], stroke);
        }
        Module::Architecture => {
            let a = c + egui::vec2(0.0, -4.5);
            let b = c + egui::vec2(-4.5, 4.0);
            let d = c + egui::vec2(4.5, 4.0);
            p.line_segment([a, b], stroke);
            p.line_segment([a, d], stroke);
            for q in [a, b, d] {
                p.circle_filled(q, 2.2, color);
            }
        }
        Module::Decisions => {
            p.line_segment([c + egui::vec2(0.0, -6.0), c + egui::vec2(0.0, 6.0)], stroke);
            p.line_segment([c + egui::vec2(0.0, -3.5), c + egui::vec2(5.0, -3.5)], stroke);
            p.line_segment([c + egui::vec2(0.0, 1.0), c + egui::vec2(-5.0, 1.0)], stroke);
        }
        Module::Prompts => {
            p.line_segment([c + egui::vec2(-5.0, -3.0), c + egui::vec2(5.0, -3.0)], stroke);
            p.line_segment([c + egui::vec2(-5.0, 0.5), c + egui::vec2(3.0, 0.5)], stroke);
            p.line_segment([c + egui::vec2(-5.0, 4.0), c + egui::vec2(1.0, 4.0)], stroke);
        }
        Module::Context => {
            for (i, w) in [10.0f32, 8.0, 6.0].iter().enumerate() {
                let y = -3.5 + i as f32 * 3.5;
                p.rect_stroke(
                    egui::Rect::from_center_size(c + egui::vec2(0.0, y), egui::vec2(*w, 2.0)),
                    1.0,
                    stroke,
                    egui::StrokeKind::Inside,
                );
            }
        }
        Module::Health => {
            p.circle_stroke(c, 5.5, stroke);
            p.line_segment([c + egui::vec2(-3.0, 0.0), c + egui::vec2(-1.0, 2.5)], stroke);
            p.line_segment([c + egui::vec2(-1.0, 2.5), c + egui::vec2(3.0, -2.5)], stroke);
        }
        Module::Handoff => {
            p.line_segment([c + egui::vec2(-5.0, 0.0), c + egui::vec2(4.0, 0.0)], stroke);
            p.line_segment([c + egui::vec2(1.0, -3.0), c + egui::vec2(4.5, 0.0)], stroke);
            p.line_segment([c + egui::vec2(1.0, 3.0), c + egui::vec2(4.5, 0.0)], stroke);
        }
    }
}
