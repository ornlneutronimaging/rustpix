//! Statistics panel rendering.

use eframe::egui;

use crate::app::RustpixApp;
use crate::util::{format_number, format_number_si};

impl RustpixApp {
    /// Render the statistics panel.
    pub(crate) fn render_statistics(&self, ui: &mut egui::Ui) {
        ui.heading("Statistics");

        if self.statistics.hit_count > 0 {
            // Hit statistics
            ui.label(format!("Hits: {}", format_number(self.statistics.hit_count)));

            // TOF range
            let max_ms = self.statistics.tof_range_ms(self.tdc_frequency);
            ui.label(format!("TOF: 0.0 - {max_ms:.2} ms"));

            // Processing speed
            if let Some(speed) = self.statistics.load_speed() {
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let speed_usize = speed as usize;
                ui.label(format!("Speed: {} hits/s", format_number_si(speed_usize)));
            }

            // Duration
            if let Some(dur) = self.statistics.load_duration {
                ui.label(format!("Duration: {:.2}s", dur.as_secs_f64()));
            }

            // Neutron statistics (if clustering was run)
            if self.statistics.neutron_count > 0 {
                ui.separator();
                ui.label(egui::RichText::new("After Clustering").strong());
                ui.label(format!(
                    "Neutrons: {}",
                    format_number(self.statistics.neutron_count)
                ));
                ui.label(format!("Avg size: {:.1} hits", self.statistics.avg_cluster_size));

                if let Some(speed) = self.statistics.cluster_speed() {
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    let speed_usize = speed as usize;
                    ui.label(format!("Speed: {} n/s", format_number_si(speed_usize)));
                }
            }
        } else {
            ui.label("No data loaded");
        }
    }
}
