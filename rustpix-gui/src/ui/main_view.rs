//! Main view (central panel) rendering.

use std::fs::File;
use std::io::Write;

use eframe::egui::{self, Color32, Rounding, Stroke, Vec2b};
use egui_plot::{Line, Plot, PlotBounds, PlotImage, PlotPoint, PlotPoints, VLine};
use image::{Rgba, RgbaImage};
use rfd::FileDialog;

use super::theme::{accent, ThemeColors};
use crate::app::RustpixApp;
use crate::state::{SpectrumXAxis, ViewMode};
use crate::util::{
    energy_ev_to_tof_ms, f64_to_usize_bounded, tof_ms_to_energy_ev, u64_to_f64, usize_to_f64,
};
use crate::viewer::RoiSelectionMode;

/// Unique ID for the main histogram plot (used for state persistence).
const HISTOGRAM_PLOT_ID: &str = "histogram_plot";

struct SpectrumExportConfig {
    axis: SpectrumXAxis,
    flight_path_m: f64,
    tof_offset_ns: f64,
    log_x: bool,
    log_y: bool,
}

#[allow(clippy::cast_possible_truncation)]
fn round_to_i32_clamped(value: f64) -> i32 {
    if !value.is_finite() {
        return 0;
    }
    let clamped = value
        .round()
        .clamp(f64::from(i32::MIN), f64::from(i32::MAX));
    clamped as i32
}

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
        let needs_plot_reset = self.ui_state.needs_plot_reset;
        let show_grid = self.ui_state.show_grid;
        let wants_keyboard = ctx.wants_keyboard_input();
        let delete_roi = ctx.input(|i| {
            !wants_keyboard
                && (i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace))
        });
        let exit_edit_mode = ctx.input(|i| i.key_pressed(egui::Key::Escape));
        let commit_polygon = !wants_keyboard
            && self.roi_state.polygon_draft.is_some()
            && ctx.input(|i| i.key_pressed(egui::Key::Enter));

        // Get data bounds based on view mode
        // TODO: Neutron mode may have different bounds due to super-resolution
        #[allow(clippy::match_same_arms)]
        let data_size: f64 = match self.ui_state.view_mode {
            ViewMode::Hits => 512.0,
            ViewMode::Neutrons => 512.0,
        };

        // Track if bin changed via interaction
        let mut new_tof_bin: Option<usize> = None;
        // Track if user clicked reset view button
        let mut reset_view_clicked = false;

        if delete_roi {
            self.roi_state.delete_selected();
        }
        if exit_edit_mode {
            self.roi_state.clear_edit_mode();
            self.roi_state.cancel_draft();
        }
        if commit_polygon {
            if let Err(err) = self.roi_state.commit_polygon(3) {
                let message = match err {
                    crate::viewer::RoiCommitError::TooFewPoints => {
                        "Polygon needs at least 3 points".to_string()
                    }
                    crate::viewer::RoiCommitError::SelfIntersecting => {
                        "Polygon edges cannot self-intersect".to_string()
                    }
                };
                let expires_at = ctx.input(|i| i.time) + 2.5;
                self.ui_state.roi_warning = Some((message, expires_at));
                self.roi_state.cancel_draft();
            }
        }

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
                                let texture_id = self.texture.as_ref().map(egui::TextureHandle::id);
                                if let Some(tex_id) = texture_id {
                                    // Toolbar row above the plot
                                    ui.horizontal(|ui| {
                                        self.render_roi_toolbar(ui);

                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                let colors = ThemeColors::from_ui(ui);
                                                // Grid toggle
                                                let grid_btn = egui::Button::new(
                                                    egui::RichText::new("▦ Grid").size(10.0).color(
                                                        if self.ui_state.show_grid {
                                                            Color32::WHITE
                                                        } else {
                                                            colors.text_muted
                                                        },
                                                    ),
                                                )
                                                .fill(if self.ui_state.show_grid {
                                                    accent::BLUE
                                                } else {
                                                    Color32::TRANSPARENT
                                                })
                                                .stroke(Stroke::new(1.0, colors.border_light))
                                                .rounding(Rounding::same(4.0));

                                                if ui
                                                    .add(grid_btn)
                                                    .on_hover_text("Toggle grid")
                                                    .clicked()
                                                {
                                                    self.ui_state.show_grid =
                                                        !self.ui_state.show_grid;
                                                }

                                                ui.add_space(8.0);

                                                // Reset View button
                                                let reset_btn = egui::Button::new(
                                                    egui::RichText::new("↺ Reset View")
                                                        .size(10.0)
                                                        .color(colors.text_muted),
                                                )
                                                .fill(Color32::TRANSPARENT)
                                                .stroke(Stroke::new(1.0, colors.border_light))
                                                .rounding(Rounding::same(4.0));

                                                if ui
                                                    .add(reset_btn)
                                                    .on_hover_text(
                                                        "Reset view to fit data (or double-click)",
                                                    )
                                                    .clicked()
                                                {
                                                    reset_view_clicked = true;
                                                }
                                            },
                                        );
                                    });
                                    ui.add_space(4.0);

                                    let half = data_size / 2.0;
                                    #[allow(clippy::cast_possible_truncation)]
                                    let data_size_f32 = data_size as f32;

                                    // Determine if we need to reset the plot view
                                    let should_reset = needs_plot_reset || reset_view_clicked;
                                    let plot_rect = ui.available_rect_before_wrap();
                                    let shift_down = ctx.input(|i| i.modifiers.shift);
                                    let handle_radius = 3.0;
                                    let pre_drag_hit = if !shift_down
                                        && ctx.input(|i| {
                                            i.pointer
                                                .button_down(egui::PointerButton::Primary)
                                        }) {
                                        if let (Some(bounds), Some(rect), Some(pos)) = (
                                            self.ui_state.roi_last_plot_bounds,
                                            self.ui_state.roi_last_plot_rect,
                                            ctx.input(|i| i.pointer.interact_pos()),
                                        ) {
                                            if rect.contains(pos) && rect.width() > 0.0 && rect.height() > 0.0 {
                                                let x_frac = f64::from(pos.x - rect.left())
                                                    / f64::from(rect.width());
                                                let y_frac = f64::from(pos.y - rect.top())
                                                    / f64::from(rect.height());
                                                if (0.0..=1.0).contains(&x_frac)
                                                    && (0.0..=1.0).contains(&y_frac)
                                                {
                                                    let plot_x = bounds.min()[0]
                                                        + x_frac
                                                            * (bounds.max()[0] - bounds.min()[0]);
                                                    let plot_y = bounds.max()[1]
                                                        - y_frac
                                                            * (bounds.max()[1] - bounds.min()[1]);
                                                    let point = PlotPoint::new(plot_x, plot_y);
                                                    self.roi_state
                                                        .hit_test_handle(point, handle_radius)
                                                        .is_some()
                                                        || self.roi_state
                                                            .hit_test_vertex(point, handle_radius)
                                                            .is_some()
                                                        || self.roi_state
                                                            .hit_test_edge(point, handle_radius)
                                                            .is_some()
                                                        || self.roi_state.hit_test(point).is_some()
                                                } else {
                                                    false
                                                }
                                            } else {
                                                false
                                            }
                                        } else {
                                            false
                                        }
                                    } else {
                                        false
                                    };
                                    let roi_drag_active = self.roi_state.is_dragging()
                                        || self.roi_state.is_edit_dragging();
                                    let roi_drawing_active = self.roi_state.draft.is_some()
                                        || self.roi_state.polygon_draft.is_some();
                                    let disable_plot_drag = shift_down
                                        || roi_drag_active
                                        || roi_drawing_active
                                        || pre_drag_hit;

                                    // Build the base plot
                                    let mut plot = Plot::new(HISTOGRAM_PLOT_ID)
                                        .data_aspect(1.0)
                                        .auto_bounds(Vec2b::new(false, false))
                                        .include_x(0.0)
                                        .include_x(data_size)
                                        .include_y(0.0)
                                        .include_y(data_size)
                                        .show_grid(Vec2b::new(show_grid, show_grid))
                                        .x_axis_label("X (pixels)")
                                        .y_axis_label("Y (pixels)")
                                        .allow_drag(!disable_plot_drag);

                                    // Apply reset if needed
                                    if should_reset {
                                        plot = plot.reset();
                                    }

                                    let roi_mode = self.roi_state.mode;
                                    let min_roi_size = 2.0;
                                    plot.show(ui, |plot_ui| {
                                        // Set explicit bounds on reset or double-click
                                        if should_reset || plot_ui.response().double_clicked() {
                                            let pad = (data_size * 0.05).max(16.0);

                                            // Fit the full detector + padding to the current plot aspect.
                                            let plot_w = f64::from(plot_rect.width().max(1.0));
                                            let plot_h = f64::from(plot_rect.height().max(1.0));
                                            let available_aspect = plot_w / plot_h;
                                            let data_span = data_size + pad * 2.0;
                                            let mut x_half = data_span / 2.0;
                                            let mut y_half = data_span / 2.0;

                                            if available_aspect >= 1.0 {
                                                x_half = y_half * available_aspect;
                                            } else {
                                                y_half = x_half / available_aspect;
                                            }

                                            let center = data_size / 2.0;
                                            plot_ui.set_plot_bounds(PlotBounds::from_min_max(
                                                [center - x_half, center - y_half],
                                                [center + x_half, center + y_half],
                                            ));
                                        }
                                        plot_ui.image(PlotImage::new(
                                            tex_id,
                                            PlotPoint::new(half, half),
                                            [data_size_f32, data_size_f32],
                                        ));

                                        let clamp_point = |point: PlotPoint| {
                                            PlotPoint::new(
                                                point.x.clamp(0.0, data_size),
                                                point.y.clamp(0.0, data_size),
                                            )
                                        };

                                        let response = plot_ui.response().clone();
                                        let pointer_pos =
                                            plot_ui.pointer_coordinate().map(clamp_point);

                                        let rect_drawing = roi_mode == RoiSelectionMode::Rectangle
                                            && (shift_down || self.roi_state.draft.is_some());
                                        let poly_drawing = roi_mode == RoiSelectionMode::Polygon
                                            && (shift_down
                                                || self.roi_state.polygon_draft.is_some());
                                        if response.hovered() {
                                            if rect_drawing || poly_drawing {
                                                plot_ui
                                                    .ctx()
                                                    .set_cursor_icon(egui::CursorIcon::Crosshair);
                                            } else if let Some(pos) = pointer_pos {
                                                if let Some((_, handle)) = self
                                                    .roi_state
                                                    .hit_test_handle(pos, handle_radius)
                                                {
                                                    let icon = match handle {
                                                        crate::viewer::RoiHandle::North
                                                        | crate::viewer::RoiHandle::South => {
                                                            egui::CursorIcon::ResizeVertical
                                                        }
                                                        crate::viewer::RoiHandle::East
                                                        | crate::viewer::RoiHandle::West => {
                                                            egui::CursorIcon::ResizeHorizontal
                                                        }
                                                        crate::viewer::RoiHandle::NorthEast
                                                        | crate::viewer::RoiHandle::SouthWest => {
                                                            egui::CursorIcon::ResizeNeSw
                                                        }
                                                        crate::viewer::RoiHandle::NorthWest
                                                        | crate::viewer::RoiHandle::SouthEast => {
                                                            egui::CursorIcon::ResizeNwSe
                                                        }
                                                    };
                                                    plot_ui.ctx().set_cursor_icon(icon);
                                                } else if self
                                                    .roi_state
                                                    .hit_test_vertex(pos, handle_radius)
                                                    .is_some()
                                                {
                                                    plot_ui.ctx().set_cursor_icon(
                                                        egui::CursorIcon::Crosshair,
                                                    );
                                                } else if self
                                                    .roi_state
                                                    .hit_test_edge(pos, handle_radius)
                                                    .is_some()
                                                {
                                                    plot_ui.ctx().set_cursor_icon(
                                                        egui::CursorIcon::PointingHand,
                                                    );
                                                } else if self.roi_state.hit_test(pos).is_some() {
                                                    plot_ui
                                                        .ctx()
                                                        .set_cursor_icon(egui::CursorIcon::Grab);
                                                }
                                            }
                                        }

                                        if rect_drawing {
                                            if response.drag_started() {
                                                if let Some(pos) = pointer_pos {
                                                    self.roi_state.begin_rectangle(pos);
                                                }
                                            }
                                            if response.dragged() {
                                                if let Some(pos) = pointer_pos {
                                                    self.roi_state.update_rectangle(pos);
                                                }
                                            }
                                            if response.drag_stopped() {
                                                self.roi_state.commit_rectangle(min_roi_size);
                                            }
                                        } else if poly_drawing {
                                            self.roi_state.update_polygon_hover(pointer_pos);
                                            if response.clicked() {
                                                if let Some(pos) = pointer_pos {
                                                    self.roi_state.add_polygon_point(pos);
                                                }
                                            }
                                        } else {
                                            if response.drag_started() {
                                                if let Some(pos) = pointer_pos {
                                                    if let Some((roi_id, index)) = self
                                                        .roi_state
                                                        .hit_test_vertex(pos, handle_radius)
                                                    {
                                                        let bounds = plot_ui.plot_bounds();
                                                        self.roi_state.start_vertex_drag(
                                                            roi_id, index, pos, bounds,
                                                        );
                                                    } else if let Some((hit_id, handle)) = self
                                                        .roi_state
                                                        .hit_test_handle(pos, handle_radius)
                                                    {
                                                        let bounds = plot_ui.plot_bounds();
                                                        self.roi_state.start_edit_drag(
                                                            hit_id, handle, pos, bounds,
                                                        );
                                                    } else if let Some(hit_id) =
                                                        self.roi_state.hit_test(pos)
                                                    {
                                                        let bounds = plot_ui.plot_bounds();
                                                        self.roi_state
                                                            .start_drag(hit_id, pos, bounds);
                                                    }
                                                }
                                            }
                                            if response.dragged() {
                                                if let Some(pos) = pointer_pos {
                                                    if self.roi_state.is_edit_dragging() {
                                                        self.roi_state.update_vertex_drag(pos);
                                                        self.roi_state.update_edit_drag(
                                                            pos,
                                                            min_roi_size,
                                                            0.0,
                                                            data_size,
                                                        );
                                                    } else {
                                                        self.roi_state
                                                            .update_drag(pos, 0.0, data_size);
                                                    }
                                                }
                                            }
                                            if response.drag_stopped() {
                                                if let Err(err) =
                                                    self.roi_state.end_vertex_drag()
                                                {
                                                    let message = match err {
                                                        crate::viewer::RoiCommitError::TooFewPoints => {
                                                            "Polygon needs at least 3 points".to_string()
                                                        }
                                                        crate::viewer::RoiCommitError::SelfIntersecting => {
                                                            "Polygon edges cannot self-intersect".to_string()
                                                        }
                                                    };
                                                    let expires_at = ctx.input(|i| i.time) + 2.5;
                                                    self.ui_state.roi_warning =
                                                        Some((message, expires_at));
                                                }
                                                self.roi_state.end_edit_drag();
                                                self.roi_state.end_drag();
                                            }

                                            // Plot dragging is disabled during ROI interactions; no bounds reset needed.
                                        }

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

                                        if response.clicked()
                                            && self.roi_state.draft.is_none()
                                            && self.roi_state.polygon_draft.is_none()
                                            && !shift_down
                                            && !self.roi_state.is_dragging()
                                            && !self.roi_state.is_edit_dragging()
                                        {
                                            if let Some(pos) = pointer_pos {
                                                match self
                                                    .roi_state
                                                    .insert_vertex_at(pos, handle_radius)
                                                {
                                                    Ok(true) => {
                                                        self.roi_state.select_at(pos);
                                                    }
                                                    Ok(false) => {
                                                        if self.roi_state.hit_test(pos).is_some() {
                                                            self.roi_state.select_at(pos);
                                                        } else {
                                                            self.roi_state.clear_edit_mode();
                                                        }
                                                    }
                                                    Err(err) => {
                                                        let message = match err {
                                                            crate::viewer::RoiCommitError::TooFewPoints => {
                                                                "Polygon needs at least 3 points".to_string()
                                                            }
                                                            crate::viewer::RoiCommitError::SelfIntersecting => {
                                                                "Polygon edges cannot self-intersect".to_string()
                                                            }
                                                        };
                                                        let expires_at =
                                                            ctx.input(|i| i.time) + 2.5;
                                                        self.ui_state.roi_warning =
                                                            Some((message, expires_at));
                                                    }
                                                }
                                            }
                                        }

                                        if response.double_clicked()
                                            && self.roi_state.draft.is_none()
                                        {
                                            if let Some(pos) = pointer_pos {
                                                if let Some(hit_id) = self.roi_state.hit_test(pos) {
                                                    self.roi_state.set_edit_mode(hit_id, true);
                                                } else {
                                                    self.roi_state.clear_edit_mode();
                                                }
                                            }
                                        }

                                        let mut suppress_context_menu = false;
                                        if response.secondary_clicked() {
                                            let mut target = None;
                                            if let Some(pos) = pointer_pos {
                                                if self
                                                    .roi_state
                                                    .delete_vertex_at(pos, handle_radius)
                                                {
                                                    self.roi_state.set_context_menu(None);
                                                    suppress_context_menu = true;
                                                } else if let Some(hit_id) =
                                                    self.roi_state.hit_test(pos)
                                                {
                                                    self.roi_state.select_id(hit_id);
                                                    target = Some(hit_id);
                                                }
                                            }
                                            self.roi_state.set_context_menu(target);
                                        }

                                        response.context_menu(|ui| {
                                            if suppress_context_menu {
                                                ui.close_menu();
                                                return;
                                            }
                                            if let Some(target) =
                                                self.roi_state.context_menu_target()
                                            {
                                                if ui.button("Edit").clicked() {
                                                    self.roi_state.set_edit_mode(target, true);
                                                    ui.close_menu();
                                                }
                                                if ui.button("Delete").clicked() {
                                                    self.roi_state.delete_id(target);
                                                    ui.close_menu();
                                                }
                                            } else {
                                                ui.label("No ROI");
                                            }
                                        });

                                        self.roi_state.draw(plot_ui);
                                        self.roi_state.draw_draft(plot_ui);

                                        self.ui_state.roi_last_plot_bounds =
                                            Some(plot_ui.plot_bounds());
                                        self.ui_state.roi_last_plot_rect =
                                            Some(plot_ui.response().rect);
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
                    let margin = egui::Margin::symmetric(16.0, 12.0);
                    let content_height = ui.spacing().interact_size.y.max(20.0);
                    let frame_height = content_height + margin.top + margin.bottom;

                    let left = ui.max_rect().left();
                    let right = ui.max_rect().right();
                    let top = ui.cursor().top();
                    let rect = egui::Rect::from_min_max(
                        egui::pos2(left, top),
                        egui::pos2(right, top + frame_height),
                    );
                    let _ = ui.allocate_rect(rect, egui::Sense::hover());

                    // Draw frame manually at full width.
                    ui.painter().rect_filled(rect, 4.0, colors.bg_panel);
                    ui.painter()
                        .rect_stroke(rect, 4.0, Stroke::new(1.0, colors.border));

                    let inner_rect = rect.shrink2(egui::vec2(margin.left, margin.top));
                    let mut slicer_ui = ui.new_child(
                        egui::UiBuilder::new()
                            .max_rect(inner_rect)
                            .layout(egui::Layout::left_to_right(egui::Align::Center)),
                    );

                    let colors = ThemeColors::from_ui(&slicer_ui);
                    slicer_ui.set_width(inner_rect.width());

                    // Clamp to valid range
                    let clamped_bin = current_tof_bin.min(n_bins - 1);
                    let mut bin = clamped_bin;

                    // Use horizontal layout with fixed label widths
                    let total_width = inner_rect.width();
                    let label_width = 70.0;
                    let value_width = 70.0;
                    let spacing = slicer_ui.spacing().item_spacing.x;
                    let slider_width =
                        (total_width - label_width - value_width - spacing * 2.0).max(120.0);

                    let prev_slider_width = slicer_ui.spacing().slider_width;
                    slicer_ui.spacing_mut().slider_width = slider_width;
                    slicer_ui.allocate_ui_with_layout(
                        egui::vec2(label_width, slicer_ui.available_height()),
                        egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
                        |ui| {
                            ui.label(
                                egui::RichText::new("TOF Slice")
                                    .size(11.0)
                                    .color(colors.text_muted),
                            );
                        },
                    );

                    let slider = slicer_ui.add(
                        egui::Slider::new(&mut bin, 0..=(n_bins - 1))
                            .show_value(false)
                            .clamping(egui::SliderClamping::Always),
                    );
                    slicer_ui.spacing_mut().slider_width = prev_slider_width;

                    slicer_ui.allocate_ui_with_layout(
                        egui::vec2(value_width, slicer_ui.available_height()),
                        egui::Layout::right_to_left(egui::Align::Center),
                        |ui| {
                            let colors = ThemeColors::from_ui(ui);
                            ui.label(
                                egui::RichText::new(format!("{} / {}", bin + 1, n_bins))
                                    .size(11.0)
                                    .color(colors.text_primary),
                            );
                        },
                    );

                    if slider.changed() && bin != current_tof_bin {
                        new_tof_bin = Some(bin);
                    }
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

        // Clear the reset flag after rendering
        if needs_plot_reset || reset_view_clicked {
            self.ui_state.needs_plot_reset = false;
        }
    }

    /// Render the colorbar legend.
    #[allow(clippy::cast_precision_loss)]
    fn render_colorbar(&self, ui: &mut egui::Ui) {
        let colors = ThemeColors::from_ui(ui);
        ui.vertical(|ui| {
            // "max" label at top
            ui.horizontal(|ui| {
                ui.add_space(2.0);
                ui.label(egui::RichText::new("max").size(9.0).color(colors.text_dim));
            });
            ui.add_space(4.0);

            // Gradient bar
            let gradient_height = ui.available_height() - 24.0; // Reserve space for "0" label
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

            // "0" label at bottom
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.add_space(2.0);
                ui.label(egui::RichText::new("0").size(9.0).color(colors.text_dim));
            });
        });
    }

    /// Render ROI tool group controls.
    fn render_roi_toolbar(&mut self, ui: &mut egui::Ui) {
        let colors = ThemeColors::from_ui(ui);
        let mode_icon = match self.roi_state.mode {
            RoiSelectionMode::Rectangle => "▭",
            RoiSelectionMode::Polygon => "⬠",
        };

        egui::Frame::none()
            .stroke(Stroke::new(1.0, colors.border_light))
            .rounding(Rounding::same(4.0))
            .inner_margin(egui::Margin::symmetric(4.0, 2.0))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let mut selection_mode = self.roi_state.mode;
                    egui::ComboBox::from_id_salt("roi_selection_mode")
                        .selected_text(egui::RichText::new(mode_icon).size(11.0))
                        .width(36.0)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut selection_mode,
                                RoiSelectionMode::Rectangle,
                                "▭ Rectangle",
                            );
                            ui.selectable_value(
                                &mut selection_mode,
                                RoiSelectionMode::Polygon,
                                "⬠ Polygon",
                            );
                        });
                    if selection_mode != self.roi_state.mode {
                        self.roi_state.mode = selection_mode;
                        self.roi_state.cancel_draft();
                    }

                    let clear_btn = egui::Button::new(
                        egui::RichText::new("✕").size(11.0).color(colors.text_muted),
                    )
                    .fill(Color32::TRANSPARENT)
                    .stroke(Stroke::new(1.0, colors.border_light))
                    .rounding(Rounding::same(4.0));
                    if ui.add(clear_btn).on_hover_text("Clear all ROIs").clicked() {
                        self.roi_state.clear();
                    }

                    ui.menu_button(
                        egui::RichText::new("⚙").size(11.0).color(colors.text_muted),
                        |ui| {
                            ui.checkbox(
                                &mut self.roi_state.debounce_updates,
                                "Debounce spectrum updates",
                            );
                        },
                    )
                    .response
                    .on_hover_text("ROI settings");
                });
            });

        ui.add_space(8.0);
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

        // Track if spectrum reset was clicked
        let mut spectrum_reset_clicked = false;
        if self.ui_state.spectrum_x_axis == SpectrumXAxis::EnergyEv && self.flight_path_m <= 0.0 {
            self.ui_state.spectrum_x_axis = SpectrumXAxis::ToFMs;
            spectrum_reset_clicked = true;
        }
        let has_spectrum = spectrum.as_ref().is_some_and(|s| !s.is_empty());
        let mut export_png_clicked = false;
        let mut export_csv_clicked = false;

        // Spectrum toolbar
        egui::Frame::none()
            .fill(colors.bg_panel)
            .stroke(Stroke::new(1.0, colors.border))
            .rounding(Rounding::same(4.0))
            .inner_margin(egui::Margin::symmetric(12.0, 8.0))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let colors = ThemeColors::from_ui(ui);
                    let energy_available = self.flight_path_m > 0.0;
                    let prev_axis = self.ui_state.spectrum_x_axis;

                    // TOF/Energy selector
                    egui::ComboBox::from_id_salt("tof_unit")
                        .selected_text(self.ui_state.spectrum_x_axis.to_string())
                        .width(90.0)
                        .show_ui(ui, |ui| {
                            if ui
                                .selectable_label(
                                    self.ui_state.spectrum_x_axis == SpectrumXAxis::ToFMs,
                                    SpectrumXAxis::ToFMs.to_string(),
                                )
                                .clicked()
                            {
                                self.ui_state.spectrum_x_axis = SpectrumXAxis::ToFMs;
                            }

                            if ui
                                .add_enabled(
                                    energy_available,
                                    egui::SelectableLabel::new(
                                        self.ui_state.spectrum_x_axis == SpectrumXAxis::EnergyEv,
                                        SpectrumXAxis::EnergyEv.to_string(),
                                    ),
                                )
                                .on_hover_text(if energy_available {
                                    "Energy axis"
                                } else {
                                    "Set flight path in spectrum settings"
                                })
                                .clicked()
                            {
                                self.ui_state.spectrum_x_axis = SpectrumXAxis::EnergyEv;
                            }
                        });

                    if prev_axis != self.ui_state.spectrum_x_axis {
                        spectrum_reset_clicked = true;
                    }

                    // Spectrum settings
                    if ui
                        .add(
                            egui::Button::new("⚙")
                                .fill(Color32::TRANSPARENT)
                                .stroke(Stroke::new(1.0, colors.border_light))
                                .rounding(Rounding::same(4.0)),
                        )
                        .on_hover_text("Spectrum settings")
                        .clicked()
                    {
                        self.ui_state.show_spectrum_settings =
                            !self.ui_state.show_spectrum_settings;
                    }

                    ui.add_space(8.0);

                    // Log axis toggles
                    let logx_btn = egui::Button::new(egui::RichText::new("logX").size(10.0).color(
                        if self.ui_state.log_x {
                            Color32::WHITE
                        } else {
                            colors.text_dim
                        },
                    ))
                    .fill(if self.ui_state.log_x {
                        accent::BLUE
                    } else {
                        Color32::TRANSPARENT
                    })
                    .stroke(Stroke::new(1.0, colors.border_light))
                    .rounding(Rounding::same(4.0));
                    if ui.add(logx_btn).clicked() {
                        self.ui_state.log_x = !self.ui_state.log_x;
                        spectrum_reset_clicked = true;
                    }

                    let logy_btn = egui::Button::new(egui::RichText::new("logY").size(10.0).color(
                        if self.ui_state.log_y {
                            Color32::WHITE
                        } else {
                            colors.text_dim
                        },
                    ))
                    .fill(if self.ui_state.log_y {
                        accent::BLUE
                    } else {
                        Color32::TRANSPARENT
                    })
                    .stroke(Stroke::new(1.0, colors.border_light))
                    .rounding(Rounding::same(4.0));
                    if ui.add(logy_btn).clicked() {
                        self.ui_state.log_y = !self.ui_state.log_y;
                        spectrum_reset_clicked = true;
                    }

                    ui.add_space(8.0);
                    Self::toolbar_divider(ui);
                    ui.add_space(8.0);

                    // Export buttons
                    let png_btn = egui::Button::new(
                        egui::RichText::new("📷 PNG")
                            .size(10.0)
                            .color(colors.text_dim),
                    )
                    .fill(Color32::TRANSPARENT)
                    .stroke(Stroke::new(1.0, colors.border_light))
                    .rounding(Rounding::same(4.0));
                    if ui
                        .add_enabled(has_spectrum, png_btn)
                        .on_hover_text("Export spectrum as PNG")
                        .clicked()
                    {
                        export_png_clicked = true;
                    }

                    let csv_btn = egui::Button::new(
                        egui::RichText::new("💾 CSV")
                            .size(10.0)
                            .color(colors.text_dim),
                    )
                    .fill(Color32::TRANSPARENT)
                    .stroke(Stroke::new(1.0, colors.border_light))
                    .rounding(Rounding::same(4.0));
                    if ui
                        .add_enabled(has_spectrum, csv_btn)
                        .on_hover_text("Export spectrum as CSV")
                        .clicked()
                    {
                        export_csv_clicked = true;
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let colors = ThemeColors::from_ui(ui);
                        // Legend
                        ui.horizontal(|ui| {
                            let colors = ThemeColors::from_ui(ui);
                            // Full legend
                            ui.add(Self::legend_box(colors.text_muted));
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
                            .on_hover_text("Reset spectrum view (or double-click)")
                            .clicked()
                        {
                            spectrum_reset_clicked = true;
                        }
                    });
                });
            });

        ui.add_space(4.0);

        // Spectrum plot
        let log_x = self.ui_state.log_x;
        let log_y = self.ui_state.log_y;
        if let Some(full) = spectrum.as_ref() {
            let colors = ThemeColors::from_ui(ui);
            let tdc_period = 1.0 / self.tdc_frequency;
            let max_ms = tdc_period * 1e3;
            let spec_bins = full.len();
            let bin_width_ms = max_ms / usize_to_f64(spec_bins);
            let axis = self.ui_state.spectrum_x_axis;
            let flight_path_m = self.flight_path_m;
            let tof_offset_ns = self.tof_offset_ns;

            let mut points = Vec::with_capacity(spec_bins);
            let mut x_min = f64::INFINITY;
            let mut x_max = f64::NEG_INFINITY;
            let mut y_max = 0.0;

            for (i, &c) in full.iter().enumerate() {
                let tof_ms = usize_to_f64(i) * bin_width_ms;
                let mut x = match axis {
                    SpectrumXAxis::ToFMs => tof_ms,
                    SpectrumXAxis::EnergyEv => {
                        let Some(e) = tof_ms_to_energy_ev(tof_ms, flight_path_m, tof_offset_ns)
                        else {
                            continue;
                        };
                        e
                    }
                };
                if log_x {
                    if x <= 0.0 {
                        continue;
                    }
                    x = x.log10();
                }

                let mut y = u64_to_f64(c);
                if log_y {
                    y = u64_to_f64(c.max(1)).log10();
                }

                if y > y_max {
                    y_max = y;
                }
                if x < x_min {
                    x_min = x;
                }
                if x > x_max {
                    x_max = x;
                }
                points.push([x, y]);
            }

            let x_span = x_max - x_min;
            if !x_min.is_finite() || !x_max.is_finite() || x_span.abs() <= f64::EPSILON {
                x_min = 0.0;
                x_max = 1.0;
            }
            if y_max <= 0.0 {
                y_max = 1.0;
            } else {
                y_max *= 1.05;
            }

            let x_label = match axis {
                SpectrumXAxis::ToFMs => {
                    if log_x {
                        "log10(TOF (ms))"
                    } else {
                        "TOF (ms)"
                    }
                }
                SpectrumXAxis::EnergyEv => {
                    if log_x {
                        "log10(Energy (eV))"
                    } else {
                        "Energy (eV)"
                    }
                }
            };
            let y_label = if log_y { "log10(Counts)" } else { "Counts" };

            let line_color = colors.text_muted;

            // Build the base spectrum plot
            let mut spectrum_plot = Plot::new("spectrum")
                .height(140.0)
                .x_axis_label(x_label)
                .y_axis_label(y_label)
                .include_x(x_min)
                .include_x(x_max)
                .include_y(0.0);

            // Apply reset if needed
            if spectrum_reset_clicked {
                spectrum_plot = spectrum_plot.reset();
            }

            let points = points;
            let plot_response = spectrum_plot.show(ui, |plot_ui| {
                // Reset on button click or double-click - show ALL data
                if spectrum_reset_clicked || plot_ui.response().double_clicked() {
                    plot_ui.set_plot_bounds(PlotBounds::from_min_max([x_min, 0.0], [x_max, y_max]));
                }

                // Full spectrum as line
                plot_ui.line(
                    Line::new(PlotPoints::new(points))
                        .color(line_color)
                        .name("Full"),
                );

                // Slice marker
                if slicer_enabled && current_tof_bin < spec_bins {
                    let slice_tof_ms = usize_to_f64(current_tof_bin) * bin_width_ms;
                    let slice_x = match axis {
                        SpectrumXAxis::ToFMs => Some(slice_tof_ms),
                        SpectrumXAxis::EnergyEv => {
                            tof_ms_to_energy_ev(slice_tof_ms, flight_path_m, tof_offset_ns)
                        }
                    };

                    if let Some(mut slice_x) = slice_x {
                        if log_x {
                            if slice_x > 0.0 {
                                slice_x = slice_x.log10();
                            } else {
                                slice_x = x_min;
                            }
                        }
                        plot_ui.vline(
                            VLine::new(slice_x)
                                .color(accent::RED)
                                .width(1.0)
                                .style(egui_plot::LineStyle::Dashed { length: 4.0 })
                                .name(format!("Slice {}", current_tof_bin + 1)),
                        );
                    }
                }

                // Handle drag to move slice marker
                if slicer_enabled && spec_bins > 0 {
                    let drag_delta = plot_ui.pointer_coordinate_drag_delta();
                    if drag_delta.x.abs() > 0.0 {
                        if let Some(coord) = plot_ui.pointer_coordinate() {
                            let mut x_axis = coord.x;
                            if log_x {
                                x_axis = 10_f64.powf(x_axis);
                            }
                            let x_ms = match axis {
                                SpectrumXAxis::ToFMs => Some(x_axis),
                                SpectrumXAxis::EnergyEv => {
                                    energy_ev_to_tof_ms(x_axis, flight_path_m, tof_offset_ns)
                                }
                            };
                            let Some(x_ms) = x_ms else {
                                return;
                            };
                            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                            let bin = if x_ms <= 0.0 {
                                0
                            } else if x_ms >= max_ms {
                                spec_bins - 1
                            } else {
                                ((x_ms / bin_width_ms) as usize).min(spec_bins - 1)
                            };
                            if bin != current_tof_bin {
                                *new_tof_bin = Some(bin);
                            }
                        }
                    }
                }
            });

            // Click to set slice position
            if slicer_enabled && spec_bins > 0 && plot_response.response.clicked() {
                if let Some(pos) = plot_response.response.interact_pointer_pos() {
                    let plot_bounds = plot_response.transform.bounds();
                    let plot_rect = plot_response.response.rect;
                    let x_frac = f64::from(pos.x - plot_rect.left()) / f64::from(plot_rect.width());
                    let x_plot = plot_bounds.min()[0]
                        + x_frac * (plot_bounds.max()[0] - plot_bounds.min()[0]);
                    let mut x_axis = x_plot;
                    if log_x {
                        x_axis = 10_f64.powf(x_axis);
                    }
                    let x_ms = match axis {
                        SpectrumXAxis::ToFMs => Some(x_axis),
                        SpectrumXAxis::EnergyEv => {
                            energy_ev_to_tof_ms(x_axis, flight_path_m, tof_offset_ns)
                        }
                    };
                    let Some(x_ms) = x_ms else {
                        return;
                    };

                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    let bin = if x_ms <= 0.0 {
                        0
                    } else if x_ms >= max_ms {
                        spec_bins - 1
                    } else {
                        ((x_ms / bin_width_ms) as usize).min(spec_bins - 1)
                    };
                    if bin != current_tof_bin {
                        *new_tof_bin = Some(bin);
                    }
                }
            }

            if export_csv_clicked {
                if let Err(err) = Self::export_spectrum_csv(
                    full,
                    bin_width_ms,
                    axis,
                    flight_path_m,
                    tof_offset_ns,
                ) {
                    log::error!("Failed to export spectrum CSV: {err}");
                }
            }

            if export_png_clicked {
                let export_config = SpectrumExportConfig {
                    axis,
                    flight_path_m,
                    tof_offset_ns,
                    log_x,
                    log_y,
                };
                if let Err(err) = Self::export_spectrum_png(full, bin_width_ms, &export_config) {
                    log::error!("Failed to export spectrum PNG: {err}");
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
    fn toolbar_divider(ui: &mut egui::Ui) {
        let colors = ThemeColors::from_ui(ui);
        let rect = ui.allocate_space(egui::vec2(1.0, 20.0));
        ui.painter().vline(
            rect.1.center().x,
            rect.1.y_range(),
            Stroke::new(1.0, colors.border),
        );
    }

    /// Create a legend box widget.
    fn legend_box(color: Color32) -> impl egui::Widget {
        move |ui: &mut egui::Ui| {
            let (rect, response) =
                ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
            ui.painter().rect_filled(rect, Rounding::same(2.0), color);
            response
        }
    }

    fn export_spectrum_csv(
        spectrum: &[u64],
        bin_width_ms: f64,
        axis: SpectrumXAxis,
        flight_path_m: f64,
        tof_offset_ns: f64,
    ) -> anyhow::Result<()> {
        let Some(path) = FileDialog::new().set_file_name("spectrum.csv").save_file() else {
            return Ok(());
        };

        let mut file = File::create(path)?;
        let header = match axis {
            SpectrumXAxis::ToFMs => "tof_ms,counts",
            SpectrumXAxis::EnergyEv => "energy_ev,counts",
        };
        writeln!(file, "{header}")?;
        for (i, &count) in spectrum.iter().enumerate() {
            let tof_ms = usize_to_f64(i) * bin_width_ms;
            match axis {
                SpectrumXAxis::ToFMs => {
                    writeln!(file, "{tof_ms:.6},{count}")?;
                }
                SpectrumXAxis::EnergyEv => {
                    let Some(energy) = tof_ms_to_energy_ev(tof_ms, flight_path_m, tof_offset_ns)
                    else {
                        continue;
                    };
                    writeln!(file, "{energy:.6},{count}")?;
                }
            }
        }

        Ok(())
    }

    fn export_spectrum_png(
        spectrum: &[u64],
        bin_width_ms: f64,
        export: &SpectrumExportConfig,
    ) -> anyhow::Result<()> {
        let Some(path) = FileDialog::new().set_file_name("spectrum.png").save_file() else {
            return Ok(());
        };

        let axis = export.axis;
        let flight_path_m = export.flight_path_m;
        let tof_offset_ns = export.tof_offset_ns;
        let log_x = export.log_x;
        let log_y = export.log_y;

        let width: u32 = 800;
        let height: u32 = 240;
        let pad: i32 = 24;
        let width_i32 = i32::try_from(width).unwrap_or(i32::MAX);
        let height_i32 = i32::try_from(height).unwrap_or(i32::MAX);
        let pad_f64 = f64::from(pad);
        let bg = Rgba([0x0d, 0x0d, 0x0d, 0xff]);
        let line = Rgba([0xb8, 0xb8, 0xb8, 0xff]);
        let axis_color = Rgba([0x33, 0x33, 0x33, 0xff]);

        let mut img = RgbaImage::from_pixel(width, height, bg);

        let spec_bins = spectrum.len();
        let mut points = Vec::with_capacity(spec_bins);
        let mut x_min = f64::INFINITY;
        let mut x_max = f64::NEG_INFINITY;
        let mut y_max = 0.0;

        for (i, &count) in spectrum.iter().enumerate() {
            let tof_ms = usize_to_f64(i) * bin_width_ms;
            let mut x = match axis {
                SpectrumXAxis::ToFMs => tof_ms,
                SpectrumXAxis::EnergyEv => {
                    let Some(e) = tof_ms_to_energy_ev(tof_ms, flight_path_m, tof_offset_ns) else {
                        continue;
                    };
                    e
                }
            };
            if log_x {
                if x <= 0.0 {
                    continue;
                }
                x = x.log10();
            }

            let mut y = u64_to_f64(count);
            if log_y {
                y = u64_to_f64(count.max(1)).log10();
            }

            if y > y_max {
                y_max = y;
            }
            if x < x_min {
                x_min = x;
            }
            if x > x_max {
                x_max = x;
            }
            points.push((x, y));
        }

        let x_span = x_max - x_min;
        if !x_min.is_finite() || !x_max.is_finite() || x_span.abs() <= f64::EPSILON {
            x_min = 0.0;
            x_max = 1.0;
        }
        if y_max <= 0.0 {
            y_max = 1.0;
        } else {
            y_max *= 1.05;
        }

        let plot_w = f64::from((width_i32 - pad * 2).max(1));
        let plot_h = f64::from((height_i32 - pad * 2).max(1));
        let x_scale = plot_w / (x_max - x_min).max(1e-9);
        let y_scale = plot_h / y_max.max(1e-9);

        // Axes
        Self::draw_line(
            &mut img,
            pad,
            height_i32 - pad,
            width_i32 - pad,
            height_i32 - pad,
            axis_color,
        );
        Self::draw_line(&mut img, pad, pad, pad, height_i32 - pad, axis_color);

        let mut prev: Option<(i32, i32)> = None;
        for (x, y) in points {
            let px = pad_f64 + (x - x_min) * x_scale;
            let py = f64::from(height_i32) - pad_f64 - y * y_scale;
            let pixel = (round_to_i32_clamped(px), round_to_i32_clamped(py));

            if let Some((prev_x, prev_y)) = prev {
                Self::draw_line(&mut img, prev_x, prev_y, pixel.0, pixel.1, line);
            }
            prev = Some(pixel);
        }

        img.save(path)?;
        Ok(())
    }

    fn draw_line(img: &mut RgbaImage, x0: i32, y0: i32, x1: i32, y1: i32, color: Rgba<u8>) {
        let (mut x0, mut y0, x1, y1) = (x0, y0, x1, y1);
        let width_i32 = i32::try_from(img.width()).unwrap_or(i32::MAX);
        let height_i32 = i32::try_from(img.height()).unwrap_or(i32::MAX);
        let dx = (x1 - x0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let dy = -(y1 - y0).abs();
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut err = dx + dy;

        loop {
            if x0 >= 0 && y0 >= 0 && x0 < width_i32 && y0 < height_i32 {
                if let (Ok(xu), Ok(yu)) = (u32::try_from(x0), u32::try_from(y0)) {
                    img.put_pixel(xu, yu, color);
                }
            }
            if x0 == x1 && y0 == y1 {
                break;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                if x0 == x1 {
                    break;
                }
                err += dy;
                x0 += sx;
            }
            if e2 <= dx {
                if y0 == y1 {
                    break;
                }
                err += dx;
                y0 += sy;
            }
        }
    }
}
