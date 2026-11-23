use crate::theme;
use crate::widgets::chip;
use eframe::egui::{self, text::LayoutJob, Color32, FontId, Margin, RichText, TextFormat, Ui};

#[derive(Debug, PartialEq)]
pub enum Block {
    Heading(usize, String),
    Paragraph(String),
    Superseded(String),
    List(Vec<Item>),
    Quote(String),
    Code(String),
    Rule,
}

#[derive(Debug, PartialEq)]
pub struct Item {
    pub indent: usize,
    pub marker: Marker,
    pub text: String,
}

#[derive(Debug, PartialEq)]
pub enum Marker {
    Bullet,
    Number(u32),
    Task(bool),
}

pub fn parse(text: &str) -> Vec<Block> {
    let mut blocks = Vec::new();
    let mut paragraph: Vec<&str> = Vec::new();
    let mut items: Vec<Item> = Vec::new();
    let mut quote: Vec<&str> = Vec::new();
    let mut code: Option<Vec<&str>> = None;

    let flush = |blocks: &mut Vec<Block>, paragraph: &mut Vec<&str>, items: &mut Vec<Item>, quote: &mut Vec<&str>| {
        if !paragraph.is_empty() {
            let p = paragraph.join(" ");
            match p.get(..12) {
                Some(head) if head.eq_ignore_ascii_case("[SUPERSEDED]") => {
                    blocks.push(Block::Superseded(p[12..].trim().to_owned()))
                }
                _ => blocks.push(Block::Paragraph(p)),
            }
            paragraph.clear();
        }
        if !items.is_empty() {
            blocks.push(Block::List(std::mem::take(items)));
        }
        if !quote.is_empty() {
            blocks.push(Block::Quote(quote.join(" ")));
            quote.clear();
        }
    };

    for raw in text.lines() {
        if let Some(lines) = code.as_mut() {
            if raw.trim_start().starts_with("```") {
                blocks.push(Block::Code(lines.join("\n")));
                code = None;
            } else {
                lines.push(raw);
            }
            continue;
        }
        let line = raw.trim();
        let indent = raw.len() - raw.trim_start().len();
        if line.starts_with("```") {
            flush(&mut blocks, &mut paragraph, &mut items, &mut quote);
            code = Some(Vec::new());
        } else if line.is_empty() {
            flush(&mut blocks, &mut paragraph, &mut items, &mut quote);
        } else if let Some((level, rest)) = heading(line) {
            flush(&mut blocks, &mut paragraph, &mut items, &mut quote);
            blocks.push(Block::Heading(level, rest.to_owned()));
        } else if matches!(line, "---" | "***" | "___") {
            flush(&mut blocks, &mut paragraph, &mut items, &mut quote);
            blocks.push(Block::Rule);
        } else if let Some(rest) = line.strip_prefix('>') {
            if !paragraph.is_empty() || !items.is_empty() {
                flush(&mut blocks, &mut paragraph, &mut items, &mut quote);
            }
            quote.push(rest.trim());
        } else if let Some(item) = list_item(line, indent / 2) {
            if !paragraph.is_empty() || !quote.is_empty() {
                flush(&mut blocks, &mut paragraph, &mut items, &mut quote);
            }
            items.push(item);
        } else if indent > 0 && !items.is_empty() {
            let last = items.last_mut().expect("non-empty");
            last.text.push(' ');
            last.text.push_str(line);
        } else {
            if !items.is_empty() || !quote.is_empty() {
                flush(&mut blocks, &mut paragraph, &mut items, &mut quote);
            }
            paragraph.push(line);
        }
    }
    if let Some(lines) = code {
        blocks.push(Block::Code(lines.join("\n")));
    }
    flush(&mut blocks, &mut paragraph, &mut items, &mut quote);
    blocks
}

fn heading(line: &str) -> Option<(usize, &str)> {
    let level = line.chars().take_while(|c| *c == '#').count();
    ((1..=6).contains(&level) && line[level..].starts_with(' ')).then(|| (level, line[level..].trim()))
}

fn list_item(line: &str, indent: usize) -> Option<Item> {
    for bullet in ["- ", "* ", "+ "] {
        if let Some(rest) = line.strip_prefix(bullet) {
            if let Some(t) = rest.strip_prefix("[ ] ") {
                return Some(Item {
                    indent,
                    marker: Marker::Task(false),
                    text: t.into(),
                });
            }
            if let Some(t) = rest.strip_prefix("[x] ").or_else(|| rest.strip_prefix("[X] ")) {
                return Some(Item {
                    indent,
                    marker: Marker::Task(true),
                    text: t.into(),
                });
            }
            return Some(Item {
                indent,
                marker: Marker::Bullet,
                text: rest.into(),
            });
        }
    }
    let digits: String = line.chars().take_while(char::is_ascii_digit).collect();
    let rest = line[digits.len()..].strip_prefix(". ")?;
    let n = digits.parse().ok().filter(|_| digits.len() <= 3)?;
    Some(Item {
        indent,
        marker: Marker::Number(n),
        text: rest.into(),
    })
}

