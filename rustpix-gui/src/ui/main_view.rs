//! Main view (central panel) rendering.

use eframe::egui;
use egui_plot::{Plot, PlotImage, PlotPoint};

use crate::app::RustpixApp;
use crate::util::f64_to_usize_bounded;

impl RustpixApp {
    /// Render the central panel with the histogram image.
    pub(crate) fn render_central_panel(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            if let Some(tex) = &self.texture {
                Plot::new("plot").data_aspect(1.0).show(ui, |plot_ui| {
                    plot_ui.image(PlotImage::new(
                        tex,
                        PlotPoint::new(256.0, 256.0),
                        [512.0, 512.0],
                    ));

                    if let Some(curr) = plot_ui.pointer_coordinate() {
                        let x = curr.x;
                        let y = curr.y;
                        if x >= 0.0 && y >= 0.0 && x < 512.0 && y < 512.0 {
                            let (Some(xi), Some(yi)) =
                                (f64_to_usize_bounded(x, 512), f64_to_usize_bounded(y, 512))
                            else {
                                self.cursor_info = None;
                                return;
                            };
                            let count = if let Some(counts) = &self.hit_counts {
                                counts[yi * 512 + xi]
                            } else {
                                0
                            };
                            self.cursor_info = Some((xi, yi, count));
                        } else {
                            self.cursor_info = None;
                        }
                    } else {
                        self.cursor_info = None;
                    }
                });
            } else {
                ui.centered_and_justified(|ui| ui.label("No Data"));
            }
        });
    }
}
