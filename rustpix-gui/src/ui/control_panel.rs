//! Control panel (left sidebar) and top/bottom bars rendering.

use eframe::egui::{self, Color32, FontFamily, FontId, Rect, Rounding, Stroke};
use rfd::FileDialog;

use super::theme::{accent, form_label, primary_button, ThemeColors};
use crate::app::{DetectorProfile, DetectorProfileKind, RustpixApp};
use crate::pipeline::AlgorithmType;
use crate::state::{
    ExportFormat, Hdf5ExportOptions, TiffBitDepth, TiffExportOptions, TiffSpectraTiming,
    TiffStackBehavior, ViewMode,
};
use crate::util::{format_bytes, format_number};
use crate::viewer::Colormap;
use rustpix_tpx::{ChipTransform, DetectorConfig};

#[derive(Clone, Copy)]
enum FileToolbarIcon {
    Open,
    Export,
    Gear,
}

#[derive(Clone, Copy)]
enum SectionHelp {
    Clustering,
    View,
    PixelHealth,
}

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
                ui.set_min_height(36.0);
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    let colors = ThemeColors::from_ui(ui);
                    ui.spacing_mut().item_spacing = egui::vec2(10.0, 0.0);

                    self.render_top_bar_left(ui, colors);
                    self.render_top_bar_status(ui, colors);
                    self.render_top_bar_right(ui);
                });
            });
    }

    fn render_top_bar_left(&mut self, ui: &mut egui::Ui, colors: ThemeColors) {
        let can_load = !self.processing.is_loading && !self.processing.is_processing;
        let can_export = self.hit_batch.is_some()
            || !self.neutrons.is_empty()
            || self.hyperstack.is_some()
            || self.neutron_hyperstack.is_some();
        let can_export = can_export && !self.ui_state.export.in_progress;

        ui.label(
            egui::RichText::new("RUSTPIX")
                .size(14.0)
                .strong()
                .color(accent::BLUE),
        );

        Self::top_bar_separator(ui, colors);

        if Self::file_toolbar_button(ui, colors, FileToolbarIcon::Open, can_load, "Open file")
            .clicked()
        {
            if let Some(path) = FileDialog::new().add_filter("TPX3", &["tpx3"]).pick_file() {
                self.load_file(path);
            }
        }

        if Self::file_toolbar_button(
            ui,
            colors,
            FileToolbarIcon::Export,
            can_export,
            "Export data",
        )
        .clicked()
        {
            self.ui_state.export.show_dialog = true;
        }

        Self::top_bar_separator(ui, colors);
    }

    fn render_top_bar_status(&self, ui: &mut egui::Ui, colors: ThemeColors) {
        let (status_text, status_color, status_bold) = self.status_banner_text(colors);
        let right_reserve = 330.0;
        let status_width = (ui.available_width() - right_reserve).max(120.0);
        let status_height = ui.spacing().interact_size.y.max(24.0);
        let (status_rect, status_response) = ui.allocate_exact_size(
            egui::vec2(status_width, status_height),
            egui::Sense::hover(),
        );
        let status_font = if status_bold {
            FontId::new(13.0, FontFamily::Monospace)
        } else {
            FontId::new(12.0, FontFamily::Monospace)
        };
        let galley =
            ui.fonts(|fonts| fonts.layout_no_wrap(status_text.clone(), status_font, status_color));
        let text_width = galley.size().x;
        let text_y = status_rect.center().y - galley.size().y / 2.0;
        let painter = ui.painter().with_clip_rect(status_rect);

        if text_width <= status_rect.width() || !status_response.hovered() {
            let x = status_rect.left();
            painter.galley(egui::pos2(x, text_y), galley.clone(), status_color);
        } else {
            let speed = 30.0;
            let gap = 24.0;
            let scroll_len = text_width + gap;
            let t = ui.input(|i| i.time);
            #[allow(clippy::cast_possible_truncation)]
            let t_f32 = if t.is_finite() { t as f32 } else { f32::MAX };
            let offset = (t_f32 * speed) % scroll_len;
            let x1 = status_rect.left() - offset;
            painter.galley(egui::pos2(x1, text_y), galley.clone(), status_color);
            painter.galley(
                egui::pos2(x1 + scroll_len, text_y),
                galley.clone(),
                status_color,
            );
            ui.ctx().request_repaint();
        }

        status_response.on_hover_text(status_text);
    }

    fn render_top_bar_right(&mut self, ui: &mut egui::Ui) {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let colors = ThemeColors::from_ui(ui);
            ui.spacing_mut().item_spacing = egui::vec2(8.0, 0.0);

            if Self::file_toolbar_button(
                ui,
                colors,
                FileToolbarIcon::Gear,
                true,
                "Hyperstack settings",
            )
            .clicked()
            {
                self.ui_state.panels.show_app_settings = !self.ui_state.panels.show_app_settings;
            }

            self.render_view_mode_toggle(ui);
            self.render_cache_toggle(ui);
        });
    }

    fn status_banner_text(&self, colors: ThemeColors) -> (String, Color32, bool) {
        if let Some(p) = &self.selected_file {
            let name = p.file_name().unwrap_or_default().to_string_lossy();
            if self.statistics.hit_count > 0 {
                (
                    format!(
                        "{} • {} hits",
                        name,
                        format_number(self.statistics.hit_count)
                    ),
                    colors.text_muted,
                    false,
                )
            } else {
                (format!("{name}"), colors.text_muted, false)
            }
        } else {
            ("No file loaded".to_string(), colors.text_primary, true)
        }
    }

    fn top_bar_separator(ui: &mut egui::Ui, colors: ThemeColors) {
        ui.add_space(2.0);
        ui.label(egui::RichText::new("│").size(14.0).color(colors.text_dim));
        ui.add_space(2.0);
    }

    fn file_toolbar_button(
        ui: &mut egui::Ui,
        colors: ThemeColors,
        icon: FileToolbarIcon,
        enabled: bool,
        tooltip: &str,
    ) -> egui::Response {
        let image = Self::file_icon_image(icon, colors.text_primary)
            .fit_to_exact_size(egui::vec2(16.0, 16.0));
        let btn = egui::ImageButton::new(image)
            .frame(true)
            .rounding(Rounding::same(4.0));
        let response = ui
            .add_enabled_ui(enabled, |ui| ui.add_sized(egui::vec2(30.0, 28.0), btn))
            .inner;
        response.on_hover_text(tooltip)
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
                    self.render_status_indicator(ui);
                    Self::status_separator(ui, colors);
                    self.render_cursor_status(ui, colors);
                    self.render_roi_messages(ui, ctx, colors);
                    self.render_export_status(ui, colors);
                    self.render_bottom_right(ui);
                });
            });
    }

    fn render_status_indicator(&self, ui: &mut egui::Ui) {
        let (status_color, status_text) =
            if self.processing.is_loading || self.processing.is_processing {
                (accent::BLUE, self.processing.status_text.as_str())
            } else {
                (accent::GREEN, "Ready")
            };
        ui.label(egui::RichText::new("●").size(11.0).color(status_color));
        ui.label(
            egui::RichText::new(status_text)
                .size(11.0)
                .color(status_color),
        );
    }

    fn render_cursor_status(&self, ui: &mut egui::Ui, colors: ThemeColors) {
        if let Some((x, y, count)) = self.cursor_info {
            ui.label(
                egui::RichText::new(format!("Cursor: ({x}, {y}) = "))
                    .size(11.0)
                    .color(colors.text_muted),
            );
            let count_usize = usize::try_from(count).unwrap_or(usize::MAX);
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
    }

    fn render_roi_messages(&self, ui: &mut egui::Ui, ctx: &egui::Context, colors: ThemeColors) {
        if let Some((message, expires_at)) = &self.ui_state.roi_status {
            let now = ctx.input(|i| i.time);
            if now <= *expires_at {
                Self::status_separator(ui, colors);
                ui.label(egui::RichText::new(message).size(11.0).color(accent::BLUE));
                ctx.request_repaint();
            }
        }

        if let Some((message, expires_at)) = &self.ui_state.roi_warning {
            let now = ctx.input(|i| i.time);
            if now <= *expires_at {
                Self::status_separator(ui, colors);
                ui.label(egui::RichText::new(message).size(11.0).color(accent::RED));
            }
        }
    }

    fn render_export_status(&self, ui: &mut egui::Ui, colors: ThemeColors) {
        if self.ui_state.export.in_progress {
            Self::status_separator(ui, colors);
            ui.label(
                egui::RichText::new(&self.ui_state.export.status)
                    .size(11.0)
                    .color(colors.text_muted),
            );
            ui.add(
                egui::ProgressBar::new(self.ui_state.export.progress)
                    .desired_width(120.0)
                    .show_percentage(),
            );
        }
    }

    fn render_bottom_right(&self, ui: &mut egui::Ui) {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let colors = ThemeColors::from_ui(ui);
            let memory_bytes = self.memory_rss_bytes();
            let memory_text = if memory_bytes > 0 {
                format!("RAM: {}", format_bytes(memory_bytes))
            } else {
                "RAM: --".to_string()
            };

            let memory_response = ui.label(
                egui::RichText::new(memory_text)
                    .size(11.0)
                    .color(colors.text_primary),
            );
            memory_response.on_hover_ui(|ui| {
                let colors = ThemeColors::from_ui(ui);
                ui.label(egui::RichText::new("Memory (process)").size(12.0).strong());
                if memory_bytes > 0 {
                    ui.label(format!("Process RSS: {}", format_bytes(memory_bytes)));
                } else {
                    ui.label(
                        egui::RichText::new("Process memory unavailable").color(colors.text_muted),
                    );
                }

                let breakdown = self.memory_breakdown();
                let estimated: u64 = breakdown.iter().map(|(_, bytes)| *bytes).sum();
                if estimated > 0 {
                    ui.label(format!("Estimated buffers: {}", format_bytes(estimated)));
                }

                if !breakdown.is_empty() {
                    ui.add_space(4.0);
                    egui::Grid::new("memory_breakdown")
                        .num_columns(2)
                        .spacing(egui::vec2(8.0, 2.0))
                        .show(ui, |ui| {
                            for (label, bytes) in breakdown {
                                ui.label(egui::RichText::new(label).color(colors.text_muted));
                                ui.label(format_bytes(bytes));
                                ui.end_row();
                            }
                        });
                }

                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new(
                        "Estimates exclude allocator overhead, textures, and OS caches.",
                    )
                    .size(10.0)
                    .color(colors.text_muted),
                );
            });

            ui.add_space(8.0);
            ui.label(egui::RichText::new("│").size(11.0).color(colors.text_dim));
            ui.add_space(8.0);
            let (dead_count, hot_count) = self
                .pixel_masks
                .as_ref()
                .map_or((0, 0), |mask| (mask.dead_count, mask.hot_count));

            ui.label(
                egui::RichText::new(format_number(hot_count))
                    .size(11.0)
                    .color(accent::RED),
            );
            ui.label(
                egui::RichText::new("Hot: ")
                    .size(11.0)
                    .color(colors.text_muted),
            );

            ui.add_space(8.0);

            ui.label(
                egui::RichText::new(format_number(dead_count))
                    .size(11.0)
                    .color(colors.text_dim),
            );
            ui.label(
                egui::RichText::new("Dead: ")
                    .size(11.0)
                    .color(colors.text_muted),
            );
        });
    }

    fn status_separator(ui: &mut egui::Ui, colors: ThemeColors) {
        ui.label(egui::RichText::new("│").size(11.0).color(colors.text_dim));
    }

    /// Render cache vs streaming toggle in the top bar.
    fn render_cache_toggle(&mut self, ui: &mut egui::Ui) {
        let colors = ThemeColors::from_ui(ui);
        let cache_on = self.ui_state.cache.cache_hits_in_memory;
        let on_tint = if cache_on {
            Color32::WHITE
        } else {
            colors.text_muted
        };
        let off_tint = if cache_on {
            colors.text_muted
        } else {
            Color32::WHITE
        };

        let on_icon = Self::cache_icon_image(CacheModeIcon::Memory, on_tint)
            .fit_to_exact_size(egui::vec2(14.0, 14.0));
        let off_icon = Self::cache_icon_image(CacheModeIcon::Stream, off_tint)
            .fit_to_exact_size(egui::vec2(14.0, 14.0));

        let cache_tooltip =
            "Cache hits in RAM (enables rebuild + HDF5 export). Higher memory, slower load.";
        let stream_tooltip = "Stream only (faster load, lower memory). Rebuild/export disabled.";

        egui::Frame::none()
            .fill(colors.bg_dark)
            .stroke(Stroke::new(1.0, colors.border))
            .rounding(Rounding::same(4.0))
            .inner_margin(egui::Margin::same(2.0))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(2.0, 0.0);

                    let cache_btn = egui::Button::image(on_icon)
                        .fill(if cache_on {
                            accent::BLUE
                        } else {
                            Color32::TRANSPARENT
                        })
                        .stroke(Stroke::NONE)
                        .rounding(Rounding::same(3.0))
                        .min_size(egui::vec2(28.0, 0.0));

                    if ui.add(cache_btn).on_hover_text(cache_tooltip).clicked() {
                        self.ui_state.cache.cache_hits_in_memory = true;
                    }

                    let stream_btn = egui::Button::image(off_icon)
                        .fill(if cache_on {
                            Color32::TRANSPARENT
                        } else {
                            accent::BLUE
                        })
                        .stroke(Stroke::NONE)
                        .rounding(Rounding::same(3.0))
                        .min_size(egui::vec2(28.0, 0.0));

                    if ui.add(stream_btn).on_hover_text(stream_tooltip).clicked() {
                        self.ui_state.cache.cache_hits_in_memory = false;
                    }
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
                        self.render_section(ui, "Statistics", true, None, |app, ui| {
                            app.render_statistics(ui);
                        });

                        // Clustering section
                        self.render_section(
                            ui,
                            "Clustering",
                            true,
                            Some(SectionHelp::Clustering),
                            |app, ui| {
                                app.render_clustering_controls(ui);
                            },
                        );

                        // View section
                        self.render_section(
                            ui,
                            "View",
                            true,
                            Some(SectionHelp::View),
                            |app, ui| {
                                app.render_view_options(ui);
                            },
                        );

                        // Pixel Health section
                        self.render_section(
                            ui,
                            "Pixel Health",
                            false,
                            Some(SectionHelp::PixelHealth),
                            |app, ui| {
                                app.render_pixel_health(ui);
                            },
                        );

                        // Progress indicator (when active)
                        self.render_progress_status(ui);

                        ui.add_space(12.0);
                    });
            });
    }

    /// Render a collapsible section with header.
    #[allow(clippy::too_many_lines)]
    fn render_section<F>(
        &mut self,
        ui: &mut egui::Ui,
        title: &str,
        default_open: bool,
        help: Option<SectionHelp>,
        content: F,
    ) where
        F: FnOnce(&mut Self, &mut egui::Ui),
    {
        // Section container
        ui.push_id(title, |ui| {
            let colors = ThemeColors::from_ui(ui);
            let header_height = ui.spacing().interact_size.y.max(28.0);
            let (header_rect, header_response) = ui.allocate_exact_size(
                egui::vec2(ui.available_width(), header_height),
                egui::Sense::click(),
            );

            let id = ui.make_persistent_id(format!("{title}_open"));
            let mut is_open = ui.data_mut(|d| *d.get_temp_mut_or_insert_with(id, || default_open));

            let help_state = match help {
                Some(SectionHelp::Clustering) => self.ui_state.panel_popups.show_clustering_help,
                Some(SectionHelp::View) => self.ui_state.panel_popups.show_view_help,
                Some(SectionHelp::PixelHealth) => self.ui_state.panel_popups.show_pixel_health_help,
                None => false,
            };

            let help_rect = help.map(|_| {
                let size = egui::vec2(18.0, 18.0);
                Rect::from_center_size(header_rect.right_center() - egui::vec2(36.0, 0.0), size)
            });
            let help_clicked = help_rect.is_some_and(|rect| {
                let help_id = ui.make_persistent_id(format!("{title}_help"));
                let response = ui.interact(rect, help_id, egui::Sense::click());
                let response = response.on_hover_text("Help");
                if response.clicked() {
                    match help {
                        Some(SectionHelp::Clustering) => {
                            self.ui_state.panel_popups.show_clustering_help = !help_state;
                        }
                        Some(SectionHelp::View) => {
                            self.ui_state.panel_popups.show_view_help = !help_state;
                        }
                        Some(SectionHelp::PixelHealth) => {
                            self.ui_state.panel_popups.show_pixel_health_help = !help_state;
                        }
                        None => {}
                    }
                }
                response.clicked()
            });

            if header_response.clicked() && !help_clicked {
                is_open = !is_open;
                ui.data_mut(|d| d.insert_temp(id, is_open));
            }

            let header_fill = if header_response.hovered() {
                colors.bg_header
            } else {
                Color32::TRANSPARENT
            };
            ui.painter().rect_filled(header_rect, 0.0, header_fill);

            let text_pos = header_rect.left_center() + egui::vec2(16.0, 0.0);
            ui.painter().text(
                text_pos,
                egui::Align2::LEFT_CENTER,
                title.to_uppercase(),
                FontId::new(11.0, FontFamily::Proportional),
                colors.text_primary,
            );

            if let Some(rect) = help_rect {
                let fill = if help_state {
                    colors.bg_header
                } else {
                    Color32::TRANSPARENT
                };
                ui.painter().rect_stroke(
                    rect,
                    Rounding::same(3.0),
                    Stroke::new(1.0, colors.border_light),
                );
                ui.painter().rect_filled(rect, Rounding::same(3.0), fill);
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "?",
                    FontId::new(11.0, FontFamily::Proportional),
                    colors.text_dim,
                );
            }

            let arrow = if is_open { "▼" } else { "▶" };
            let arrow_pos = header_rect.right_center() - egui::vec2(16.0, 0.0);
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
                .checkbox(&mut self.ui_state.histogram.slicer_enabled, "TOF Slicer")
                .changed()
            {
                self.texture = None;
            }
        });

        ui.checkbox(&mut self.ui_state.histogram.show, "Spectrum");

        if ui
            .checkbox(&mut self.ui_state.histogram.log_scale, "Log scale")
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

    fn render_clustering_controls(&mut self, ui: &mut egui::Ui) {
        let colors = ThemeColors::from_ui(ui);

        self.render_clustering_algorithm(ui);

        if self.ui_state.panels.show_clustering_params {
            self.render_clustering_params(ui, &colors);
        }

        self.render_run_clustering_button(ui);
    }

    fn render_clustering_algorithm(&mut self, ui: &mut egui::Ui) {
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
                self.ui_state.panels.show_clustering_params =
                    !self.ui_state.panels.show_clustering_params;
            }
        });
    }

    fn render_clustering_params(&mut self, ui: &mut egui::Ui, colors: &ThemeColors) {
        ui.add_space(8.0);
        egui::Frame::none()
            .fill(colors.bg_header)
            .stroke(Stroke::new(1.0, colors.border))
            .rounding(Rounding::same(4.0))
            .inner_margin(egui::Margin::same(12.0))
            .show(ui, |ui| {
                self.render_detector_profile_controls(ui);
                ui.add_space(6.0);
                self.render_tdc_frequency_control(ui);
                ui.add_space(4.0);
                self.render_radius_control(ui);
                ui.add_space(4.0);
                self.render_time_window_control(ui);
                ui.add_space(4.0);
                self.render_min_cluster_control(ui);
                ui.add_space(4.0);
                self.render_max_cluster_control(ui);

                if self.algo_type == AlgorithmType::Dbscan {
                    ui.add_space(4.0);
                    self.render_dbscan_control(ui);
                }

                if self.algo_type == AlgorithmType::Grid {
                    ui.add_space(4.0);
                    self.render_grid_control(ui);
                }

                ui.add_space(4.0);
                self.render_super_resolution_control(ui);
                ui.add_space(6.0);
                self.render_extraction_controls(ui);
                ui.add_space(8.0);
                self.render_clustering_reset(ui);
            });
    }

    #[allow(clippy::too_many_lines)]
    fn render_detector_profile_controls(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let colors = ThemeColors::from_ui(ui);
            ui.label(
                egui::RichText::new("Detector")
                    .size(10.0)
                    .color(colors.text_muted),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let label = self.detector_profile.label();
                egui::ComboBox::from_id_salt("detector_profile")
                    .selected_text(label)
                    .width(160.0)
                    .show_ui(ui, |ui| {
                        let mut kind = self.detector_profile.kind;
                        ui.selectable_value(&mut kind, DetectorProfileKind::Venus, "VENUS (SNS)");
                        if self.detector_profile.has_custom() {
                            let custom_label = self
                                .detector_profile
                                .custom_name
                                .clone()
                                .unwrap_or_else(|| "Custom".to_string());
                            ui.selectable_value(
                                &mut kind,
                                DetectorProfileKind::Custom,
                                custom_label,
                            );
                        } else {
                            ui.add_enabled(
                                false,
                                egui::SelectableLabel::new(false, "Custom (load...)"),
                            );
                        }
                        if kind != self.detector_profile.kind {
                            self.detector_profile.kind = kind;
                        }
                    });
            });
        });

        ui.horizontal(|ui| {
            if ui.button("Load detector config…").clicked() {
                if let Some(path) = FileDialog::new()
                    .add_filter("Detector config", &["json"])
                    .pick_file()
                {
                    match DetectorConfig::from_file(&path) {
                        Ok(config) => {
                            let name = path.file_name().map(|n| n.to_string_lossy().to_string());
                            if config.tdc_frequency_hz.is_finite() && config.tdc_frequency_hz > 0.0
                            {
                                self.tdc_frequency = config.tdc_frequency_hz;
                            }
                            self.detector_profile.custom_config = Some(config);
                            self.detector_profile.custom_path = Some(path.clone());
                            self.detector_profile.custom_name = name;
                            self.detector_profile.kind = DetectorProfileKind::Custom;
                        }
                        Err(err) => {
                            self.ui_state.roi_warning = Some((
                                format!("Detector config load failed: {err}"),
                                ui.ctx().input(|i| i.time + 6.0),
                            ));
                        }
                    }
                }
            }
            if ui.button("Save detector config…").clicked() {
                if let Some(path) = FileDialog::new()
                    .add_filter("Detector config", &["json"])
                    .save_file()
                {
                    let config = self.current_detector_config();
                    if let Err(err) = config.to_file(&path) {
                        self.ui_state.roi_warning = Some((
                            format!("Detector config save failed: {err}"),
                            ui.ctx().input(|i| i.time + 6.0),
                        ));
                    } else if self.detector_profile.kind == DetectorProfileKind::Custom {
                        let name = path.file_name().map(|n| n.to_string_lossy().to_string());
                        self.detector_profile.custom_path = Some(path.clone());
                        if let Some(name) = name {
                            self.detector_profile.custom_name = Some(name);
                        }
                    }
                }
            }
            if ui.button("Reset to VENUS").clicked() {
                self.detector_profile = DetectorProfile {
                    kind: DetectorProfileKind::Venus,
                    ..Default::default()
                };
            }
        });

        if let Some(path) = self.detector_profile.custom_path.as_ref() {
            let colors = ThemeColors::from_ui(ui);
            ui.label(
                egui::RichText::new(format!(
                    "Custom: {}",
                    path.file_name().unwrap_or_default().to_string_lossy()
                ))
                .size(10.0)
                .color(colors.text_dim),
            );
        }

        egui::CollapsingHeader::new("Edit detector config")
            .default_open(false)
            .show(ui, |ui| {
                if self.detector_profile.custom_config.is_none() {
                    ui.label("No custom config loaded.");
                    if ui.button("Create custom from VENUS").clicked() {
                        self.detector_profile.custom_config =
                            Some(DetectorConfig::venus_defaults());
                        self.tdc_frequency = 60.0;
                        if self.detector_profile.custom_name.is_none() {
                            self.detector_profile.custom_name = Some("Custom".to_string());
                        }
                        self.detector_profile.custom_path = None;
                        self.detector_profile.kind = DetectorProfileKind::Custom;
                    }
                    return;
                }

                let mut changed = false;
                let mut validation_error = None;
                let mut reverted_invalid = false;

                {
                    let Some(config) = self.detector_profile.custom_config.as_mut() else {
                        return;
                    };
                    let before = config.clone();

                    ui.horizontal(|ui| {
                        ui.label("Chip size X");
                        let mut value = config.chip_size_x;
                        if ui
                            .add(egui::DragValue::new(&mut value).range(1..=u16::MAX))
                            .changed()
                        {
                            config.chip_size_x = value;
                            changed = true;
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("Chip size Y");
                        let mut value = config.chip_size_y;
                        if ui
                            .add(egui::DragValue::new(&mut value).range(1..=u16::MAX))
                            .changed()
                        {
                            config.chip_size_y = value;
                            changed = true;
                        }
                    });

                    changed |= ui
                        .checkbox(
                            &mut config.enable_missing_tdc_correction,
                            "Enable missing TDC correction",
                        )
                        .changed();

                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        ui.label("Chip transforms");
                        let colors = ThemeColors::from_ui(ui);
                        ui.add(
                            egui::Button::new("?")
                                .fill(Color32::TRANSPARENT)
                                .stroke(Stroke::new(1.0, colors.border_light))
                                .rounding(Rounding::same(4.0)),
                        )
                        .on_hover_text(
                            "Affine transform per chip:\n\
global_x = a*x + b*y + tx\n\
global_y = c*x + d*y + ty\n\
x,y are local chip coordinates (pixels).",
                        );
                    });
                    egui::Grid::new("chip_transform_grid")
                        .spacing(egui::vec2(6.0, 4.0))
                        .show(ui, |ui| {
                            ui.label("Chip");
                            ui.label("a");
                            ui.label("b");
                            ui.label("c");
                            ui.label("d");
                            ui.label("tx");
                            ui.label("ty");
                            ui.end_row();

                            let range = -65_535..=65_535;
                            for (chip_id, transform) in
                                config.chip_transforms.iter_mut().enumerate()
                            {
                                ui.label(chip_id.to_string());
                                changed |= ui
                                    .add(
                                        egui::DragValue::new(&mut transform.a).range(range.clone()),
                                    )
                                    .changed();
                                changed |= ui
                                    .add(
                                        egui::DragValue::new(&mut transform.b).range(range.clone()),
                                    )
                                    .changed();
                                changed |= ui
                                    .add(
                                        egui::DragValue::new(&mut transform.c).range(range.clone()),
                                    )
                                    .changed();
                                changed |= ui
                                    .add(
                                        egui::DragValue::new(&mut transform.d).range(range.clone()),
                                    )
                                    .changed();
                                changed |= ui
                                    .add(
                                        egui::DragValue::new(&mut transform.tx)
                                            .range(range.clone()),
                                    )
                                    .changed();
                                changed |= ui
                                    .add(
                                        egui::DragValue::new(&mut transform.ty)
                                            .range(range.clone()),
                                    )
                                    .changed();
                                ui.end_row();
                            }
                        });

                    ui.horizontal(|ui| {
                        if ui.button("Add chip").clicked() {
                            config.chip_transforms.push(ChipTransform::identity());
                            changed = true;
                        }
                        if !config.chip_transforms.is_empty()
                            && ui.button("Remove last chip").clicked()
                        {
                            config.chip_transforms.pop();
                            changed = true;
                        }
                    });

                    if let Err(err) = config.validate_transforms() {
                        validation_error = Some(err.to_string());
                        if changed {
                            *config = before;
                            reverted_invalid = true;
                        }
                    }
                }

                if changed && !reverted_invalid {
                    self.detector_profile.kind = DetectorProfileKind::Custom;
                    self.detector_profile.custom_path = None;
                    if self.detector_profile.custom_name.is_none() {
                        self.detector_profile.custom_name = Some("Custom".to_string());
                    }
                }

                if let Some(err) = validation_error {
                    let message = if reverted_invalid {
                        format!("Transform warning: {err} (changes reverted)")
                    } else {
                        format!("Transform warning: {err}")
                    };
                    ui.colored_label(Color32::YELLOW, message);
                }
            });
    }

    fn render_tdc_frequency_control(&mut self, ui: &mut egui::Ui) {
        let colors = ThemeColors::from_ui(ui);
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("TDC Freq")
                    .size(10.0)
                    .color(colors.text_muted),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let step = 1.0;
                let min = 1.0;
                let max = 120.0;
                if ui
                    .add_enabled(
                        self.tdc_frequency < max,
                        egui::Button::new("+").min_size(egui::vec2(18.0, 18.0)),
                    )
                    .clicked()
                {
                    self.tdc_frequency = (self.tdc_frequency + step).min(max);
                }
                ui.add(
                    egui::DragValue::new(&mut self.tdc_frequency)
                        .range(min..=max)
                        .speed(step)
                        .suffix(" Hz"),
                );
                if ui
                    .add_enabled(
                        self.tdc_frequency > min,
                        egui::Button::new("−").min_size(egui::vec2(18.0, 18.0)),
                    )
                    .clicked()
                {
                    self.tdc_frequency = (self.tdc_frequency - step).max(min);
                }
            });
        });
    }

    fn render_radius_control(&mut self, ui: &mut egui::Ui) {
        let colors = ThemeColors::from_ui(ui);
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("Radius")
                    .size(10.0)
                    .color(colors.text_muted),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let step = 0.5;
                let min = 1.0;
                let max = 50.0;
                if ui
                    .add_enabled(
                        self.radius < max,
                        egui::Button::new("+").min_size(egui::vec2(18.0, 18.0)),
                    )
                    .clicked()
                {
                    self.radius = (self.radius + step).min(max);
                }
                ui.add(
                    egui::DragValue::new(&mut self.radius)
                        .range(min..=max)
                        .speed(step)
                        .suffix(" px"),
                );
                if ui
                    .add_enabled(
                        self.radius > min,
                        egui::Button::new("−").min_size(egui::vec2(18.0, 18.0)),
                    )
                    .clicked()
                {
                    self.radius = (self.radius - step).max(min);
                }
            });
        });
    }

    fn render_time_window_control(&mut self, ui: &mut egui::Ui) {
        let colors = ThemeColors::from_ui(ui);
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("Time window")
                    .size(10.0)
                    .color(colors.text_muted),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let step = 1.0;
                let min = 10.0;
                let max = 500.0;
                if ui
                    .add_enabled(
                        self.temporal_window_ns < max,
                        egui::Button::new("+").min_size(egui::vec2(18.0, 18.0)),
                    )
                    .clicked()
                {
                    self.temporal_window_ns = (self.temporal_window_ns + step).min(max);
                }
                ui.add(
                    egui::DragValue::new(&mut self.temporal_window_ns)
                        .range(min..=max)
                        .speed(step)
                        .suffix(" ns"),
                );
                if ui
                    .add_enabled(
                        self.temporal_window_ns > min,
                        egui::Button::new("−").min_size(egui::vec2(18.0, 18.0)),
                    )
                    .clicked()
                {
                    self.temporal_window_ns = (self.temporal_window_ns - step).max(min);
                }
            });
        });
    }

    fn render_min_cluster_control(&mut self, ui: &mut egui::Ui) {
        let colors = ThemeColors::from_ui(ui);
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("Min cluster")
                    .size(10.0)
                    .color(colors.text_muted),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let min = 1;
                let max = 10;
                if ui
                    .add_enabled(
                        self.min_cluster_size < max,
                        egui::Button::new("+").min_size(egui::vec2(18.0, 18.0)),
                    )
                    .clicked()
                {
                    self.min_cluster_size = (self.min_cluster_size + 1).min(max);
                }
                ui.add(
                    egui::DragValue::new(&mut self.min_cluster_size)
                        .range(min..=max)
                        .speed(1),
                );
                if ui
                    .add_enabled(
                        self.min_cluster_size > min,
                        egui::Button::new("−").min_size(egui::vec2(18.0, 18.0)),
                    )
                    .clicked()
                {
                    self.min_cluster_size = (self.min_cluster_size - 1).max(min);
                }
            });
        });
        if let Some(max_value) = self.max_cluster_size {
            if self.min_cluster_size > max_value {
                self.max_cluster_size = Some(self.min_cluster_size);
            }
        }
    }

    fn render_max_cluster_control(&mut self, ui: &mut egui::Ui) {
        let colors = ThemeColors::from_ui(ui);
        let mut limit_max = self.max_cluster_size.is_some();
        let mut max_value = self
            .max_cluster_size
            .unwrap_or(self.min_cluster_size.max(1));
        ui.horizontal(|ui| {
            ui.checkbox(&mut limit_max, "");
            ui.label(
                egui::RichText::new("Max cluster")
                    .size(10.0)
                    .color(colors.text_muted),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if limit_max {
                    let min = 1;
                    let max = 256;
                    if ui
                        .add_enabled(
                            max_value < max,
                            egui::Button::new("+").min_size(egui::vec2(18.0, 18.0)),
                        )
                        .clicked()
                    {
                        max_value = (max_value + 1).min(max);
                    }
                    ui.add(
                        egui::DragValue::new(&mut max_value)
                            .range(min..=max)
                            .speed(1),
                    );
                    if ui
                        .add_enabled(
                            max_value > min,
                            egui::Button::new("−").min_size(egui::vec2(18.0, 18.0)),
                        )
                        .clicked()
                    {
                        max_value = (max_value - 1).max(min);
                    }
                } else {
                    ui.label(egui::RichText::new("∞").size(10.0).color(colors.text_dim));
                }
            });
        });

        if limit_max {
            let min_value = self.min_cluster_size.max(1);
            if max_value < min_value {
                max_value = min_value;
            }
            self.max_cluster_size = Some(max_value);
        } else {
            self.max_cluster_size = None;
        }
    }

    fn render_dbscan_control(&mut self, ui: &mut egui::Ui) {
        let colors = ThemeColors::from_ui(ui);
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("Min points")
                    .size(10.0)
                    .color(colors.text_muted),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let min = 1;
                let max = 10;
                if ui
                    .add_enabled(
                        self.dbscan_min_points < max,
                        egui::Button::new("+").min_size(egui::vec2(18.0, 18.0)),
                    )
                    .clicked()
                {
                    self.dbscan_min_points = (self.dbscan_min_points + 1).min(max);
                }
                ui.add(
                    egui::DragValue::new(&mut self.dbscan_min_points)
                        .range(min..=max)
                        .speed(1),
                );
                if ui
                    .add_enabled(
                        self.dbscan_min_points > min,
                        egui::Button::new("−").min_size(egui::vec2(18.0, 18.0)),
                    )
                    .clicked()
                {
                    self.dbscan_min_points = (self.dbscan_min_points - 1).max(min);
                }
            });
        });
    }

    fn render_grid_control(&mut self, ui: &mut egui::Ui) {
        let colors = ThemeColors::from_ui(ui);
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("Grid cell")
                    .size(10.0)
                    .color(colors.text_muted),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let min = 4;
                let max = 128;
                if ui
                    .add_enabled(
                        self.grid_cell_size < max,
                        egui::Button::new("+").min_size(egui::vec2(18.0, 18.0)),
                    )
                    .clicked()
                {
                    self.grid_cell_size = (self.grid_cell_size + 1).min(max);
                }
                ui.add(
                    egui::DragValue::new(&mut self.grid_cell_size)
                        .range(min..=max)
                        .speed(1)
                        .suffix(" px"),
                );
                if ui
                    .add_enabled(
                        self.grid_cell_size > min,
                        egui::Button::new("−").min_size(egui::vec2(18.0, 18.0)),
                    )
                    .clicked()
                {
                    self.grid_cell_size = (self.grid_cell_size - 1).max(min);
                }
            });
        });
    }

    fn render_super_resolution_control(&mut self, ui: &mut egui::Ui) {
        let colors = ThemeColors::from_ui(ui);
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("Super-res")
                    .size(10.0)
                    .color(colors.text_muted),
            )
            .on_hover_text("Sub-pixel centroid scale (affects neutron coordinates + export)");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let step = 0.1;
                let min = 1.0;
                let max = 16.0;
                if ui
                    .add_enabled(
                        self.super_resolution_factor < max,
                        egui::Button::new("+").min_size(egui::vec2(18.0, 18.0)),
                    )
                    .clicked()
                {
                    self.super_resolution_factor = (self.super_resolution_factor + step).min(max);
                }
                ui.add(
                    egui::DragValue::new(&mut self.super_resolution_factor)
                        .range(min..=max)
                        .speed(step)
                        .suffix("×"),
                )
                .on_hover_text("Higher values increase spatial precision (more sub-pixels)");
                if ui
                    .add_enabled(
                        self.super_resolution_factor > min,
                        egui::Button::new("−").min_size(egui::vec2(18.0, 18.0)),
                    )
                    .clicked()
                {
                    self.super_resolution_factor = (self.super_resolution_factor - step).max(min);
                }
            });
        });
    }

    fn render_extraction_controls(&mut self, ui: &mut egui::Ui) {
        let colors = ThemeColors::from_ui(ui);
        ui.label(
            egui::RichText::new("Extraction")
                .size(10.0)
                .color(colors.text_muted),
        );
        ui.add_space(2.0);

        ui.checkbox(&mut self.weighted_by_tot, "Weighted by TOT")
            .on_hover_text("Use TOT as weights for centroiding (more stable positions)");
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("Min TOT")
                    .size(10.0)
                    .color(colors.text_muted),
            )
            .on_hover_text("Ignore hits with very low TOT (reduces noise)");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let min = 0;
                let max = 200;
                if ui
                    .add_enabled(
                        self.min_tot_threshold < max,
                        egui::Button::new("+").min_size(egui::vec2(18.0, 18.0)),
                    )
                    .clicked()
                {
                    self.min_tot_threshold = (self.min_tot_threshold + 1).min(max);
                }
                ui.add(
                    egui::DragValue::new(&mut self.min_tot_threshold)
                        .range(min..=max)
                        .speed(1),
                );
                if ui
                    .add_enabled(
                        self.min_tot_threshold > min,
                        egui::Button::new("−").min_size(egui::vec2(18.0, 18.0)),
                    )
                    .clicked()
                {
                    self.min_tot_threshold = (self.min_tot_threshold - 1).max(min);
                }
            });
        });
    }

    fn render_clustering_reset(&mut self, ui: &mut egui::Ui) {
        if ui.button("Reset to defaults").clicked() {
            self.radius = 5.0;
            self.temporal_window_ns = 75.0;
            self.min_cluster_size = 1;
            self.max_cluster_size = None;
            self.dbscan_min_points = 2;
            self.grid_cell_size = 32;
            self.super_resolution_factor = 1.0;
            self.weighted_by_tot = false;
            self.min_tot_threshold = 0;
        }
    }

    fn render_run_clustering_button(&mut self, ui: &mut egui::Ui) {
        ui.add_space(12.0);

        let can_cluster = !self.processing.is_loading
            && !self.processing.is_processing
            && self.selected_file.is_some()
            && self.statistics.hit_count > 0;

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

    /// Render pixel health (dead/hot masks) summary and controls.
    fn render_pixel_health(&mut self, ui: &mut egui::Ui) {
        let colors = ThemeColors::from_ui(ui);
        let Some(mask) = self.pixel_masks.as_ref() else {
            ui.label(
                egui::RichText::new("Load data to analyze pixel health")
                    .size(11.0)
                    .color(colors.text_muted),
            );
            return;
        };
        let (dead_count, hot_count, mean, std_dev, hot_threshold) = (
            mask.dead_count,
            mask.hot_count,
            mask.mean,
            mask.std_dev,
            mask.hot_threshold,
        );

        self.render_pixel_health_header(ui, &colors);
        Self::render_pixel_health_counts(ui, &colors, dead_count, hot_count);
        self.render_pixel_health_overlays(ui);

        if self.ui_state.pixel_health.show_pixel_health_settings {
            self.render_pixel_health_settings(ui, &colors, mean, std_dev, hot_threshold);
        }
    }

    fn render_pixel_health_header(&mut self, ui: &mut egui::Ui, colors: &ThemeColors) {
        ui.horizontal(|ui| {
            ui.label(form_label("Hot pixels"));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let gear = Self::file_icon_image(FileToolbarIcon::Gear, colors.text_muted)
                    .fit_to_exact_size(egui::vec2(14.0, 14.0));
                if ui
                    .add(egui::ImageButton::new(gear).frame(true))
                    .on_hover_text("Pixel mask settings")
                    .clicked()
                {
                    self.ui_state.pixel_health.show_pixel_health_settings =
                        !self.ui_state.pixel_health.show_pixel_health_settings;
                }
            });
        });
    }

    fn render_pixel_health_counts(
        ui: &mut egui::Ui,
        colors: &ThemeColors,
        dead_count: usize,
        hot_count: usize,
    ) {
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("Dead")
                    .size(11.0)
                    .color(colors.text_muted),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    egui::RichText::new(format_number(dead_count))
                        .size(11.0)
                        .color(colors.text_primary),
                );
            });
        });
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("Hot")
                    .size(11.0)
                    .color(colors.text_muted),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    egui::RichText::new(format_number(hot_count))
                        .size(11.0)
                        .color(accent::RED),
                );
            });
        });
    }

    fn render_pixel_health_overlays(&mut self, ui: &mut egui::Ui) {
        ui.add_space(8.0);
        ui.checkbox(
            &mut self.ui_state.pixel_health.show_hot_pixels,
            "Show hot pixels overlay",
        );
        let mut exclude_masked = self.ui_state.pixel_health.exclude_masked_pixels;
        let exclude_response = ui.checkbox(
            &mut exclude_masked,
            "Exclude masked pixels from spectra/stats",
        );
        if exclude_response.changed() {
            self.ui_state.pixel_health.exclude_masked_pixels = exclude_masked;
            self.update_masked_spectrum();
            self.hit_data_revision = self.hit_data_revision.wrapping_add(1);
        }
    }

    fn render_pixel_health_settings(
        &mut self,
        ui: &mut egui::Ui,
        colors: &ThemeColors,
        mean: f64,
        std_dev: f64,
        threshold: f64,
    ) {
        ui.add_space(8.0);
        ui.separator();
        ui.add_space(6.0);

        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("Sigma threshold")
                    .size(11.0)
                    .color(colors.text_muted),
            );
            let mut sigma = self.hot_pixel_sigma;
            let response = ui.add(egui::DragValue::new(&mut sigma).range(1.0..=10.0));
            if response.changed() {
                self.hot_pixel_sigma = sigma;
                self.update_pixel_masks();
            }
        });
        ui.add_space(6.0);
        if ui.button("Recompute masks").clicked() {
            self.update_pixel_masks();
        }
        ui.add_space(4.0);
        ui.label(
            egui::RichText::new(format!(
                "mean {mean:.2} • σ {std_dev:.2} • threshold {threshold:.2}"
            ))
            .size(10.0)
            .color(colors.text_dim),
        );
    }

    /// Render floating settings windows (app + spectrum).
    pub(crate) fn render_settings_windows(&mut self, ctx: &egui::Context) {
        if self.ui_state.panels.show_app_settings {
            let mut show_app_settings = self.ui_state.panels.show_app_settings;
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
            self.ui_state.panels.show_app_settings = show_app_settings;
        }

        if self.ui_state.panels.show_spectrum_settings {
            let mut show_spectrum_settings = self.ui_state.panels.show_spectrum_settings;
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
            self.ui_state.panels.show_spectrum_settings = show_spectrum_settings;
        }

        if self.ui_state.export.show_dialog {
            self.render_export_dialog(ctx);
        }

        self.render_help_windows(ctx);
    }

    fn render_help_windows(&mut self, ctx: &egui::Context) {
        self.render_clustering_help_panel(ctx);
        self.render_view_help_panel(ctx);
        self.render_pixel_health_help_panel(ctx);
    }

    fn render_clustering_help_panel(&mut self, ctx: &egui::Context) {
        if !self.ui_state.panel_popups.show_clustering_help {
            return;
        }
        let mut open = self.ui_state.panel_popups.show_clustering_help;
        egui::Window::new("Clustering Help")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(300.0)
            .show(ctx, |ui| {
                ui.label(egui::RichText::new("Algorithm").strong());
                ui.label("• ABS / DBSCAN / Grid: choose clustering method.");
                ui.label("• Parameters control spatial radius + time window.");
                ui.add_space(6.0);
                ui.label(egui::RichText::new("Extraction").strong());
                ui.label("• Super-res scales sub-pixel neutron positions.");
                ui.label("• Weighted by TOT improves centroid stability.");
                ui.label("• Min TOT filters low signal hits.");
                ui.add_space(6.0);
                ui.label(egui::RichText::new("Run").strong());
                ui.label("• Click Run Clustering to generate neutrons.");
            });
        self.ui_state.panel_popups.show_clustering_help = open;
    }

    fn render_view_help_panel(&mut self, ctx: &egui::Context) {
        if !self.ui_state.panel_popups.show_view_help {
            return;
        }
        let mut open = self.ui_state.panel_popups.show_view_help;
        egui::Window::new("View Help")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(280.0)
            .show(ctx, |ui| {
                ui.label(egui::RichText::new("Colormap").strong());
                ui.label("• Change display palette only (data unchanged).");
                ui.add_space(6.0);
                ui.label(egui::RichText::new("TOF Slicer").strong());
                ui.label("• Show a single TOF bin instead of full projection.");
                ui.add_space(6.0);
                ui.label(egui::RichText::new("Spectrum").strong());
                ui.label("• Toggle spectrum panel visibility.");
                ui.add_space(6.0);
                ui.label(egui::RichText::new("Log scale").strong());
                ui.label("• Use log intensity for histogram display.");
            });
        self.ui_state.panel_popups.show_view_help = open;
    }

    fn render_pixel_health_help_panel(&mut self, ctx: &egui::Context) {
        if !self.ui_state.panel_popups.show_pixel_health_help {
            return;
        }
        let mut open = self.ui_state.panel_popups.show_pixel_health_help;
        egui::Window::new("Pixel Health Help")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(300.0)
            .show(ctx, |ui| {
                ui.label(egui::RichText::new("Dead/Hot pixels").strong());
                ui.label("• Computed from hit statistics in the current dataset.");
                ui.label("• Hot pixel overlay marks outliers.");
                ui.add_space(6.0);
                ui.label(egui::RichText::new("Masks").strong());
                ui.label("• Exclude masked pixels from spectra/stats.");
                ui.label("• Recompute masks after changing thresholds.");
            });
        self.ui_state.panel_popups.show_pixel_health_help = open;
    }

    fn render_export_dialog(&mut self, ctx: &egui::Context) {
        let mut open = self.ui_state.export.show_dialog;
        let mut should_close = false;
        let availability = self.export_availability();
        egui::Window::new("Export")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                let colors = ThemeColors::from_ui(ui);
                let hit_count = self.statistics.hit_count;
                let neutron_count = self.statistics.neutron_count;
                let view_mode = self.ui_state.view_mode;
                let export_in_progress = self.ui_state.export.in_progress;

                Self::render_export_format_selector(ui, &colors, &mut self.ui_state.export.format);
                ui.add_space(10.0);

                let save_clicked = match self.ui_state.export.format {
                    ExportFormat::Hdf5 => {
                        let options = &mut self.ui_state.export.options;
                        Self::apply_export_availability(options, availability);
                        Self::render_export_header(ui, &colors, options);
                        ui.add_space(8.0);
                        Self::render_export_dataset_options(
                            ui,
                            options,
                            availability,
                            hit_count,
                            neutron_count,
                            view_mode,
                        );
                        if !availability.deflate.is_available() {
                            Self::render_export_deflate_warning(ui);
                        }
                        if options.advanced.enabled {
                            Self::render_export_advanced(ui, &colors, options);
                        }
                        ui.add_space(10.0);
                        Self::render_export_save_button(
                            ui,
                            self.ui_state.export.format,
                            options,
                            availability,
                            export_in_progress,
                        )
                    }
                    ExportFormat::TiffFolder | ExportFormat::TiffStack => {
                        self.populate_default_tiff_base_name();
                        let options = &mut self.ui_state.export.tiff;
                        let base_name_ok =
                            !Self::sanitize_export_base_name(&options.base_name).is_empty();
                        Self::render_tiff_export_options(
                            ui,
                            &colors,
                            options,
                            self.ui_state.export.format,
                            availability,
                        );
                        ui.add_space(10.0);
                        Self::render_tiff_export_button(
                            ui,
                            self.ui_state.export.format,
                            availability,
                            export_in_progress,
                            base_name_ok,
                        )
                    }
                };

                if save_clicked {
                    match self.ui_state.export.format {
                        ExportFormat::Hdf5 => {
                            if let Some(path) =
                                FileDialog::new().set_file_name("rustpix.h5").save_file()
                            {
                                self.start_export_hdf5(path);
                                should_close = true;
                            }
                        }
                        ExportFormat::TiffFolder | ExportFormat::TiffStack => {
                            if let Some(parent) = FileDialog::new().pick_folder() {
                                let base_name = Self::sanitize_export_base_name(
                                    &self.ui_state.export.tiff.base_name,
                                );
                                if !base_name.is_empty() {
                                    let folder = parent.join(&base_name);
                                    self.start_export_tiff(folder, self.ui_state.export.format);
                                    should_close = true;
                                }
                            }
                        }
                    }
                }
            });
        if should_close {
            open = false;
        }
        self.ui_state.export.show_dialog = open;
    }

    fn render_export_format_selector(
        ui: &mut egui::Ui,
        colors: &ThemeColors,
        format: &mut ExportFormat,
    ) {
        ui.label(
            egui::RichText::new("Export format")
                .size(11.0)
                .color(colors.text_primary),
        );
        ui.add_space(4.0);
        egui::ComboBox::from_id_salt("export_format")
            .selected_text(format.to_string())
            .width(ui.available_width() - 8.0)
            .show_ui(ui, |ui| {
                for option in [
                    ExportFormat::Hdf5,
                    ExportFormat::TiffFolder,
                    ExportFormat::TiffStack,
                ] {
                    ui.selectable_value(format, option, option.to_string());
                }
            });
    }

    fn render_tiff_export_options(
        ui: &mut egui::Ui,
        colors: &ThemeColors,
        options: &mut TiffExportOptions,
        format: ExportFormat,
        availability: ExportAvailability,
    ) {
        ui.label(
            egui::RichText::new("TIFF export options")
                .size(11.0)
                .color(colors.text_primary),
        );
        ui.add_space(6.0);

        Self::render_tiff_bit_depth(ui, options);
        ui.add_space(4.0);
        Self::render_tiff_spectra_options(ui, options);
        ui.add_space(8.0);
        Self::render_tiff_base_name(ui, colors, options);
        if format == ExportFormat::TiffStack {
            Self::render_tiff_stack_behavior(ui, colors, options);
        }

        if !availability.histogram.is_available() {
            Self::render_tiff_availability_warning(ui);
        }
    }

    fn render_tiff_bit_depth(ui: &mut egui::Ui, options: &mut TiffExportOptions) {
        ui.horizontal(|ui| {
            ui.label("Bit depth");
            egui::ComboBox::from_id_salt("tiff_bit_depth")
                .selected_text(options.bit_depth.to_string())
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut options.bit_depth,
                        TiffBitDepth::Bit16,
                        TiffBitDepth::Bit16.to_string(),
                    );
                    ui.selectable_value(
                        &mut options.bit_depth,
                        TiffBitDepth::Bit32,
                        TiffBitDepth::Bit32.to_string(),
                    );
                });
        });
    }

    fn render_tiff_spectra_options(ui: &mut egui::Ui, options: &mut TiffExportOptions) {
        ui.checkbox(&mut options.include_spectra, "Include spectra file");
        ui.checkbox(&mut options.include_summed_image, "Include summed image");
        ui.checkbox(
            &mut options.exclude_masked_pixels,
            "Exclude masked pixels (spectra)",
        );
        ui.horizontal(|ui| {
            ui.checkbox(&mut options.include_tof_offset, "Include TOF offset");
            ui.add_space(10.0);
            ui.label("Timing");
            egui::ComboBox::from_id_salt("tiff_spectra_timing")
                .selected_text(options.spectra_timing.to_string())
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut options.spectra_timing,
                        TiffSpectraTiming::BinCenter,
                        TiffSpectraTiming::BinCenter.to_string(),
                    );
                    ui.selectable_value(
                        &mut options.spectra_timing,
                        TiffSpectraTiming::BinStart,
                        TiffSpectraTiming::BinStart.to_string(),
                    );
                });
        });
    }

    fn render_tiff_base_name(
        ui: &mut egui::Ui,
        colors: &ThemeColors,
        options: &mut TiffExportOptions,
    ) {
        ui.horizontal(|ui| {
            ui.label("Base name");
            ui.add(
                egui::TextEdit::singleline(&mut options.base_name)
                    .desired_width(ui.available_width() - 70.0),
            );
        });
        ui.add_space(2.0);
        ui.label(
            egui::RichText::new("A folder named after the base name will be created.")
                .size(10.0)
                .color(colors.text_muted),
        );
        if Self::sanitize_export_base_name(&options.base_name).is_empty() {
            ui.add_space(2.0);
            ui.label(
                egui::RichText::new("Base name is required.")
                    .size(10.0)
                    .color(accent::RED),
            );
        }
    }

    fn render_tiff_stack_behavior(
        ui: &mut egui::Ui,
        colors: &ThemeColors,
        options: &mut TiffExportOptions,
    ) {
        ui.add_space(8.0);
        ui.separator();
        ui.add_space(6.0);
        ui.label(
            egui::RichText::new("Large stack handling")
                .size(11.0)
                .color(colors.text_primary),
        );
        egui::ComboBox::from_id_salt("tiff_stack_behavior")
            .selected_text(options.stack_behavior.to_string())
            .width(ui.available_width() - 8.0)
            .show_ui(ui, |ui| {
                ui.selectable_value(
                    &mut options.stack_behavior,
                    TiffStackBehavior::StandardOnly,
                    TiffStackBehavior::StandardOnly.to_string(),
                );
                ui.selectable_value(
                    &mut options.stack_behavior,
                    TiffStackBehavior::AutoBigTiff,
                    TiffStackBehavior::AutoBigTiff.to_string(),
                );
                ui.selectable_value(
                    &mut options.stack_behavior,
                    TiffStackBehavior::AlwaysBigTiff,
                    TiffStackBehavior::AlwaysBigTiff.to_string(),
                );
            });
        ui.add_space(2.0);
        ui.label(
            egui::RichText::new("Standard TIFF is the most compatible with ImageJ.")
                .size(10.0)
                .color(colors.text_muted),
        );
    }

    fn render_tiff_availability_warning(ui: &mut egui::Ui) {
        ui.add_space(6.0);
        ui.label(
            egui::RichText::new("No histogram data available for TIFF export.")
                .size(10.0)
                .color(accent::RED),
        );
    }

    fn render_tiff_export_button(
        ui: &mut egui::Ui,
        format: ExportFormat,
        availability: ExportAvailability,
        export_in_progress: bool,
        base_name_ok: bool,
    ) -> bool {
        let can_export =
            availability.histogram.is_available() && !export_in_progress && base_name_ok;
        let label = match format {
            ExportFormat::TiffFolder => "Export TIFF Folder...",
            ExportFormat::TiffStack => "Export TIFF Stack...",
            ExportFormat::Hdf5 => "Save HDF5...",
        };
        ui.add_enabled(can_export, egui::Button::new(label))
            .clicked()
    }

    fn sanitize_export_base_name(value: &str) -> String {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return String::new();
        }
        trimmed
            .chars()
            .map(|c| match c {
                'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' => c,
                _ => '_',
            })
            .collect::<String>()
            .trim_matches('_')
            .to_string()
    }

    fn populate_default_tiff_base_name(&mut self) {
        let options = &mut self.ui_state.export.tiff;
        if !options.base_name.is_empty() && options.base_name != "Run_XXXXX" {
            return;
        }
        let Some(path) = self.selected_file.as_ref() else {
            return;
        };
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            return;
        };
        let sanitized = Self::sanitize_export_base_name(stem);
        if !sanitized.is_empty() {
            options.base_name = sanitized;
        }
    }

    fn export_availability(&self) -> ExportAvailability {
        let hits_available = self.hit_batch.as_ref().is_some_and(|b| !b.is_empty());
        let neutrons_available = !self.neutrons.is_empty();
        let histogram_available = match self.ui_state.view_mode {
            ViewMode::Hits => self.hyperstack.is_some(),
            ViewMode::Neutrons => self.neutron_hyperstack.is_some(),
        };
        let masks_available = self.pixel_masks.is_some();
        let deflate_ok = hdf5::filters::deflate_available();
        ExportAvailability {
            hits: Availability::from(hits_available),
            neutrons: Availability::from(neutrons_available),
            histogram: Availability::from(histogram_available),
            masks: Availability::from(masks_available),
            deflate: Availability::from(deflate_ok),
        }
    }

    fn apply_export_availability(
        options: &mut Hdf5ExportOptions,
        availability: ExportAvailability,
    ) {
        if !availability.hits.is_available() {
            options.datasets.hits = false;
        }
        if !availability.neutrons.is_available() {
            options.datasets.neutrons = false;
        }
        if !availability.histogram.is_available() {
            options.datasets.histogram = false;
        }
        if !availability.masks.is_available() {
            options.masks.pixel_masks = false;
        }
    }

    fn render_export_header(
        ui: &mut egui::Ui,
        colors: &ThemeColors,
        options: &mut Hdf5ExportOptions,
    ) {
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new("Select data to export")
                    .size(11.0)
                    .color(colors.text_primary),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let gear = Self::file_icon_image(FileToolbarIcon::Gear, colors.text_muted);
                if ui
                    .add(egui::ImageButton::new(gear).frame(true))
                    .on_hover_text("Advanced export options")
                    .clicked()
                {
                    options.advanced.enabled = !options.advanced.enabled;
                }
            });
        });
    }

    fn render_export_dataset_options(
        ui: &mut egui::Ui,
        options: &mut Hdf5ExportOptions,
        availability: ExportAvailability,
        hit_count: usize,
        neutron_count: usize,
        view_mode: ViewMode,
    ) {
        let hits_label = if hit_count > 0 {
            format!("Hits ({})", format_number(hit_count))
        } else {
            "Hits".to_string()
        };
        ui.add_enabled(
            availability.hits.is_available(),
            egui::Checkbox::new(&mut options.datasets.hits, hits_label),
        );

        let neutrons_label = if neutron_count > 0 {
            format!("Neutrons ({})", format_number(neutron_count))
        } else {
            "Neutrons".to_string()
        };
        ui.add_enabled(
            availability.neutrons.is_available(),
            egui::Checkbox::new(&mut options.datasets.neutrons, neutrons_label),
        );

        let hist_label = format!("Histogram ({view_mode})");
        ui.add_enabled(
            availability.histogram.is_available(),
            egui::Checkbox::new(&mut options.datasets.histogram, hist_label),
        );

        ui.add_enabled(
            availability.masks.is_available(),
            egui::Checkbox::new(&mut options.masks.pixel_masks, "Pixel masks"),
        );
    }

    fn render_export_deflate_warning(ui: &mut egui::Ui) {
        ui.add_space(6.0);
        ui.label(
            egui::RichText::new("Deflate compression unavailable. Rebuild with HDF5 zlib support.")
                .size(10.0)
                .color(accent::RED),
        );
    }

    fn render_export_advanced(
        ui: &mut egui::Ui,
        colors: &ThemeColors,
        options: &mut Hdf5ExportOptions,
    ) {
        ui.add_space(8.0);
        ui.separator();
        ui.add_space(6.0);

        ui.label(
            egui::RichText::new("Compression")
                .size(11.0)
                .color(colors.text_primary),
        );
        ui.horizontal(|ui| {
            ui.label("Level");
            ui.add(egui::DragValue::new(&mut options.compression_level).range(0..=9));
            ui.checkbox(&mut options.advanced.shuffle, "Shuffle");
        });

        ui.add_space(6.0);
        ui.label(
            egui::RichText::new("Chunking")
                .size(11.0)
                .color(colors.text_primary),
        );
        ui.horizontal(|ui| {
            ui.label("Event chunk");
            ui.add(egui::DragValue::new(&mut options.chunk_events).range(1_000..=5_000_000));
        });

        ui.horizontal(|ui| {
            ui.checkbox(
                &mut options.advanced.hist_chunk_override,
                "Histogram chunk override",
            );
        });

        if options.advanced.hist_chunk_override {
            ui.horizontal(|ui| {
                ui.label("rot");
                ui.add(egui::DragValue::new(&mut options.hist_chunk_rot).range(1..=16));
                ui.label("y");
                ui.add(egui::DragValue::new(&mut options.hist_chunk_y).range(8..=512));
                ui.label("x");
                ui.add(egui::DragValue::new(&mut options.hist_chunk_x).range(8..=512));
                ui.label("tof");
                ui.add(egui::DragValue::new(&mut options.hist_chunk_tof).range(4..=512));
            });
        }

        ui.add_space(6.0);
        ui.label(
            egui::RichText::new("Include fields")
                .size(11.0)
                .color(colors.text_primary),
        );
        ui.horizontal_wrapped(|ui| {
            ui.checkbox(&mut options.fields.xy, "x/y");
            ui.checkbox(&mut options.fields.tot, "tot");
            ui.checkbox(&mut options.fields.chip_id, "chip id");
            ui.checkbox(&mut options.cluster_fields.cluster_id, "cluster id");
            ui.checkbox(&mut options.cluster_fields.n_hits, "n_hits");
        });
    }

    fn render_export_save_button(
        ui: &mut egui::Ui,
        format: ExportFormat,
        options: &Hdf5ExportOptions,
        availability: ExportAvailability,
        export_in_progress: bool,
    ) -> bool {
        let any_selected = options.datasets.hits
            || options.datasets.neutrons
            || options.datasets.histogram
            || options.masks.pixel_masks;
        let deflate_ok = availability.deflate.is_available();
        let can_export = any_selected && deflate_ok && !export_in_progress;

        let label = match format {
            ExportFormat::Hdf5 => "Save HDF5...",
            ExportFormat::TiffFolder => "Export TIFF Folder...",
            ExportFormat::TiffStack => "Export TIFF Stack...",
        };
        ui.add_enabled(can_export, egui::Button::new(label))
            .clicked()
    }

    fn file_icon_image(icon: FileToolbarIcon, tint: Color32) -> egui::Image<'static> {
        let source = match icon {
            FileToolbarIcon::Open => egui::include_image!("../../assets/icons/file-open.svg"),
            FileToolbarIcon::Export => egui::include_image!("../../assets/icons/file-save.svg"),
            FileToolbarIcon::Gear => egui::include_image!("../../assets/icons/roi-gear.svg"),
        };
        egui::Image::new(source)
            .tint(tint)
            .fit_to_exact_size(egui::vec2(16.0, 16.0))
    }

    fn cache_icon_image(icon: CacheModeIcon, tint: Color32) -> egui::Image<'static> {
        let source = match icon {
            CacheModeIcon::Memory => egui::include_image!("../../assets/icons/cache-ram.svg"),
            CacheModeIcon::Stream => egui::include_image!("../../assets/icons/cache-stream.svg"),
        };
        egui::Image::new(source)
            .tint(tint)
            .fit_to_exact_size(egui::vec2(16.0, 16.0))
    }
}

#[derive(Clone, Copy)]
enum CacheModeIcon {
    Memory,
    Stream,
}

#[derive(Clone, Copy)]
enum Availability {
    Available,
    Unavailable,
}

impl Availability {
    fn is_available(self) -> bool {
        matches!(self, Self::Available)
    }
}

impl From<bool> for Availability {
    fn from(value: bool) -> Self {
        if value {
            Self::Available
        } else {
            Self::Unavailable
        }
    }
}

#[derive(Clone, Copy)]
struct ExportAvailability {
    hits: Availability,
    neutrons: Availability,
    histogram: Availability,
    masks: Availability,
    deflate: Availability,
}