pub fn show(ui: &mut Ui, text: &str) {
    ui.spacing_mut().item_spacing.y = 6.0;
    for block in parse(text) {
        match block {
            Block::Heading(level, s) => {
                let size = match level {
                    1 => 22.0,
                    2 => 18.0,
                    _ => 15.0,
                };
                ui.add_space(if level <= 2 { 6.0 } else { 2.0 });
                ui.label(inline(ui, &s, size, true));
            }
            Block::Paragraph(s) => {
                ui.label(inline(ui, &s, 14.0, false));
            }
            Block::Superseded(s) => {
                ui.horizontal_wrapped(|ui| {
                    chip(ui, "SUPERSEDED", theme::WARNING);
                    ui.label(RichText::new(s).strikethrough().color(theme::MUTED));
                });
            }
            Block::List(items) => {
                for item in items {
                    ui.horizontal_wrapped(|ui| {
                        ui.add_space(item.indent as f32 * 16.0);
                        let marker = match item.marker {
                            Marker::Bullet => "•".to_owned(),
                            Marker::Number(n) => format!("{n}."),
                            Marker::Task(true) => "[x]".to_owned(),
                            Marker::Task(false) => "[ ]".to_owned(),
                        };
                        ui.label(RichText::new(marker).color(theme::MUTED).monospace());
                        ui.label(inline(ui, &item.text, 14.0, false));
                    });
                }
            }
            Block::Quote(s) => {
                egui::Frame::new()
                    .inner_margin(Margin {
                        left: 10,
                        right: 0,
                        top: 2,
                        bottom: 2,
                    })
                    .stroke(egui::Stroke::NONE)
                    .show(ui, |ui| {
                        let r = ui.label(inline(ui, &s, 14.0, false).color(theme::MUTED));
                        ui.painter().vline(
                            r.rect.left() - 8.0,
                            r.rect.y_range(),
                            egui::Stroke::new(3.0, theme::ACCENT.gamma_multiply(0.5)),
                        );
                    });
            }
            Block::Code(s) => {
                egui::Frame::new()
                    .fill(ui.visuals().faint_bg_color)
                    .corner_radius(6)
                    .inner_margin(Margin::same(8))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.label(RichText::new(s).monospace());
                    });
            }
            Block::Rule => {
                ui.separator();
            }
        }
    }
}

fn inline(ui: &Ui, text: &str, size: f32, strong: bool) -> egui::WidgetText {
    let color = ui.visuals().text_color();
    let strong_color = ui.visuals().strong_text_color();
    let mut job = LayoutJob::default();
    let mut rest = text;
    let fmt = |mono: bool, bold: bool, italic: bool| TextFormat {
        font_id: if mono {
            FontId::monospace(size - 1.0)
        } else {
            FontId::proportional(size)
        },
        color: if bold || strong { strong_color } else { color },
        italics: italic,
        background: if mono {
            ui.visuals().faint_bg_color
        } else {
            Color32::TRANSPARENT
        },
        ..Default::default()
    };
    while !rest.is_empty() {
        let next = ["`", "**", "*"]
            .iter()
            .filter_map(|m| rest.find(m).map(|i| (i, *m)))
            .min_by_key(|(i, _)| *i);
        let Some((start, marker)) = next else {
            job.append(rest, 0.0, fmt(false, false, false));
            break;
        };
        let body_start = start + marker.len();
        match rest[body_start..].find(marker) {
            Some(len) if len > 0 => {
                job.append(&rest[..start], 0.0, fmt(false, false, false));
                let body = &rest[body_start..body_start + len];
                job.append(body, 0.0, fmt(marker == "`", marker == "**", marker == "*"));
                rest = &rest[body_start + len + marker.len()..];
            }
            _ => {
                job.append(&rest[..body_start], 0.0, fmt(false, false, false));
                rest = &rest[body_start..];
            }
        }
    }
    job.into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_blocks() {
        let blocks = parse("# Title\n\nPara one\ncontinues.\n\n- [x] done\n- todo\n  more\n\n```\nlet x = 1;\n```\n> quote\n\n[SUPERSEDED] old");
        assert_eq!(blocks[0], Block::Heading(1, "Title".into()));
        assert_eq!(blocks[1], Block::Paragraph("Para one continues.".into()));
        let Block::List(items) = &blocks[2] else {
            panic!("{blocks:?}")
        };
        assert_eq!(items[0].marker, Marker::Task(true));
        assert_eq!(items[1].text, "todo more");
        assert_eq!(blocks[3], Block::Code("let x = 1;".into()));
        assert_eq!(blocks[4], Block::Quote("quote".into()));
        assert_eq!(blocks[5], Block::Superseded("old".into()));
    }
}
