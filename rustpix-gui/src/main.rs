//! Rustpix GUI application entry point.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod histogram;
mod message;
mod pipeline;
mod state;
mod ui;
mod util;
mod viewer;

use app::RustpixApp;
use eframe::egui;

fn main() -> eframe::Result<()> {
    env_logger::init();
    let opts = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1200.0, 800.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Rustpix",
        opts,
        Box::new(|cc| {
            // Apply custom styling based on system theme preference
            ui::theme::configure_style(&cc.egui_ctx);
            Ok(Box::new(RustpixApp::default()))
        }),
    )
}
