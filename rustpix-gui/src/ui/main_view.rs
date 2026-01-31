//! Main view (central panel) rendering.

use eframe::egui::{self, Color32, Rounding, Stroke, Vec2b};
use egui_plot::{Line, Plot, PlotBounds, PlotImage, PlotPoint, PlotPoints, VLine};

use super::theme::{accent, ThemeColors};
use crate::app::RustpixApp;
use crate::state::ViewMode;
use crate::util::{f64_to_usize_bounded, u64_to_f64, usize_to_f64};

impl RustpixApp {
    /// Render the central panel with histogram, slicer, and spectrum.
    #[allow(clippy::too_many_lines)]
    pub(crate) fn render_central_panel(&mut self, ctx: &egui::Context) {
        // Get theme-aware colors
        let colors = ThemeColors::from_ctx(ctx);

        // Ensure texture is up to date
        self.ensure_texture(ctx);

        // Clone data to avoid borrow conflicts in Plot closures
        let counts_for_cursor = self.current_counts().map(<[u64]>::to_vec);
        let spectrum = self.tof_spectrum().map(<[u64]>::to_vec);
        let slicer_enabled = self.ui_state.slicer_enabled;
        let current_tof_bin = self.ui_state.current_tof_bin;
        let show_spectrum = self.ui_state.show_histogram;
        let n_bins = self.n_tof_bins();

        // Get data bounds based on view mode
        // TODO: Neutron mode may have different bounds due to super-resolution
        #[allow(clippy::match_same_arms)]
        let data_size: f64 = match self.ui_state.view_mode {
            ViewMode::Hits => 512.0,
            ViewMode::Neutrons => 512.0,
        };

        // Track if bin changed via interaction
        let mut new_tof_bin: Option<usize> = None;

        egui::CentralPanel::default()
            .frame(
                egui::Frame::none()
                    .fill(colors.bg_dark)
                    .inner_margin(egui::Margin::same(16.0)),
            )
            .show(ctx, |ui| {
                // Calculate available height for layout
                let available_height = ui.available_height();
                let slicer_height = if slicer_enabled && n_bins > 0 {
                    48.0
                } else {
                    0.0
                };
                let spectrum_height = if show_spectrum { 220.0 } else { 0.0 };
                let image_height = available_height - slicer_height - spectrum_height - 8.0;

                // Main 2D histogram view with colorbar
                ui.allocate_ui_with_layout(
                    egui::vec2(ui.available_width(), image_height.max(200.0)),
                    egui::Layout::left_to_right(egui::Align::TOP),
                    |ui| {
                        // Main plot area (takes most of the width)
                        let plot_width = ui.available_width() - 60.0; // Reserve space for colorbar
                        ui.allocate_ui_with_layout(
                            egui::vec2(plot_width, ui.available_height()),
                            egui::Layout::top_down(egui::Align::LEFT),
                            |ui| {
                                let colors = ThemeColors::from_ui(ui);
                                if let Some(tex) = &self.texture {
                                    let half = data_size / 2.0;
                                    #[allow(clippy::cast_possible_truncation)]
                                    let data_size_f32 = data_size as f32;
                                    Plot::new("plot")
                                        .data_aspect(1.0)
                                        .auto_bounds(Vec2b::new(false, false))
                                        .include_x(0.0)
                                        .include_x(data_size)
                                        .include_y(0.0)
                                        .include_y(data_size)
                                        .x_axis_label("X (pixels)")
                                        .y_axis_label("Y (pixels)")
                                        .show(ui, |plot_ui| {
                                            // Set initial bounds to match data dimensions
                                            if plot_ui.response().double_clicked() {
                                                plot_ui.set_plot_bounds(PlotBounds::from_min_max(
                                                    [0.0, 0.0],
                                                    [data_size, data_size],
                                                ));
                                            }
                                            plot_ui.image(PlotImage::new(
                                                tex,
                                                PlotPoint::new(half, half),
                                                [data_size_f32, data_size_f32],
                                            ));

                                            if let Some(curr) = plot_ui.pointer_coordinate() {
                                                let x = curr.x;
                                                let y = curr.y;
                                                if x >= 0.0
                                                    && y >= 0.0
                                                    && x < data_size
                                                    && y < data_size
                                                {
                                                    #[allow(
                                                        clippy::cast_possible_truncation,
                                                        clippy::cast_sign_loss
                                                    )]
                                                    let bound = data_size as usize;
                                                    let (Some(xi), Some(yi)) = (
                                                        f64_to_usize_bounded(x, bound),
                                                        f64_to_usize_bounded(y, bound),
                                                    ) else {
                                                        self.cursor_info = None;
                                                        return;
                                                    };
                                                    let count = counts_for_cursor
                                                        .as_ref()
                                                        .map_or(0, |c| c[yi * 512 + xi]);
                                                    self.cursor_info = Some((xi, yi, count));
                                                } else {
                                                    self.cursor_info = None;
                                                }
                                            } else {
                                                self.cursor_info = None;
                                            }
                                        });
                                } else {
                                    // "No Data" placeholder - use theme-aware background
                                    let no_data_bg =
                                        if colors.bg_dark == super::theme::dark::BG_DARK {
                                            Color32::from_rgb(0x0d, 0x0d, 0x0d)
                                        } else {
                                            Color32::from_rgb(0xe8, 0xe8, 0xe8)
                                        };
                                    egui::Frame::none()
                                        .fill(no_data_bg)
                                        .stroke(Stroke::new(1.0, colors.border))
                                        .rounding(Rounding::same(4.0))
                                        .show(ui, |ui| {
                                            ui.set_min_size(ui.available_size());
                                            ui.centered_and_justified(|ui| {
                                                ui.label(
                                                    egui::RichText::new("No Data")
                                                        .size(14.0)
                                                        .color(colors.text_dim),
                                                );
                                            });
                                        });
                                }
                            },
                        );

                        // Colorbar (right side)
                        ui.add_space(8.0);
                        self.render_colorbar(ui);
                    },
                );

                // TOF Slicer (below image)
                if slicer_enabled && n_bins > 0 {
                    let colors = ThemeColors::from_ui(ui);
                    ui.add_space(8.0);
                    egui::Frame::none()
                        .fill(colors.bg_panel)
                        .stroke(Stroke::new(1.0, colors.border))
                        .rounding(Rounding::same(4.0))
                        .inner_margin(egui::Margin::symmetric(16.0, 12.0))
                        .show(ui, |ui| {
                            let colors = ThemeColors::from_ui(ui);
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new("TOF Slice")
                                        .size(11.0)
                                        .color(colors.text_muted),
                                );

                                ui.add_space(16.0);

                                // Clamp to valid range
                                let clamped_bin = current_tof_bin.min(n_bins - 1);
                                let mut bin = clamped_bin;
                                let slider = ui.add(
                                    egui::Slider::new(&mut bin, 0..=(n_bins - 1))
                                        .show_value(false)
                                        .clamping(egui::SliderClamping::Always),
                                );

                                ui.add_space(16.0);

                                ui.label(
                                    egui::RichText::new(format!("{} / {}", bin + 1, n_bins))
                                        .size(11.0)
                                        .color(colors.text_primary),
                                );

                                if slider.changed() && bin != current_tof_bin {
                                    new_tof_bin = Some(bin);
                                }
                            });
                        });
                }

                // Spectrum viewer (at bottom)
                if show_spectrum {
                    ui.add_space(8.0);
                    self.render_spectrum_panel(
                        ui,
                        &spectrum,
                        slicer_enabled,
                        current_tof_bin,
                        n_bins,
                        &mut new_tof_bin,
                    );
                }
            });

        // Update slicer state if bin changed
        if let Some(bin) = new_tof_bin {
            self.ui_state.current_tof_bin = bin;
            self.texture = None;
        }
    }

    /// Render the colorbar legend.
    #[allow(clippy::cast_precision_loss)]
    fn render_colorbar(&self, ui: &mut egui::Ui) {
        let colors = ThemeColors::from_ui(ui);
        ui.vertical(|ui| {
            // Gradient bar
            let gradient_height = ui.available_height() - 40.0;
            let rect = ui.allocate_space(egui::vec2(20.0, gradient_height.max(100.0)));

            // Draw gradient using the current colormap
            let painter = ui.painter();
            let steps = 64;
            let step_height = rect.1.height() / steps as f32;

            for i in 0..steps {
                let t = 1.0 - (i as f32 / steps as f32); // Flip for max at top
                let color = self.colormap.color_at(t);
                let y_start = rect.1.top() + i as f32 * step_height;
                painter.rect_filled(
                    egui::Rect::from_min_size(
                        egui::pos2(rect.1.left(), y_start),
                        egui::vec2(rect.1.width(), step_height + 1.0),
                    ),
                    0.0,
                    color,
                );
            }

            // Border
            painter.rect_stroke(rect.1, Rounding::ZERO, Stroke::new(1.0, colors.border));

            // Labels
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                let colors = ThemeColors::from_ui(ui);
                ui.add_space(2.0);
                ui.label(egui::RichText::new("max").size(9.0).color(colors.text_dim));
            });
            ui.add_space(gradient_height - 30.0);
            ui.horizontal(|ui| {
                let colors = ThemeColors::from_ui(ui);
                ui.add_space(2.0);
                ui.label(egui::RichText::new("0").size(9.0).color(colors.text_dim));
            });
        });
    }

    /// Render the spectrum panel with toolbar.
    #[allow(
        clippy::too_many_arguments,
        clippy::similar_names,
        clippy::too_many_lines,
        clippy::ref_option
    )]
    fn render_spectrum_panel(
        &mut self,
        ui: &mut egui::Ui,
        spectrum: &Option<Vec<u64>>,
        slicer_enabled: bool,
        current_tof_bin: usize,
        _n_bins: usize,
        new_tof_bin: &mut Option<usize>,
    ) {
        let colors = ThemeColors::from_ui(ui);

        // Spectrum toolbar
        egui::Frame::none()
            .fill(colors.bg_panel)
            .stroke(Stroke::new(1.0, colors.border))
            .rounding(Rounding::same(4.0))
            .inner_margin(egui::Margin::symmetric(12.0, 8.0))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let colors = ThemeColors::from_ui(ui);
                    // TOF unit selector (placeholder for now)
                    egui::ComboBox::from_id_salt("tof_unit")
                        .selected_text("TOF (µs)")
                        .width(90.0)
                        .show_ui(ui, |ui| {
                            let _ = ui.selectable_label(true, "TOF (µs)");
                            let _ = ui.selectable_label(false, "Energy (eV)");
                        });

                    // Settings button
                    ui.add(
                        egui::Button::new("⚙")
                            .fill(Color32::TRANSPARENT)
                            .stroke(Stroke::new(1.0, colors.border_light))
                            .rounding(Rounding::same(4.0)),
                    );

                    ui.add_space(8.0);
                    self.toolbar_divider(ui);
                    ui.add_space(8.0);

                    // logX button
                    let log_x_btn =
                        egui::Button::new(egui::RichText::new("logX").size(10.0).strong().color(
                            if self.ui_state.log_x {
                                Color32::WHITE
                            } else {
                                colors.text_muted
                            },
                        ))
                        .fill(if self.ui_state.log_x {
                            accent::BLUE
                        } else {
                            Color32::TRANSPARENT
                        })
                        .stroke(Stroke::new(1.0, colors.border_light))
                        .rounding(Rounding::same(4.0));

                    if ui.add(log_x_btn).clicked() {
                        self.ui_state.log_x = !self.ui_state.log_x;
                    }

                    // logY button
                    let log_y_btn =
                        egui::Button::new(egui::RichText::new("logY").size(10.0).strong().color(
                            if self.ui_state.log_plot {
                                Color32::WHITE
                            } else {
                                colors.text_muted
                            },
                        ))
                        .fill(if self.ui_state.log_plot {
                            accent::BLUE
                        } else {
                            Color32::TRANSPARENT
                        })
                        .stroke(Stroke::new(1.0, colors.border_light))
                        .rounding(Rounding::same(4.0));

                    if ui.add(log_y_btn).clicked() {
                        self.ui_state.log_plot = !self.ui_state.log_plot;
                    }

                    ui.add_space(8.0);
                    self.toolbar_divider(ui);
                    ui.add_space(8.0);

                    // Export buttons
                    if ui
                        .add(
                            egui::Button::new(
                                egui::RichText::new("📷 PNG")
                                    .size(10.0)
                                    .color(colors.text_muted),
                            )
                            .fill(Color32::TRANSPARENT)
                            .stroke(Stroke::new(1.0, colors.border_light))
                            .rounding(Rounding::same(4.0)),
                        )
                        .on_hover_text("Export spectrum as PNG")
                        .clicked()
                    {
                        // TODO: Export PNG
                    }

                    if ui
                        .add(
                            egui::Button::new(
                                egui::RichText::new("💾 CSV")
                                    .size(10.0)
                                    .color(colors.text_muted),
                            )
                            .fill(Color32::TRANSPARENT)
                            .stroke(Stroke::new(1.0, colors.border_light))
                            .rounding(Rounding::same(4.0)),
                        )
                        .on_hover_text("Export spectrum as CSV")
                        .clicked()
                    {
                        // TODO: Export CSV
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let colors = ThemeColors::from_ui(ui);
                        // Legend
                        ui.horizontal(|ui| {
                            let colors = ThemeColors::from_ui(ui);
                            // ROI 1 legend (placeholder)
                            ui.add(self.legend_box(accent::BLUE));
                            ui.label(
                                egui::RichText::new("ROI 1")
                                    .size(10.0)
                                    .color(colors.text_muted),
                            );

                            ui.add_space(8.0);

                            // Full legend
                            ui.add(self.legend_box(colors.text_muted));
                            ui.label(
                                egui::RichText::new("Full")
                                    .size(10.0)
                                    .color(colors.text_muted),
                            );
                        });

                        ui.add_space(16.0);

                        // Reset button
                        if ui
                            .add(
                                egui::Button::new(
                                    egui::RichText::new("↺ Reset")
                                        .size(10.0)
                                        .color(colors.text_muted),
                                )
                                .fill(Color32::TRANSPARENT)
                                .stroke(Stroke::new(1.0, colors.border_light))
                                .rounding(Rounding::same(4.0)),
                            )
                            .clicked()
                        {
                            // Reset view
                        }
                    });
                });
            });

        ui.add_space(4.0);

        // Spectrum plot
        if let Some(full) = spectrum.as_ref() {
            let colors = ThemeColors::from_ui(ui);
            let tdc_period = 1.0 / self.tdc_frequency;
            let max_us = tdc_period * 1e6;
            let spec_bins = full.len();
            let bin_width_us = max_us / usize_to_f64(spec_bins);

            let line_color = colors.text_muted;
            let plot_response = Plot::new("spectrum")
                .height(140.0)
                .x_axis_label("TOF (µs)")
                .y_axis_label(if self.ui_state.log_plot {
                    "log₁₀(Counts)"
                } else {
                    "Counts"
                })
                .include_x(0.0)
                .include_x(max_us)
                .include_y(0.0)
                .show(ui, |plot_ui| {
                    // Full spectrum as line
                    let points: Vec<[f64; 2]> = full
                        .iter()
                        .enumerate()
                        .map(|(i, &c)| {
                            let x = if self.ui_state.log_x && i > 0 {
                                (usize_to_f64(i) * bin_width_us).log10()
                            } else {
                                usize_to_f64(i) * bin_width_us
                            };
                            let y = if self.ui_state.log_plot {
                                if c > 0 {
                                    u64_to_f64(c).log10()
                                } else {
                                    0.0
                                }
                            } else {
                                u64_to_f64(c)
                            };
                            [x, y]
                        })
                        .collect();

                    plot_ui.line(
                        Line::new(PlotPoints::new(points))
                            .color(line_color)
                            .name("Full"),
                    );

                    // Slice marker
                    if slicer_enabled && current_tof_bin < spec_bins {
                        let slice_x = if self.ui_state.log_x && current_tof_bin > 0 {
                            (usize_to_f64(current_tof_bin) * bin_width_us).log10()
                        } else {
                            usize_to_f64(current_tof_bin) * bin_width_us
                        };
                        plot_ui.vline(
                            VLine::new(slice_x)
                                .color(accent::RED)
                                .width(1.0)
                                .style(egui_plot::LineStyle::Dashed { length: 4.0 })
                                .name(format!("Slice {}", current_tof_bin + 1)),
                        );
                    }

                    // Handle drag
                    if slicer_enabled && spec_bins > 0 {
                        let drag_delta = plot_ui.pointer_coordinate_drag_delta();
                        if drag_delta.x.abs() > 0.0 {
                            if let Some(coord) = plot_ui.pointer_coordinate() {
                                let x_us = if self.ui_state.log_x {
                                    10f64.powf(coord.x)
                                } else {
                                    coord.x
                                };
                                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                                let bin = if x_us <= 0.0 {
                                    0
                                } else if x_us >= max_us {
                                    spec_bins - 1
                                } else {
                                    ((x_us / bin_width_us) as usize).min(spec_bins - 1)
                                };
                                if bin != current_tof_bin {
                                    *new_tof_bin = Some(bin);
                                }
                            }
                        }
                    }
                });

            // Click to set position
            if slicer_enabled && spec_bins > 0 && plot_response.response.clicked() {
                if let Some(pos) = plot_response.response.interact_pointer_pos() {
                    let plot_bounds = plot_response.transform.bounds();
                    let plot_rect = plot_response.response.rect;
                    let x_frac = f64::from(pos.x - plot_rect.left()) / f64::from(plot_rect.width());
                    let x_plot = plot_bounds.min()[0]
                        + x_frac * (plot_bounds.max()[0] - plot_bounds.min()[0]);
                    let x_us = if self.ui_state.log_x {
                        10f64.powf(x_plot)
                    } else {
                        x_plot
                    };

                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    let bin = if x_us <= 0.0 {
                        0
                    } else if x_us >= max_us {
                        spec_bins - 1
                    } else {
                        ((x_us / bin_width_us) as usize).min(spec_bins - 1)
                    };
                    if bin != current_tof_bin {
                        *new_tof_bin = Some(bin);
                    }
                }
            }
        } else {
            let colors = ThemeColors::from_ui(ui);
            // "No Data" placeholder - use theme-aware background
            let no_data_bg = if colors.bg_dark == super::theme::dark::BG_DARK {
                Color32::from_rgb(0x0d, 0x0d, 0x0d)
            } else {
                Color32::from_rgb(0xe8, 0xe8, 0xe8)
            };
            egui::Frame::none()
                .fill(no_data_bg)
                .stroke(Stroke::new(1.0, colors.border))
                .rounding(Rounding::same(4.0))
                .inner_margin(egui::Margin::same(16.0))
                .show(ui, |ui| {
                    let colors = ThemeColors::from_ui(ui);
                    ui.set_min_height(140.0);
                    ui.centered_and_justified(|ui| {
                        ui.label(
                            egui::RichText::new("Spectrum: No Data")
                                .size(11.0)
                                .color(colors.text_dim),
                        );
                    });
                });
        }
    }

    /// Render a toolbar divider.
    #[allow(clippy::unused_self)]
    fn toolbar_divider(&self, ui: &mut egui::Ui) {
        let colors = ThemeColors::from_ui(ui);
        let rect = ui.allocate_space(egui::vec2(1.0, 20.0));
        ui.painter().vline(
            rect.1.center().x,
            rect.1.y_range(),
            Stroke::new(1.0, colors.border),
        );
    }

    /// Create a legend box widget.
    #[allow(clippy::unused_self)]
    fn legend_box(&self, color: Color32) -> impl egui::Widget + '_ {
        move |ui: &mut egui::Ui| {
            let (rect, response) =
                ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
            ui.painter().rect_filled(rect, Rounding::same(2.0), color);
            response
        }
    }
}
