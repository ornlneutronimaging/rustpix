//! Control panel (left sidebar) rendering.

use eframe::egui;
use rfd::FileDialog;

use crate::app::RustpixApp;
use crate::pipeline::AlgorithmType;
use crate::viewer::Colormap;

impl RustpixApp {
    /// Render the left control panel.
    pub(crate) fn render_side_panel(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("ctrl").show(ctx, |ui| {
            ui.heading("Rustpix GUI");
            ui.separator();

            self.render_file_controls(ui);
            ui.separator();

            self.render_progress_status(ui);
            ui.separator();

            self.render_statistics(ui);
            ui.separator();

            self.render_cursor_info(ui);
            ui.separator();

            self.render_visualization_controls(ctx, ui);
            ui.separator();

            self.render_processing_controls(ui);
        });
    }

    fn render_file_controls(&mut self, ui: &mut egui::Ui) {
        // Disable file loading while loading or processing to prevent state corruption
        let can_load = !self.processing.is_loading && !self.processing.is_processing;
        if ui
            .add_enabled(can_load, egui::Button::new("Open File"))
            .clicked()
        {
            if let Some(path) = FileDialog::new().add_filter("TPX3", &["tpx3"]).pick_file() {
                self.load_file(path);
            }
        }
        if let Some(p) = &self.selected_file {
            ui.label(p.file_name().unwrap_or_default().to_string_lossy());
        }
    }

    fn render_progress_status(&mut self, ui: &mut egui::Ui) {
        let is_busy = self.processing.is_loading || self.processing.is_processing;

        if is_busy {
            ui.add(
                egui::ProgressBar::new(self.processing.progress).text(&self.processing.status_text),
            );
            if ui.button("Cancel").clicked() {
                self.cancel_operation();
            }
        } else if !self.processing.status_text.is_empty()
            && self.processing.status_text != "Ready"
        {
            ui.label(&self.processing.status_text);
        }
    }

    fn render_cursor_info(&self, ui: &mut egui::Ui) {
        if let Some((x, y, count)) = self.cursor_info {
            ui.label(egui::RichText::new("Pixel Info:").strong());
            ui.label(format!("X: {x}\nY: {y}\nHits: {count}"));
        } else {
            ui.label("Hover over image for details.");
        }
    }

    fn render_visualization_controls(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        ui.heading("Visualization");
        egui::ComboBox::from_label("Color")
            .selected_text(self.colormap.to_string())
            .show_ui(ui, |ui| {
                if ui
                    .selectable_value(&mut self.colormap, Colormap::Green, "Green")
                    .clicked()
                {
                    self.texture = None;
                }
                if ui
                    .selectable_value(&mut self.colormap, Colormap::Hot, "Hot")
                    .clicked()
                {
                    self.texture = None;
                }
                if ui
                    .selectable_value(&mut self.colormap, Colormap::Grayscale, "Gray")
                    .clicked()
                {
                    self.texture = None;
                }
                if ui
                    .selectable_value(&mut self.colormap, Colormap::Viridis, "Viridis")
                    .clicked()
                {
                    self.texture = None;
                }
            });

        // TOF Slicer controls
        let n_bins = self.n_tof_bins();
        if n_bins > 0 {
            if ui
                .toggle_value(&mut self.ui_state.slicer_enabled, "TOF Slicer")
                .changed()
            {
                self.texture = None;
            }

            if self.ui_state.slicer_enabled {
                // Clamp current bin to valid range
                self.ui_state.current_tof_bin = self.ui_state.current_tof_bin.min(n_bins - 1);

                let old_bin = self.ui_state.current_tof_bin;
                let slider_response = ui.add(
                    egui::Slider::new(&mut self.ui_state.current_tof_bin, 0..=(n_bins - 1))
                        .text("TOF Bin"),
                );
                // Show current bin info
                ui.label(format!(
                    "Slice {}/{}",
                    self.ui_state.current_tof_bin + 1,
                    n_bins
                ));
                if slider_response.changed() || self.ui_state.current_tof_bin != old_bin {
                    self.texture = None;
                }
            }
        }

        // Regenerate texture if needed
        if self.texture.is_none() && self.hit_counts.is_some() {
            let img = self.generate_histogram();
            self.texture = Some(ctx.load_texture("hist", img, egui::TextureOptions::NEAREST));
        }

        ui.toggle_value(&mut self.ui_state.show_histogram, "Show TOF Histogram");
    }

    fn render_processing_controls(&mut self, ui: &mut egui::Ui) {
        ui.heading("Processing");
        ui.add(
            egui::DragValue::new(&mut self.tdc_frequency)
                .speed(0.1)
                .range(1.0..=120.0)
                .prefix("TDC (Hz): "),
        );

        egui::ComboBox::from_label("Algo")
            .selected_text(self.algo_type.to_string())
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut self.algo_type, AlgorithmType::Grid, "Grid");
                ui.selectable_value(&mut self.algo_type, AlgorithmType::Abs, "ABS");
                ui.selectable_value(&mut self.algo_type, AlgorithmType::Dbscan, "DBSCAN");
            });

        ui.add(egui::Slider::new(&mut self.radius, 1.0..=50.0).text("Radius"));
        ui.add(
            egui::Slider::new(&mut self.temporal_window_ns, 10.0..=500.0).text("Time Window (ns)"),
        );
        ui.add(egui::Slider::new(&mut self.min_cluster_size, 1..=10).text("Min Cluster"));

        if self.algo_type == AlgorithmType::Dbscan {
            ui.add(egui::Slider::new(&mut self.dbscan_min_points, 1..=10).text("Min Points"));
        }

        let can_cluster = !self.processing.is_loading
            && !self.processing.is_processing
            && self.hit_batch.is_some();

        if ui
            .add_enabled(can_cluster, egui::Button::new("Run Clustering"))
            .clicked()
        {
            self.processing.reset_cancel();
            self.run_processing();
        }
    }
}
