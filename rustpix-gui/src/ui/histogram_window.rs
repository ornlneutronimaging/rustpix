//! TOF histogram window rendering.

use eframe::egui;
use egui_plot::{Bar, BarChart, Plot, VLine};

use crate::app::RustpixApp;
use crate::util::{u64_to_f64, usize_to_f64};

impl RustpixApp {
    /// Render the TOF histogram window (if visible).
    pub(crate) fn render_histogram_window(&mut self, ctx: &egui::Context) {
        if !self.ui_state.show_histogram {
            return;
        }

        // Clone spectrum data and slicer state to avoid borrow conflict with UI state
        let spectrum = self.tof_spectrum().map(<[u64]>::to_vec);
        let slicer_enabled = self.ui_state.slicer_enabled;
        let current_tof_bin = self.ui_state.current_tof_bin;

        egui::Window::new("TOF Histogram").show(ctx, |ui| {
            if let Some(full) = spectrum.as_ref() {
                let tdc_period = 1.0 / self.tdc_frequency; // s
                let max_us = tdc_period * 1e6; // microseconds
                let n_bins = full.len();
                let bin_width_us = max_us / usize_to_f64(n_bins);

                ui.horizontal(|ui| {
                    ui.checkbox(&mut self.ui_state.log_plot, "Log Scale");
                    ui.label(format!(
                        "Range: 0 - {:.0} µs ({:.1} Hz)",
                        max_us, self.tdc_frequency
                    ));
                });

                Plot::new("tof_hist")
                    .x_axis_label("Time-of-Flight (µs)")
                    .y_axis_label(if self.ui_state.log_plot {
                        "Log10(Counts)"
                    } else {
                        "Counts"
                    })
                    .include_x(0.0)
                    .include_x(max_us)
                    .include_y(0.0)
                    .show(ui, |plot_ui: &mut egui_plot::PlotUi| {
                        let bars: Vec<Bar> = full
                            .iter()
                            .enumerate()
                            .map(|(i, &c)| {
                                let x = usize_to_f64(i) * bin_width_us;
                                let val = if self.ui_state.log_plot {
                                    if c > 0 {
                                        u64_to_f64(c).log10()
                                    } else {
                                        0.0
                                    }
                                } else {
                                    u64_to_f64(c)
                                };

                                Bar::new(x, val)
                                    .width(bin_width_us)
                                    .fill(egui::Color32::BLUE)
                            })
                            .collect();
                        plot_ui.bar_chart(BarChart::new(bars).name("Full"));

                        // Draw slice marker when slicer is enabled
                        if slicer_enabled && current_tof_bin < n_bins {
                            let slice_x = usize_to_f64(current_tof_bin) * bin_width_us;
                            plot_ui.vline(
                                VLine::new(slice_x)
                                    .color(egui::Color32::RED)
                                    .width(2.0)
                                    .name(format!("Slice {}", current_tof_bin + 1)),
                            );
                        }
                    });
            } else {
                ui.label("No Data");
            }
        });
    }
}
