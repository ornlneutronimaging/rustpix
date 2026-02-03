//! Statistics panel rendering.

use eframe::egui::{self, Stroke};

use super::theme::{stat_label, stat_value, stat_value_highlight, ThemeColors};
use crate::app::RustpixApp;
use crate::util::{format_number, format_number_si};

impl RustpixApp {
    /// Render a single stat row with label on left and value on right.
    fn stat_row(ui: &mut egui::Ui, label: &str, value: &str, highlight: bool) {
        ui.horizontal(|ui| {
            ui.label(stat_label(label));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if highlight {
                    ui.label(stat_value_highlight(value));
                } else {
                    ui.label(stat_value(value));
                }
            });
        });
    }

    /// Render the statistics panel with two-column layout.
    pub(crate) fn render_statistics(&self, ui: &mut egui::Ui) {
        let colors = ThemeColors::from_ui(ui);

        if self.statistics.hit_count > 0 {
            // Hit statistics
            Self::stat_row(ui, "Hits", &format_number(self.statistics.hit_count), false);

            // TOF range
            let max_ms = self.statistics.tof_range_ms(self.tdc_frequency);
            Self::stat_row(ui, "TOF range", &format!("0.0 – {max_ms:.2} ms"), false);

            // Processing speed
            if let Some(speed) = self.statistics.load_speed() {
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let speed_usize = speed as usize;
                Self::stat_row(
                    ui,
                    "Speed",
                    &format!("{} hits/s", format_number_si(speed_usize)),
                    false,
                );
            }

            // Duration
            if let Some(dur) = self.statistics.load_duration {
                Self::stat_row(ui, "Duration", &format!("{:.2}s", dur.as_secs_f64()), false);
            }

            // Neutron statistics (if clustering was run)
            if self.statistics.neutron_count > 0 {
                ui.add_space(12.0);

                // Section divider with label
                ui.horizontal(|ui| {
                    let colors = ThemeColors::from_ui(ui);
                    ui.painter().hline(
                        ui.available_rect_before_wrap().x_range(),
                        ui.cursor().top(),
                        Stroke::new(1.0, colors.border),
                    );
                });
                ui.add_space(8.0);
                ui.label(
                    egui::RichText::new("AFTER CLUSTERING")
                        .size(10.0)
                        .color(colors.text_dim),
                );
                ui.add_space(8.0);

                // Neutron count (highlighted)
                Self::stat_row(
                    ui,
                    "Neutrons",
                    &format_number(self.statistics.neutron_count),
                    true,
                );

                // Average cluster size
                Self::stat_row(
                    ui,
                    "Avg size",
                    &format!("{:.1} hits", self.statistics.avg_cluster_size),
                    false,
                );

                // Clustering speed
                if let Some(speed) = self.statistics.cluster_speed() {
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    let speed_usize = speed as usize;
                    Self::stat_row(
                        ui,
                        "Speed",
                        &format!("{} n/s", format_number_si(speed_usize)),
                        false,
                    );
                }
            }
        } else {
            ui.label(
                egui::RichText::new("No data loaded")
                    .size(11.0)
                    .color(colors.text_dim),
            );
        }
    }
}
