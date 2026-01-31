//! Control panel (left sidebar) and top/bottom bars rendering.

use eframe::egui::{self, Color32, Rounding, Stroke};
use rfd::FileDialog;

use super::theme::{accent, form_label, primary_button, ThemeColors};
use crate::app::RustpixApp;
use crate::pipeline::AlgorithmType;
use crate::state::ViewMode;
use crate::util::format_number;
use crate::viewer::Colormap;

impl RustpixApp {
    /// Render the top panel with RUSTPIX branding, file info, and view mode toggle.
    pub(crate) fn render_top_panel(&mut self, ctx: &egui::Context) {
        let colors = ThemeColors::from_ctx(ctx);

        egui::TopBottomPanel::top("top_bar")
            .frame(
                egui::Frame::none()
                    .fill(colors.bg_header)
                    .inner_margin(egui::Margin {
                        left: 16.0,
                        right: 16.0,
                        top: 8.0,
                        bottom: 8.0,
                    }),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let colors = ThemeColors::from_ui(ui);
                    // RUSTPIX branding
                    ui.label(
                        egui::RichText::new("RUSTPIX")
                            .size(14.0)
                            .strong()
                            .color(accent::BLUE),
                    );

                    ui.label(egui::RichText::new("│").size(14.0).color(colors.text_dim));

                    // File name
                    if let Some(p) = &self.selected_file {
                        ui.label(
                            egui::RichText::new(
                                p.file_name().unwrap_or_default().to_string_lossy(),
                            )
                            .size(11.0)
                            .color(colors.text_muted),
                        );
                    } else {
                        ui.label(
                            egui::RichText::new("No file loaded")
                                .size(11.0)
                                .color(colors.text_dim),
                        );
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let colors = ThemeColors::from_ui(ui);
                        // Settings gear icon (hyperstack settings)
                        if ui
                            .add(
                                egui::Button::new("⚙")
                                    .fill(Color32::TRANSPARENT)
                                    .stroke(Stroke::new(1.0, colors.border_light))
                                    .rounding(Rounding::same(4.0)),
                            )
                            .on_hover_text("Hyperstack settings")
                            .clicked()
                        {
                            self.ui_state.show_app_settings = !self.ui_state.show_app_settings;
                        }

                        ui.add_space(12.0);

                        // HITS/NEUTRONS toggle buttons
                        self.render_view_mode_toggle(ui);
                    });
                });
            });
    }

    /// Render the HITS/NEUTRONS toggle button group.
    fn render_view_mode_toggle(&mut self, ui: &mut egui::Ui) {
        let colors = ThemeColors::from_ui(ui);
        let old_mode = self.ui_state.view_mode;

        // Container frame for the toggle group
        egui::Frame::none()
            .fill(colors.bg_dark)
            .stroke(Stroke::new(1.0, colors.border))
            .rounding(Rounding::same(4.0))
            .inner_margin(egui::Margin::same(2.0))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let colors = ThemeColors::from_ui(ui);
                    ui.spacing_mut().item_spacing = egui::vec2(2.0, 0.0);

                    // HITS button
                    let hits_active = self.ui_state.view_mode == ViewMode::Hits;
                    let hits_btn =
                        egui::Button::new(egui::RichText::new("HITS").size(11.0).strong().color(
                            if hits_active {
                                Color32::WHITE
                            } else {
                                colors.text_muted
                            },
                        ))
                        .fill(if hits_active {
                            accent::BLUE
                        } else {
                            Color32::TRANSPARENT
                        })
                        .stroke(Stroke::NONE)
                        .rounding(Rounding::same(3.0))
                        .min_size(egui::vec2(70.0, 0.0));

                    if ui.add(hits_btn).clicked() {
                        self.ui_state.view_mode = ViewMode::Hits;
                    }

                    // NEUTRONS button
                    let neutrons_active = self.ui_state.view_mode == ViewMode::Neutrons;
                    let neutrons_enabled = self.has_neutrons();
                    let neutrons_btn = egui::Button::new(
                        egui::RichText::new("NEUTRONS").size(11.0).strong().color(
                            if neutrons_active {
                                Color32::WHITE
                            } else if neutrons_enabled {
                                colors.text_muted
                            } else {
                                colors.text_dim
                            },
                        ),
                    )
                    .fill(if neutrons_active {
                        accent::GREEN
                    } else {
                        Color32::TRANSPARENT
                    })
                    .stroke(Stroke::NONE)
                    .rounding(Rounding::same(3.0))
                    .min_size(egui::vec2(90.0, 0.0));

                    if ui.add_enabled(neutrons_enabled, neutrons_btn).clicked() {
                        self.ui_state.view_mode = ViewMode::Neutrons;
                    }
                });
            });

        if self.ui_state.view_mode != old_mode {
            self.texture = None;
            self.ui_state.current_tof_bin = 0;
        }
    }

    /// Render the bottom status bar.
    pub(crate) fn render_bottom_panel(&self, ctx: &egui::Context) {
        let colors = ThemeColors::from_ctx(ctx);

        egui::TopBottomPanel::bottom("status_bar")
            .frame(
                egui::Frame::none()
                    .fill(colors.bg_header)
                    .inner_margin(egui::Margin {
                        left: 16.0,
                        right: 16.0,
                        top: 6.0,
                        bottom: 6.0,
                    }),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let colors = ThemeColors::from_ui(ui);
                    // Status indicator
                    let (status_color, status_text) =
                        if self.processing.is_loading || self.processing.is_processing {
                            (accent::BLUE, &self.processing.status_text)
                        } else {
                            (accent::GREEN, &"Ready".to_string())
                        };

                    ui.label(egui::RichText::new("●").size(11.0).color(status_color));
                    ui.label(
                        egui::RichText::new(status_text)
                            .size(11.0)
                            .color(status_color),
                    );

                    ui.label(egui::RichText::new("│").size(11.0).color(colors.text_dim));

                    // Cursor info
                    if let Some((x, y, count)) = self.cursor_info {
                        ui.label(
                            egui::RichText::new(format!("Cursor: ({x}, {y}) = "))
                                .size(11.0)
                                .color(colors.text_muted),
                        );
                        #[allow(clippy::cast_possible_truncation)]
                        let count_usize = count as usize;
                        ui.label(
                            egui::RichText::new(format_number(count_usize))
                                .size(11.0)
                                .color(colors.text_primary),
                        );
                        ui.label(
                            egui::RichText::new(" counts")
                                .size(11.0)
                                .color(colors.text_muted),
                        );
                    } else {
                        ui.label(
                            egui::RichText::new("Cursor: -")
                                .size(11.0)
                                .color(colors.text_muted),
                        );
                    }

                    if let Some((message, expires_at)) = &self.ui_state.roi_warning {
                        let now = ctx.input(|i| i.time);
                        if now <= *expires_at {
                            ui.label(egui::RichText::new("│").size(11.0).color(colors.text_dim));
                            ui.label(egui::RichText::new(message).size(11.0).color(accent::RED));
                        }
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let colors = ThemeColors::from_ui(ui);
                        // Hot pixel count (placeholder)
                        ui.label(egui::RichText::new("0").size(11.0).color(accent::RED));
                        ui.label(
                            egui::RichText::new("Hot: ")
                                .size(11.0)
                                .color(colors.text_muted),
                        );

                        ui.add_space(8.0);

                        // Dead pixel count (placeholder)
                        ui.label(egui::RichText::new("0").size(11.0).color(colors.text_dim));
                        ui.label(
                            egui::RichText::new("Dead: ")
                                .size(11.0)
                                .color(colors.text_muted),
                        );
                    });
                });
            });
    }

    /// Render the left control panel.
    pub(crate) fn render_side_panel(&mut self, ctx: &egui::Context) {
        let colors = ThemeColors::from_ctx(ctx);

        egui::SidePanel::left("ctrl")
            .default_width(240.0)
            .frame(
                egui::Frame::none()
                    .fill(colors.bg_panel)
                    .inner_margin(egui::Margin::ZERO),
            )
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        // Statistics section
                        self.render_section(ui, "Statistics", true, |app, ui| {
                            app.render_statistics(ui);
                        });

                        // Clustering section
                        self.render_section(ui, "Clustering", true, |app, ui| {
                            app.render_clustering_controls(ui);
                        });

                        // View section
                        self.render_section(ui, "View", true, |app, ui| {
                            app.render_view_options(ui);
                        });

                        // Pixel Health section (placeholder)
                        self.render_section(ui, "Pixel Health", false, |_app, ui| {
                            let colors = ThemeColors::from_ui(ui);
                            ui.label(
                                egui::RichText::new("Coming soon...")
                                    .size(11.0)
                                    .color(colors.text_dim),
                            );
                        });

                        // Export section (placeholder)
                        self.render_section(ui, "Export", false, |_app, ui| {
                            let colors = ThemeColors::from_ui(ui);
                            ui.label(
                                egui::RichText::new("Coming soon...")
                                    .size(11.0)
                                    .color(colors.text_dim),
                            );
                        });

                        // Progress indicator (when active)
                        self.render_progress_status(ui);

                        // Open File button
                        ui.separator();
                        egui::Frame::none()
                            .inner_margin(egui::Margin::symmetric(12.0, 12.0))
                            .show(ui, |ui| {
                                self.render_file_controls(ui);
                            });
                    });
            });
    }

    /// Render a collapsible section with header.
    fn render_section<F>(&mut self, ui: &mut egui::Ui, title: &str, default_open: bool, content: F)
    where
        F: FnOnce(&mut Self, &mut egui::Ui),
    {
        // Section container
        ui.push_id(title, |ui| {
            let colors = ThemeColors::from_ui(ui);
            // Header
            let header_response = ui.add(
                egui::Button::new(
                    egui::RichText::new(title.to_uppercase())
                        .size(11.0)
                        .strong()
                        .color(colors.text_primary),
                )
                .fill(Color32::TRANSPARENT)
                .stroke(Stroke::NONE)
                .rounding(Rounding::ZERO)
                .min_size(egui::vec2(ui.available_width(), 0.0)),
            );

            // Get/toggle state
            let id = ui.make_persistent_id(format!("{title}_open"));
            let mut is_open = ui.data_mut(|d| *d.get_temp_mut_or_insert_with(id, || default_open));

            if header_response.clicked() {
                is_open = !is_open;
                ui.data_mut(|d| d.insert_temp(id, is_open));
            }

            // Draw the header with proper styling
            let header_rect = header_response.rect;
            ui.painter()
                .rect_filled(header_rect, 0.0, Color32::TRANSPARENT);

            // Arrow indicator
            let arrow = if is_open { "▼" } else { "▶" };
            let arrow_pos = header_rect.right_center() - egui::vec2(20.0, 0.0);
            ui.painter().text(
                arrow_pos,
                egui::Align2::CENTER_CENTER,
                arrow,
                egui::FontId::monospace(10.0),
                colors.text_dim,
            );

            // Separator line
            ui.painter().hline(
                header_rect.x_range(),
                header_rect.bottom(),
                Stroke::new(1.0, colors.border),
            );

            // Content
            if is_open {
                egui::Frame::none()
                    .inner_margin(egui::Margin {
                        left: 16.0,
                        right: 16.0,
                        top: 12.0,
                        bottom: 16.0,
                    })
                    .show(ui, |ui| {
                        content(self, ui);
                    });
            }

            // Bottom border
            let last_rect = ui.min_rect();
            ui.painter().hline(
                last_rect.x_range(),
                last_rect.bottom(),
                Stroke::new(1.0, colors.border),
            );
        });
    }

    fn render_file_controls(&mut self, ui: &mut egui::Ui) {
        let colors = ThemeColors::from_ui(ui);
        let can_load = !self.processing.is_loading && !self.processing.is_processing;

        // Theme-aware secondary button
        let btn = egui::Button::new(egui::RichText::new("Open File...").color(colors.text_primary))
            .fill(colors.button_bg)
            .stroke(Stroke::new(1.0, colors.border_light))
            .rounding(Rounding::same(4.0))
            .min_size(egui::vec2(ui.available_width(), 0.0));

        if ui.add_enabled(can_load, btn).clicked() {
            if let Some(path) = FileDialog::new().add_filter("TPX3", &["tpx3"]).pick_file() {
                self.load_file(path);
            }
        }
    }

    fn render_progress_status(&mut self, ui: &mut egui::Ui) {
        let is_busy = self.processing.is_loading || self.processing.is_processing;

        if is_busy {
            egui::Frame::none()
                .inner_margin(egui::Margin::symmetric(12.0, 8.0))
                .show(ui, |ui| {
                    ui.add(
                        egui::ProgressBar::new(self.processing.progress)
                            .text(&self.processing.status_text),
                    );
                    if ui.button("Cancel").clicked() {
                        self.cancel_operation();
                    }
                });
        }
    }

    /// Render view options (colormap, toggles).
    fn render_view_options(&mut self, ui: &mut egui::Ui) {
        // Colormap selection
        ui.label(form_label("Colormap"));
        ui.add_space(4.0);

        egui::ComboBox::from_id_salt("colormap_select")
            .selected_text(self.colormap.to_string())
            .width(ui.available_width() - 8.0)
            .show_ui(ui, |ui| {
                for cmap in [
                    Colormap::Grayscale,
                    Colormap::Green,
                    Colormap::Hot,
                    Colormap::Viridis,
                ] {
                    if ui
                        .selectable_value(&mut self.colormap, cmap, cmap.to_string())
                        .clicked()
                    {
                        self.texture = None;
                    }
                }
            });

        ui.add_space(12.0);

        // Checkboxes
        let n_bins = self.n_tof_bins();
        ui.add_enabled_ui(n_bins > 0, |ui| {
            if ui
                .checkbox(&mut self.ui_state.slicer_enabled, "TOF Slicer")
                .changed()
            {
                self.texture = None;
            }
        });

        ui.checkbox(&mut self.ui_state.show_histogram, "Spectrum");

        if ui
            .checkbox(&mut self.ui_state.log_scale, "Log scale")
            .changed()
        {
            self.texture = None;
        }
    }

    /// Regenerate texture if needed.
    pub(crate) fn ensure_texture(&mut self, ctx: &egui::Context) {
        let has_data = match self.ui_state.view_mode {
            ViewMode::Hits => self.hit_counts.is_some(),
            ViewMode::Neutrons => self.neutron_counts.is_some(),
        };
        if self.texture.is_none() && has_data {
            let img = self.generate_histogram();
            self.texture = Some(ctx.load_texture("hist", img, egui::TextureOptions::NEAREST));
        }
    }

    #[allow(clippy::too_many_lines)]
    fn render_clustering_controls(&mut self, ui: &mut egui::Ui) {
        let colors = ThemeColors::from_ui(ui);

        // Algorithm selection
        ui.label(form_label("Algorithm"));
        ui.add_space(4.0);

        ui.horizontal(|ui| {
            let colors = ThemeColors::from_ui(ui);
            egui::ComboBox::from_id_salt("algo_select")
                .selected_text(self.algo_type.to_string())
                .width(ui.available_width() - 40.0)
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut self.algo_type,
                        AlgorithmType::Abs,
                        "ABS (Adaptive Box Search)",
                    );
                    ui.selectable_value(&mut self.algo_type, AlgorithmType::Dbscan, "DBSCAN");
                    ui.selectable_value(&mut self.algo_type, AlgorithmType::Grid, "Grid");
                });

            // Settings button for advanced options
            if ui
                .add(
                    egui::Button::new("⚙")
                        .fill(Color32::TRANSPARENT)
                        .stroke(Stroke::new(1.0, colors.border_light))
                        .rounding(Rounding::same(4.0)),
                )
                .on_hover_text("Algorithm parameters")
                .clicked()
            {
                self.ui_state.show_clustering_params = !self.ui_state.show_clustering_params;
            }
        });

        // Advanced parameters (collapsible)
        if self.ui_state.show_clustering_params {
            ui.add_space(8.0);
            egui::Frame::none()
                .fill(colors.bg_header)
                .stroke(Stroke::new(1.0, colors.border))
                .rounding(Rounding::same(4.0))
                .inner_margin(egui::Margin::same(12.0))
                .show(ui, |ui| {
                    // TDC Frequency
                    ui.horizontal(|ui| {
                        let colors = ThemeColors::from_ui(ui);
                        ui.label(
                            egui::RichText::new("TDC Freq")
                                .size(10.0)
                                .color(colors.text_muted),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let colors = ThemeColors::from_ui(ui);
                            ui.label(
                                egui::RichText::new(format!("{:.0} Hz", self.tdc_frequency))
                                    .size(10.0)
                                    .color(colors.text_primary),
                            );
                        });
                    });
                    ui.add(
                        egui::Slider::new(&mut self.tdc_frequency, 1.0..=120.0).show_value(false),
                    );

                    ui.add_space(4.0);

                    // Radius
                    ui.horizontal(|ui| {
                        let colors = ThemeColors::from_ui(ui);
                        ui.label(
                            egui::RichText::new("Radius")
                                .size(10.0)
                                .color(colors.text_muted),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let colors = ThemeColors::from_ui(ui);
                            ui.label(
                                egui::RichText::new(format!("{:.0} px", self.radius))
                                    .size(10.0)
                                    .color(colors.text_primary),
                            );
                        });
                    });
                    ui.add(egui::Slider::new(&mut self.radius, 1.0..=50.0).show_value(false));

                    ui.add_space(4.0);

                    // Time window
                    ui.horizontal(|ui| {
                        let colors = ThemeColors::from_ui(ui);
                        ui.label(
                            egui::RichText::new("Time window")
                                .size(10.0)
                                .color(colors.text_muted),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let colors = ThemeColors::from_ui(ui);
                            ui.label(
                                egui::RichText::new(format!("{:.0} ns", self.temporal_window_ns))
                                    .size(10.0)
                                    .color(colors.text_primary),
                            );
                        });
                    });
                    ui.add(
                        egui::Slider::new(&mut self.temporal_window_ns, 10.0..=500.0)
                            .show_value(false),
                    );

                    ui.add_space(4.0);

                    // Min cluster
                    ui.horizontal(|ui| {
                        let colors = ThemeColors::from_ui(ui);
                        ui.label(
                            egui::RichText::new("Min cluster")
                                .size(10.0)
                                .color(colors.text_muted),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let colors = ThemeColors::from_ui(ui);
                            ui.label(
                                egui::RichText::new(format!("{}", self.min_cluster_size))
                                    .size(10.0)
                                    .color(colors.text_primary),
                            );
                        });
                    });
                    ui.add(egui::Slider::new(&mut self.min_cluster_size, 1..=10).show_value(false));

                    // DBSCAN-specific
                    if self.algo_type == AlgorithmType::Dbscan {
                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            let colors = ThemeColors::from_ui(ui);
                            ui.label(
                                egui::RichText::new("Min points")
                                    .size(10.0)
                                    .color(colors.text_muted),
                            );
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    let colors = ThemeColors::from_ui(ui);
                                    ui.label(
                                        egui::RichText::new(format!("{}", self.dbscan_min_points))
                                            .size(10.0)
                                            .color(colors.text_primary),
                                    );
                                },
                            );
                        });
                        ui.add(
                            egui::Slider::new(&mut self.dbscan_min_points, 1..=10)
                                .show_value(false),
                        );
                    }

                    ui.add_space(4.0);

                    // Super-resolution factor
                    ui.horizontal(|ui| {
                        let colors = ThemeColors::from_ui(ui);
                        ui.label(
                            egui::RichText::new("Super-res")
                                .size(10.0)
                                .color(colors.text_muted),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let colors = ThemeColors::from_ui(ui);
                            ui.label(
                                egui::RichText::new(format!(
                                    "{:.1}×",
                                    self.super_resolution_factor
                                ))
                                .size(10.0)
                                .color(colors.text_primary),
                            );
                        });
                    });
                    ui.add(
                        egui::Slider::new(&mut self.super_resolution_factor, 1.0..=16.0)
                            .show_value(false),
                    );
                });
        }

        ui.add_space(12.0);

        // Run Clustering button
        let can_cluster = !self.processing.is_loading
            && !self.processing.is_processing
            && self.hit_batch.is_some();

        if ui
            .add_enabled(
                can_cluster,
                primary_button("Run Clustering").min_size(egui::vec2(ui.available_width(), 0.0)),
            )
            .clicked()
        {
            self.processing.reset_cancel();
            self.run_processing();
        }
    }

    /// Render floating settings windows (app + spectrum).
    pub(crate) fn render_settings_windows(&mut self, ctx: &egui::Context) {
        if self.ui_state.show_app_settings {
            let mut show_app_settings = self.ui_state.show_app_settings;
            egui::Window::new("Hyperstack Settings")
                .open(&mut show_app_settings)
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.label("Adjust TOF binning for hits and neutrons.");
                    ui.add_space(8.0);

                    egui::CollapsingHeader::new("Hits Hyperstack")
                        .default_open(true)
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label("TOF bins");
                                ui.add(
                                    egui::DragValue::new(&mut self.hit_tof_bins).range(10..=2000),
                                );
                            });

                            let can_rebuild = self.hit_batch.is_some();
                            if ui
                                .add_enabled(can_rebuild, egui::Button::new("Rebuild Hits"))
                                .clicked()
                            {
                                self.rebuild_hit_hyperstack();
                            }
                        });

                    egui::CollapsingHeader::new("Neutrons Hyperstack")
                        .default_open(true)
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label("TOF bins");
                                ui.add(
                                    egui::DragValue::new(&mut self.neutron_tof_bins)
                                        .range(10..=2000),
                                );
                            });

                            let can_rebuild = !self.neutrons.is_empty();
                            if ui
                                .add_enabled(can_rebuild, egui::Button::new("Rebuild Neutrons"))
                                .clicked()
                            {
                                self.rebuild_neutron_hyperstack();
                            }
                        });
                });
            self.ui_state.show_app_settings = show_app_settings;
        }

        if self.ui_state.show_spectrum_settings {
            let mut show_spectrum_settings = self.ui_state.show_spectrum_settings;
            egui::Window::new("Spectrum Settings")
                .open(&mut show_spectrum_settings)
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.label("Energy axis requires flight path and TOF offset.");
                    ui.add_space(8.0);

                    ui.horizontal(|ui| {
                        ui.label("Flight path (m)");
                        ui.add(
                            egui::DragValue::new(&mut self.flight_path_m)
                                .range(0.0..=100.0)
                                .speed(0.1),
                        );
                    });

                    ui.horizontal(|ui| {
                        ui.label("TOF offset (ns)");
                        ui.add(
                            egui::DragValue::new(&mut self.tof_offset_ns)
                                .range(0.0..=1_000_000.0)
                                .speed(10.0),
                        );
                    });
                });
            self.ui_state.show_spectrum_settings = show_spectrum_settings;
        }
    }
}
