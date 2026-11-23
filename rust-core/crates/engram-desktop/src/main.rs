#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod i18n;
mod markdown;
mod theme;
mod views;
mod widgets;
mod workspace;

use std::path::PathBuf;

pub const LOGO_PNG: &[u8] = include_bytes!("../../../../branding/icons/engram-256.png");

fn main() -> eframe::Result {
    let paths: Vec<PathBuf> = std::env::args_os()
        .skip(1)
        .map(PathBuf::from)
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("engram")))
        .collect();
    let mut viewport = eframe::egui::ViewportBuilder::default()
        .with_title("Engram")
        .with_app_id("com.liwidale.engram")
        .with_inner_size([1240.0, 800.0])
        .with_min_inner_size([900.0, 560.0])
        .with_drag_and_drop(true);
    if let Ok(icon) = eframe::icon_data::from_png_bytes(LOGO_PNG) {
        viewport = viewport.with_icon(icon);
    }
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    eframe::run_native(
        "Engram",
        options,
        Box::new(|cc| Ok(Box::new(app::EngramApp::new(cc, paths)))),
    )
}
