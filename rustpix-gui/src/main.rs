//! Rustpix GUI application entry point.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod histogram;
mod message;
mod pipeline;
mod recent;
mod state;
mod ui;
mod util;
mod viewer;

use app::RustpixApp;
use eframe::egui;

fn main() -> eframe::Result<()> {
    env_logger::init();
    let mut viewport = egui::ViewportBuilder::default().with_inner_size([1200.0, 800.0]);
    if let Some(icon) = load_app_icon() {
        viewport = viewport.with_icon(icon);
    }
    let opts = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    eframe::run_native(
        "Rustpix",
        opts,
        Box::new(|cc| {
            // Apply custom styling based on system theme preference
            ui::theme::configure_style(&cc.egui_ctx);
            egui_extras::install_image_loaders(&cc.egui_ctx);
            Ok(Box::new(RustpixApp::default()))
        }),
    )
}

fn load_app_icon() -> Option<egui::IconData> {
    let bytes = include_bytes!("../assets/icons/app-icon.svg");
    let size_hint = egui::SizeHint::Size {
        width: 256,
        height: 256,
        maintain_aspect_ratio: true,
    };
    // Options type is resvg::usvg::Options (transitive dep, not directly nameable)
    #[allow(clippy::default_trait_access)]
    let svg_options = Default::default();
    let image =
        egui_extras::image::load_svg_bytes_with_size(bytes, size_hint, &svg_options).ok()?;
    let [width, height] = image.size;
    let width = u32::try_from(width).ok()?;
    let height = u32::try_from(height).ok()?;
    let capacity = usize::try_from(width)
        .ok()?
        .saturating_mul(usize::try_from(height).ok()?)
        .saturating_mul(4);
    let mut rgba = Vec::with_capacity(capacity);
    for pixel in image.pixels {
        rgba.extend_from_slice(&pixel.to_array());
    }
    Some(egui::IconData {
        rgba,
        width,
        height,
    })
}
