//! TOF histogram window rendering.

use eframe::egui;
use egui_plot::{Bar, BarChart, Plot, VLine};

use crate::app::RustpixApp;
use crate::util::{u64_to_f64, usize_to_f64};

impl RustpixApp {
    /// Render the TOF histogram window (if visible).
    ///
    /// When the slicer is enabled, the vertical line marker can be dragged
    /// or clicked to navigate TOF slices interactively.
    #[allow(clippy::too_many_lines)]
    pub(crate) fn render_histogram_window(&mut self, ctx: &egui::Context) {
        if !self.ui_state.show_histogram {
            return;
        }

        // Clone spectrum data and slicer state to avoid borrow conflict with UI state
        let spectrum = self.tof_spectrum().map(<[u64]>::to_vec);
        let slicer_enabled = self.ui_state.slicer_enabled;
        let current_tof_bin = self.ui_state.current_tof_bin;

        // Track if bin changed via dragging (to trigger texture regeneration)
        let mut new_tof_bin: Option<usize> = None;

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
                    if slicer_enabled {
                        ui.label("(drag marker to navigate)");
                    }
                });

                let plot_response = Plot::new("tof_hist")
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

                        // Handle dragging to move slice marker
                        if slicer_enabled && n_bins > 0 {
                            let drag_delta = plot_ui.pointer_coordinate_drag_delta();
                            // Only respond to horizontal drag
                            if drag_delta.x.abs() > 0.0 {
                                if let Some(coord) = plot_ui.pointer_coordinate() {
                                    let x_us = coord.x;
                                    #[allow(
                                        clippy::cast_possible_truncation,
                                        clippy::cast_sign_loss
                                    )]
                                    let bin = if x_us <= 0.0 {
                                        0
                                    } else if x_us >= max_us {
                                        n_bins - 1
                                    } else {
                                        ((x_us / bin_width_us) as usize).min(n_bins - 1)
                                    };
                                    if bin != current_tof_bin {
                                        new_tof_bin = Some(bin);
                                    }
                                }
                            }
                        }
                    });

                // Also allow click to set position (not just drag)
                if slicer_enabled && n_bins > 0 && plot_response.response.clicked() {
                    if let Some(pos) = plot_response.response.interact_pointer_pos() {
                        // Convert screen position to plot coordinates
                        let plot_bounds = plot_response.transform.bounds();
                        let plot_rect = plot_response.response.rect;
                        let x_frac =
                            f64::from(pos.x - plot_rect.left()) / f64::from(plot_rect.width());
                        let x_us = plot_bounds.min()[0]
                            + x_frac * (plot_bounds.max()[0] - plot_bounds.min()[0]);

                        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                        let bin = if x_us <= 0.0 {
                            0
                        } else if x_us >= max_us {
                            n_bins - 1
                        } else {
                            ((x_us / bin_width_us) as usize).min(n_bins - 1)
                        };
                        if bin != current_tof_bin {
                            new_tof_bin = Some(bin);
                        }
                    }
                }
            } else {
                ui.label("No Data");
            }
        });

        // Update slicer state if bin changed via interaction
        if let Some(bin) = new_tof_bin {
            self.ui_state.current_tof_bin = bin;
            self.texture = None;
        }
    }
}
