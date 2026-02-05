//! Main view (central panel) rendering.

use std::collections::HashMap;
use std::fs::File;
use std::io::Write;

use eframe::egui::{self, Color32, LayerId, Order, Pos2, Rect, Rounding, Stroke, Vec2, Vec2b};
use egui_plot::{
    Line, MarkerShape, Plot, PlotBounds, PlotImage, PlotPoint, PlotPoints, Points, VLine,
};
use image::{Rgba, RgbaImage};
use rfd::FileDialog;

use super::theme::{accent, ThemeColors};
use crate::app::{RoiSpectrumEntry, RustpixApp};
use crate::state::{SpectrumXAxis, ViewMode, ZoomMode};
use crate::util::{
    energy_ev_to_tof_ms, f64_to_usize_bounded, tof_ms_to_energy_ev, u64_to_f64, usize_to_f64,
};
use crate::viewer::{Roi, RoiSelectionMode};

/// Unique ID for the main histogram plot (used for state persistence).
const HISTOGRAM_PLOT_ID: &str = "histogram_plot";

#[derive(Clone, Copy)]
enum RoiToolbarIcon {
    Rectangle,
    Polygon,
    Clear,
    Gear,
    Close,
    Data,
}

#[derive(Clone, Copy)]
enum ZoomToolbarIcon {
    In,
    Out,
    Box,
}

struct SpectrumExportConfig {
    axis: SpectrumXAxis,
    log_x: bool,
    log_y: bool,
}

#[derive(Clone, Copy)]
struct SpectrumAxisConfig {
    axis: SpectrumXAxis,
    flight_path_m: f64,
    tof_offset_ns: f64,
}

#[derive(Clone, Copy)]
struct SpectrumLineConfig {
    axis: SpectrumXAxis,
    log_x: bool,
    log_y: bool,
    bin_width_ms: f64,
    spec_bins: usize,
    flight_path_m: f64,
    tof_offset_ns: f64,
}

struct CentralPanelInputs {
    counts_for_cursor: Option<Vec<u64>>,
    spectrum: Option<Vec<u64>>,
    current_tof_bin: usize,
    n_bins: usize,
    visibility: CentralPanelVisibility,
    plot_flags: CentralPanelPlotFlags,
    actions: CentralPanelActions,
    data_width: usize,
    data_height: usize,
    data_width_raw: usize,
    data_height_raw: usize,
    data_width_f64: f64,
    data_height_f64: f64,
    data_extent: f64,
}

struct CentralPanelVisibility {
    slicer_enabled: bool,
    show_spectrum: bool,
}

struct CentralPanelActions {
    delete_roi: bool,
    exit_edit_mode: bool,
    commit_polygon: bool,
}

struct CentralPanelPlotFlags {
    needs_plot_reset: bool,
    show_grid: bool,
}

struct CentralPanelLayout {
    image_height: f32,
}

#[derive(Default)]
struct CentralPanelState {
    new_tof_bin: Option<usize>,
    reset_view_clicked: bool,
}

struct SpectrumPanelInputs<'a> {
    spectrum: &'a Option<Vec<u64>>,
    slicer_enabled: bool,
    current_tof_bin: usize,
    n_bins: usize,
    new_tof_bin: &'a mut Option<usize>,
}

#[derive(Default)]
struct SpectrumToolbarActions {
    reset_clicked: bool,
    export_png_clicked: bool,
    export_csv_clicked: bool,
}

struct SpectrumLineStats {
    x_min: f64,
    x_max: f64,
    y_max: f64,
}

struct SpectrumPlotData {
    axis: SpectrumXAxis,
    log_x: bool,
    log_y: bool,
    bin_width_ms: f64,
    spec_bins: usize,
    max_ms: f64,
    x_min: f64,
    x_max: f64,
    y_max: f64,
    x_label: String,
    y_label: String,
    lines: Vec<(String, Color32, Vec<[f64; 2]>)>,
    legend_items: Vec<(String, Color32)>,
    manual_bounds: Option<PlotBounds>,
    export_bounds: PlotBounds,
    flight_path_m: f64,
    tof_offset_ns: f64,
}

struct HistogramGeometry {
    data_width_f64: f64,
    data_height_f64: f64,
    data_extent: f64,
    half_x: f64,
    half_y: f64,
    data_width_f32: f32,
    data_height_f32: f32,
}

struct HistogramInteraction {
    shift_down: bool,
    zoom_mode: ZoomMode,
    handle_radius: f64,
    disable_plot_drag: bool,
}

impl HistogramInteraction {
    fn zoom_active(&self) -> bool {
        self.zoom_mode != ZoomMode::None
    }
}

struct SpectrumExportGeometry {
    width: u32,
    height: u32,
    pad: i32,
    pad_f64: f64,
    plot_left: i32,
    plot_right: i32,
    plot_top: i32,
    plot_bottom: i32,
    plot_w: f64,
    plot_h: f64,
    x_min: f64,
    x_max: f64,
    y_min: f64,
    y_max: f64,
    x_scale: f64,
    y_scale: f64,
    axis_color: Rgba<u8>,
    grid_color: Rgba<u8>,
    label_color: Rgba<u8>,
}

struct HistogramDragContext<'a> {
    ctx: &'a egui::Context,
    plot_ui: &'a mut egui_plot::PlotUi,
    response: &'a egui::Response,
    pointer_pos: Option<PlotPoint>,
    handle_radius: f64,
    min_roi_size: f64,
    data_extent: f64,
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

fn usize_to_i32_saturating(value: usize) -> i32 {
    i32::try_from(value).unwrap_or(i32::MAX)
}

fn text_width_px(text: &str) -> i32 {
    usize_to_i32_saturating(text.len()).saturating_mul(6)
}

#[allow(clippy::cast_possible_truncation)]
fn zoom_factor_to_f32(factor: f64) -> f32 {
    if factor.is_finite() {
        factor as f32
    } else if factor.is_sign_negative() {
        f32::MIN
    } else {
        f32::MAX
    }
}

fn parse_spectrum_range(min_text: &str, max_text: &str) -> Result<Option<(f64, f64)>, ()> {
    let min_text = min_text.trim();
    let max_text = max_text.trim();
    if min_text.is_empty() && max_text.is_empty() {
        return Ok(None);
    }
    let Ok(min_val) = min_text.parse::<f64>() else {
        return Err(());
    };
    let Ok(max_val) = max_text.parse::<f64>() else {
        return Err(());
    };
    if min_val >= max_val {
        return Err(());
    }
    Ok(Some((min_val, max_val)))
}

#[allow(clippy::unused_self)]
impl RustpixApp {
    /// Render the central panel with histogram, slicer, and spectrum.
    pub(crate) fn render_central_panel(&mut self, ctx: &egui::Context) {
        let colors = ThemeColors::from_ctx(ctx);
        self.ensure_texture(ctx);
        let inputs = self.build_central_panel_inputs(ctx);
        self.apply_central_panel_shortcuts(ctx, &inputs);
        let mut state = CentralPanelState::default();
        self.render_central_panel_body(ctx, &colors, &inputs, &mut state);
        self.finish_central_panel(&inputs, &state);
    }

    fn build_central_panel_inputs(&self, ctx: &egui::Context) -> CentralPanelInputs {
        let counts_for_cursor = self.current_counts().map(<[u64]>::to_vec);
        let spectrum = self.tof_spectrum().map(<[u64]>::to_vec);
        let slicer_enabled = self.ui_state.histogram.slicer_enabled;
        let current_tof_bin = self.ui_state.current_tof_bin;
        let show_spectrum = self.ui_state.histogram.show;
        let n_bins = self.n_tof_bins();
        let needs_plot_reset = self.ui_state.histogram_view.needs_plot_reset;
        let show_grid = self.ui_state.histogram_view.show_grid;
        let wants_keyboard = ctx.wants_keyboard_input();
        let delete_roi = ctx.input(|i| {
            !wants_keyboard
                && (i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace))
        });
        let exit_edit_mode = ctx.input(|i| i.key_pressed(egui::Key::Escape));
        let commit_polygon = !wants_keyboard
            && self.roi_state.polygon_draft.is_some()
            && ctx.input(|i| i.key_pressed(egui::Key::Enter));

        let (data_width_raw, data_height_raw) = self.current_data_dimensions();
        let (data_width, data_height) = self.current_dimensions();
        let data_width = data_width.max(1);
        let data_height = data_height.max(1);
        let data_width_f64 = usize_to_f64(data_width);
        let data_height_f64 = usize_to_f64(data_height);
        let data_extent = data_width_f64.max(data_height_f64);

        CentralPanelInputs {
            counts_for_cursor,
            spectrum,
            current_tof_bin,
            n_bins,
            visibility: CentralPanelVisibility {
                slicer_enabled,
                show_spectrum,
            },
            plot_flags: CentralPanelPlotFlags {
                needs_plot_reset,
                show_grid,
            },
            actions: CentralPanelActions {
                delete_roi,
                exit_edit_mode,
                commit_polygon,
            },
            data_width,
            data_height,
            data_width_raw,
            data_height_raw,
            data_width_f64,
            data_height_f64,
            data_extent,
        }
    }

    fn apply_central_panel_shortcuts(&mut self, ctx: &egui::Context, inputs: &CentralPanelInputs) {
        if inputs.actions.delete_roi {
            self.roi_state.delete_selected();
        }
        if inputs.actions.exit_edit_mode {
            self.roi_state.clear_edit_mode();
            self.roi_state.cancel_draft();
        }
        if inputs.actions.commit_polygon {
            self.commit_polygon_draft(ctx);
        }
        self.apply_histogram_transform_shortcuts(ctx);
    }

    fn apply_histogram_transform_shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.wants_keyboard_input() {
            return;
        }
        let shift_down = ctx.input(|i| i.modifiers.shift);
        if ctx.input(|i| i.key_pressed(egui::Key::R)) {
            if shift_down {
                self.rotate_histogram_ccw();
            } else {
                self.rotate_histogram_cw();
            }
        }
        if ctx.input(|i| i.key_pressed(egui::Key::H)) {
            self.flip_histogram_horizontal();
        }
        if ctx.input(|i| i.key_pressed(egui::Key::V)) {
            self.flip_histogram_vertical();
        }
    }

    fn render_central_panel_body(
        &mut self,
        ctx: &egui::Context,
        colors: &ThemeColors,
        inputs: &CentralPanelInputs,
        state: &mut CentralPanelState,
    ) {
        egui::CentralPanel::default()
            .frame(
                egui::Frame::none()
                    .fill(colors.bg_dark)
                    .inner_margin(egui::Margin::same(16.0)),
            )
            .show(ctx, |ui| {
                let layout = self.central_panel_layout(ui, inputs);
                self.render_histogram_section(ctx, ui, colors, inputs, state, layout.image_height);
                self.render_roi_help_panel(ctx);
                self.render_slicer_section(ui, inputs, state);
                self.render_spectrum_section(ctx, ui, inputs, state);
            });
    }

    fn finish_central_panel(&mut self, inputs: &CentralPanelInputs, state: &CentralPanelState) {
        if let Some(bin) = state.new_tof_bin {
            self.ui_state.current_tof_bin = bin;
            self.texture = None;
        }

        if inputs.plot_flags.needs_plot_reset || state.reset_view_clicked {
            self.ui_state.histogram_view.needs_plot_reset = false;
        }
    }

    fn central_panel_layout(
        &self,
        ui: &egui::Ui,
        inputs: &CentralPanelInputs,
    ) -> CentralPanelLayout {
        let available_height = ui.available_height();
        let slicer_height = if inputs.visibility.slicer_enabled && inputs.n_bins > 0 {
            48.0
        } else {
            0.0
        };
        let spectrum_height = if inputs.visibility.show_spectrum {
            if self.spectrum_has_legend() {
                260.0
            } else {
                220.0
            }
        } else {
            0.0
        };
        let image_height = available_height - slicer_height - spectrum_height - 8.0;
        CentralPanelLayout { image_height }
    }

    fn spectrum_has_legend(&self) -> bool {
        self.ui_state.spectrum.full_fov_visible
            || self
                .roi_state
                .rois
                .iter()
                .any(|roi| roi.visibility.spectrum_visible)
    }

    fn render_histogram_section(
        &mut self,
        ctx: &egui::Context,
        ui: &mut egui::Ui,
        colors: &ThemeColors,
        inputs: &CentralPanelInputs,
        state: &mut CentralPanelState,
        image_height: f32,
    ) {
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), image_height.max(200.0)),
            egui::Layout::left_to_right(egui::Align::TOP),
            |ui| {
                let plot_width = ui.available_width() - 60.0;
                ui.allocate_ui_with_layout(
                    egui::vec2(plot_width, ui.available_height()),
                    egui::Layout::top_down(egui::Align::LEFT),
                    |ui| {
                        self.render_histogram_plot_area(ctx, ui, colors, inputs, state);
                    },
                );

                ui.add_space(8.0);
                self.render_colorbar(ui);
            },
        );
    }

    fn render_slicer_section(
        &mut self,
        ui: &mut egui::Ui,
        inputs: &CentralPanelInputs,
        state: &mut CentralPanelState,
    ) {
        if inputs.visibility.slicer_enabled && inputs.n_bins > 0 {
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

            let clamped_bin = inputs.current_tof_bin.min(inputs.n_bins - 1);
            let mut bin = clamped_bin;

            let total_width = inner_rect.width();
            let label_width = 70.0;
            let value_width = 70.0;
            let spacing = slicer_ui.spacing().item_spacing.x;
            let slider_width = (total_width - label_width - value_width - spacing * 2.0).max(120.0);

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
                egui::Slider::new(&mut bin, 0..=(inputs.n_bins - 1))
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
                        egui::RichText::new(format!("{} / {}", bin + 1, inputs.n_bins))
                            .size(11.0)
                            .color(colors.text_primary),
                    );
                },
            );

            if slider.changed() && bin != inputs.current_tof_bin {
                state.new_tof_bin = Some(bin);
            }
        }
    }

    fn render_spectrum_section(
        &mut self,
        ctx: &egui::Context,
        ui: &mut egui::Ui,
        inputs: &CentralPanelInputs,
        state: &mut CentralPanelState,
    ) {
        if inputs.visibility.show_spectrum {
            ui.add_space(8.0);
            let mut new_tof_bin = state.new_tof_bin;
            let mut spectrum_inputs = SpectrumPanelInputs {
                spectrum: &inputs.spectrum,
                slicer_enabled: inputs.visibility.slicer_enabled,
                current_tof_bin: inputs.current_tof_bin,
                n_bins: inputs.n_bins,
                new_tof_bin: &mut new_tof_bin,
            };
            self.render_spectrum_panel(ctx, ui, &mut spectrum_inputs);
            state.new_tof_bin = new_tof_bin;
        }
    }

    fn render_histogram_plot_area(
        &mut self,
        ctx: &egui::Context,
        ui: &mut egui::Ui,
        colors: &ThemeColors,
        inputs: &CentralPanelInputs,
        state: &mut CentralPanelState,
    ) {
        let texture_id = self.texture.as_ref().map(egui::TextureHandle::id);
        if let Some(tex_id) = texture_id {
            self.render_histogram_toolbar(ui, colors, state);
            ui.add_space(4.0);
            self.render_histogram_plot(ctx, ui, inputs, state, tex_id);
        } else {
            Self::render_histogram_empty(ui, colors);
        }
    }

    fn render_histogram_toolbar(
        &mut self,
        ui: &mut egui::Ui,
        colors: &ThemeColors,
        state: &mut CentralPanelState,
    ) {
        ui.horizontal(|ui| {
            self.render_roi_toolbar(ui);

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let reset_btn = egui::Button::new(
                    egui::RichText::new("↺ Reset View")
                        .size(11.0)
                        .color(colors.text_muted),
                )
                .min_size(egui::vec2(0.0, 28.0))
                .fill(Color32::TRANSPARENT)
                .stroke(Stroke::new(1.0, colors.border_light))
                .rounding(Rounding::same(4.0));

                if ui
                    .add(reset_btn)
                    .on_hover_text("Reset view to fit data (or double-click)")
                    .clicked()
                {
                    state.reset_view_clicked = true;
                }

                ui.add_space(6.0);
                Self::toolbar_divider(ui);
                ui.add_space(8.0);
                self.render_histogram_zoom_group(ui);

                ui.add_space(8.0);
                Self::toolbar_divider(ui);
                ui.add_space(8.0);
                self.render_histogram_transform_controls(ui, colors);

                ui.add_space(8.0);
                Self::toolbar_divider(ui);
                ui.add_space(8.0);
                let grid_btn = egui::Button::new(egui::RichText::new("▦ Grid").size(11.0).color(
                    if self.ui_state.histogram_view.show_grid {
                        Color32::WHITE
                    } else {
                        colors.text_muted
                    },
                ))
                .min_size(egui::vec2(0.0, 28.0))
                .fill(if self.ui_state.histogram_view.show_grid {
                    accent::BLUE
                } else {
                    Color32::TRANSPARENT
                })
                .stroke(Stroke::new(1.0, colors.border_light))
                .rounding(Rounding::same(4.0));

                if ui.add(grid_btn).on_hover_text("Toggle grid").clicked() {
                    self.ui_state.histogram_view.show_grid =
                        !self.ui_state.histogram_view.show_grid;
                }
            });
        });
    }

    fn render_histogram_transform_controls(&mut self, ui: &mut egui::Ui, colors: &ThemeColors) {
        let transform = self.ui_state.histogram_view.transform;
        let label = transform
            .status_label()
            .unwrap_or_else(|| "No transform".to_string());
        let tooltip_suffix = format!("Current: {label}");

        let rotate_left = Self::transform_button(ui, "↶", false, colors)
            .on_hover_text(format!("Rotate left 90° (Shift+R)\n{tooltip_suffix}"))
            .clicked();
        if rotate_left {
            self.rotate_histogram_ccw();
        }

        let rotate_right = Self::transform_button(ui, "↷", false, colors)
            .on_hover_text(format!("Rotate right 90° (R)\n{tooltip_suffix}"))
            .clicked();
        if rotate_right {
            self.rotate_histogram_cw();
        }

        let flip_v = Self::transform_button(ui, "⇅", transform.flip_v, colors)
            .on_hover_text(format!("Flip vertical (V)\n{tooltip_suffix}"))
            .clicked();
        if flip_v {
            self.flip_histogram_vertical();
        }

        let flip_h = Self::transform_button(ui, "⇆", transform.flip_h, colors)
            .on_hover_text(format!("Flip horizontal (H)\n{tooltip_suffix}"))
            .clicked();
        if flip_h {
            self.flip_histogram_horizontal();
        }

        if !transform.is_identity() {
            let reset = Self::transform_button(ui, "Reset", false, colors)
                .on_hover_text(format!("Reset orientation\n{tooltip_suffix}"))
                .clicked();
            if reset {
                self.reset_histogram_transform();
            }
        }
    }

    fn transform_button(
        ui: &mut egui::Ui,
        label: &str,
        active: bool,
        colors: &ThemeColors,
    ) -> egui::Response {
        let text_color = if active {
            Color32::WHITE
        } else {
            colors.text_muted
        };
        let btn = egui::Button::new(egui::RichText::new(label).size(16.0).color(text_color))
            .min_size(egui::vec2(32.0, 28.0))
            .fill(if active {
                accent::BLUE
            } else {
                Color32::TRANSPARENT
            })
            .stroke(Stroke::new(1.0, colors.border_light))
            .rounding(Rounding::same(4.0));
        ui.add(btn)
    }

    fn render_histogram_plot(
        &mut self,
        ctx: &egui::Context,
        ui: &mut egui::Ui,
        inputs: &CentralPanelInputs,
        state: &mut CentralPanelState,
        tex_id: egui::TextureId,
    ) {
        let geometry = self.histogram_geometry(inputs);
        let should_reset = inputs.plot_flags.needs_plot_reset || state.reset_view_clicked;
        let plot_rect = ui.available_rect_before_wrap();
        let interaction = self.compute_histogram_interaction(ctx);

        let mut plot = Plot::new(HISTOGRAM_PLOT_ID)
            .data_aspect(1.0)
            .auto_bounds(Vec2b::new(false, false))
            .include_x(0.0)
            .include_x(inputs.data_width_f64)
            .include_y(0.0)
            .include_y(inputs.data_height_f64)
            .show_grid(Vec2b::new(
                inputs.plot_flags.show_grid,
                inputs.plot_flags.show_grid,
            ))
            .x_axis_label("X (pixels)")
            .y_axis_label("Y (pixels)")
            .allow_drag(!interaction.disable_plot_drag);

        if should_reset {
            plot = plot.reset();
        }

        let min_roi_size = 2.0;
        let roi_mode = self.roi_state.mode;
        plot.show(ui, |plot_ui| {
            self.maybe_reset_histogram_bounds(plot_ui, should_reset, plot_rect, &geometry);
            self.draw_histogram_texture(plot_ui, tex_id, &geometry);
            self.draw_hot_pixel_overlay(plot_ui);

            let response = plot_ui.response().clone();
            let pointer_pos = self.histogram_pointer_pos(plot_ui, &geometry);
            let rect_drawing = !interaction.zoom_active()
                && roi_mode == RoiSelectionMode::Rectangle
                && (interaction.shift_down || self.roi_state.draft.is_some());
            let poly_drawing = !interaction.zoom_active()
                && roi_mode == RoiSelectionMode::Polygon
                && (interaction.shift_down || self.roi_state.polygon_draft.is_some());

            self.update_histogram_cursor_icon(
                plot_ui,
                pointer_pos,
                &interaction,
                rect_drawing,
                poly_drawing,
            );

            if interaction.zoom_active() {
                self.handle_histogram_zoom(plot_ui, &interaction, &response, pointer_pos);
            } else if rect_drawing || poly_drawing {
                self.handle_histogram_roi_drawing(
                    &response,
                    pointer_pos,
                    rect_drawing,
                    poly_drawing,
                    min_roi_size,
                );
            } else {
                let mut drag = HistogramDragContext {
                    ctx,
                    plot_ui,
                    response: &response,
                    pointer_pos,
                    handle_radius: interaction.handle_radius,
                    min_roi_size,
                    data_extent: geometry.data_extent,
                };
                self.handle_histogram_roi_drag(&mut drag);
            }

            self.update_histogram_cursor_info(plot_ui, inputs);
            self.handle_histogram_roi_clicks(
                ctx,
                &response,
                pointer_pos,
                interaction.handle_radius,
                interaction.shift_down,
            );
            self.handle_histogram_roi_context_menu(
                &response,
                pointer_pos,
                interaction.handle_radius,
            );
            self.finalize_histogram_plot(plot_ui);
        });
    }

    fn render_histogram_empty(ui: &mut egui::Ui, colors: &ThemeColors) {
        let no_data_bg = if colors.bg_dark == super::theme::dark::BG_DARK {
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

    fn histogram_geometry(&self, inputs: &CentralPanelInputs) -> HistogramGeometry {
        let half_x = inputs.data_width_f64 / 2.0;
        let half_y = inputs.data_height_f64 / 2.0;
        #[allow(clippy::cast_possible_truncation)]
        let data_width_f32 = inputs.data_width_f64 as f32;
        #[allow(clippy::cast_possible_truncation)]
        let data_height_f32 = inputs.data_height_f64 as f32;
        HistogramGeometry {
            data_width_f64: inputs.data_width_f64,
            data_height_f64: inputs.data_height_f64,
            data_extent: inputs.data_extent,
            half_x,
            half_y,
            data_width_f32,
            data_height_f32,
        }
    }

    fn compute_histogram_interaction(&self, ctx: &egui::Context) -> HistogramInteraction {
        let shift_down = ctx.input(|i| i.modifiers.shift);
        let zoom_mode = self.ui_state.hist_zoom_mode;
        let zoom_active = zoom_mode != ZoomMode::None;
        let handle_radius = 3.0;
        let pre_drag_hit = if !shift_down
            && !zoom_active
            && ctx.input(|i| i.pointer.button_down(egui::PointerButton::Primary))
        {
            self.histogram_pre_drag_hit(ctx, handle_radius)
        } else {
            false
        };
        let roi_drag_active = self.roi_state.is_dragging() || self.roi_state.is_edit_dragging();
        let roi_drawing_active =
            self.roi_state.draft.is_some() || self.roi_state.polygon_draft.is_some();
        let disable_plot_drag =
            shift_down || roi_drag_active || roi_drawing_active || pre_drag_hit || zoom_active;
        HistogramInteraction {
            shift_down,
            zoom_mode,
            handle_radius,
            disable_plot_drag,
        }
    }

    fn histogram_pre_drag_hit(&self, ctx: &egui::Context, handle_radius: f64) -> bool {
        if let (Some(bounds), Some(rect), Some(pos)) = (
            self.ui_state.roi_last_plot_bounds,
            self.ui_state.roi_last_plot_rect,
            ctx.input(|i| i.pointer.interact_pos()),
        ) {
            if rect.contains(pos) && rect.width() > 0.0 && rect.height() > 0.0 {
                let x_frac = f64::from(pos.x - rect.left()) / f64::from(rect.width());
                let y_frac = f64::from(pos.y - rect.top()) / f64::from(rect.height());
                if (0.0..=1.0).contains(&x_frac) && (0.0..=1.0).contains(&y_frac) {
                    let plot_x = bounds.min()[0] + x_frac * (bounds.max()[0] - bounds.min()[0]);
                    let plot_y = bounds.max()[1] - y_frac * (bounds.max()[1] - bounds.min()[1]);
                    let point = PlotPoint::new(plot_x, plot_y);
                    return self
                        .roi_state
                        .hit_test_handle(point, handle_radius)
                        .is_some()
                        || self
                            .roi_state
                            .hit_test_vertex(point, handle_radius)
                            .is_some()
                        || self.roi_state.hit_test_edge(point, handle_radius).is_some()
                        || self.roi_state.hit_test(point).is_some();
                }
            }
        }
        false
    }

    fn maybe_reset_histogram_bounds(
        &self,
        plot_ui: &mut egui_plot::PlotUi,
        should_reset: bool,
        plot_rect: Rect,
        geometry: &HistogramGeometry,
    ) {
        if should_reset || plot_ui.response().double_clicked() {
            let pad_x = (geometry.data_width_f64 * 0.05).max(16.0);
            let pad_y = (geometry.data_height_f64 * 0.05).max(16.0);

            let plot_w = f64::from(plot_rect.width().max(1.0));
            let plot_h = f64::from(plot_rect.height().max(1.0));
            let available_aspect = plot_w / plot_h;
            let data_span_x = geometry.data_width_f64 + pad_x * 2.0;
            let data_span_y = geometry.data_height_f64 + pad_y * 2.0;
            let data_aspect = data_span_x / data_span_y;
            let mut x_half = data_span_x / 2.0;
            let mut y_half = data_span_y / 2.0;

            if available_aspect >= data_aspect {
                x_half = y_half * available_aspect;
            } else {
                y_half = x_half / available_aspect;
            }

            let center_x = geometry.data_width_f64 / 2.0;
            let center_y = geometry.data_height_f64 / 2.0;
            plot_ui.set_plot_bounds(PlotBounds::from_min_max(
                [center_x - x_half, center_y - y_half],
                [center_x + x_half, center_y + y_half],
            ));
        }
    }

    fn draw_histogram_texture(
        &self,
        plot_ui: &mut egui_plot::PlotUi,
        tex_id: egui::TextureId,
        geometry: &HistogramGeometry,
    ) {
        plot_ui.image(PlotImage::new(
            tex_id,
            PlotPoint::new(geometry.half_x, geometry.half_y),
            [geometry.data_width_f32, geometry.data_height_f32],
        ));
    }

    fn draw_hot_pixel_overlay(&self, plot_ui: &mut egui_plot::PlotUi) {
        if self.ui_state.view_mode == ViewMode::Hits && self.ui_state.pixel_health.show_hot_pixels {
            if let Some(mask) = &self.pixel_masks {
                if !mask.hot_points.is_empty() {
                    let transform = self.ui_state.histogram_view.transform;
                    let hot_points = if transform.is_identity() {
                        mask.hot_points.clone()
                    } else {
                        let (width, height) = self.current_data_dimensions();
                        let width_f = usize_to_f64(width);
                        let height_f = usize_to_f64(height);
                        mask.hot_points
                            .iter()
                            .map(|[x, y]| {
                                transform
                                    .apply_f64(*x, *y, width_f, height_f)
                                    .unwrap_or((*x, *y))
                            })
                            .map(|(x, y)| [x, y])
                            .collect()
                    };
                    let hot_points = Points::new(PlotPoints::new(hot_points))
                        .shape(MarkerShape::Square)
                        .radius(2.0)
                        .color(accent::RED)
                        .allow_hover(false);
                    plot_ui.points(hot_points);
                }
            }
        }
    }

    fn histogram_pointer_pos(
        &self,
        plot_ui: &egui_plot::PlotUi,
        geometry: &HistogramGeometry,
    ) -> Option<PlotPoint> {
        plot_ui.pointer_coordinate().map(|point| {
            PlotPoint::new(
                point.x.clamp(0.0, geometry.data_width_f64),
                point.y.clamp(0.0, geometry.data_height_f64),
            )
        })
    }

    fn update_histogram_cursor_icon(
        &self,
        plot_ui: &mut egui_plot::PlotUi,
        pointer_pos: Option<PlotPoint>,
        interaction: &HistogramInteraction,
        rect_drawing: bool,
        poly_drawing: bool,
    ) {
        let response = plot_ui.response();
        if !response.hovered() {
            return;
        }
        if interaction.zoom_active() {
            let icon = match interaction.zoom_mode {
                ZoomMode::In => egui::CursorIcon::ZoomIn,
                ZoomMode::Out => egui::CursorIcon::ZoomOut,
                ZoomMode::Box => egui::CursorIcon::Crosshair,
                ZoomMode::None => egui::CursorIcon::Default,
            };
            plot_ui.ctx().set_cursor_icon(icon);
            return;
        }
        if rect_drawing || poly_drawing {
            plot_ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
            return;
        }
        if let Some(pos) = pointer_pos {
            if let Some((_, handle)) = self
                .roi_state
                .hit_test_handle(pos, interaction.handle_radius)
            {
                let icon =
                    match handle {
                        crate::viewer::RoiHandle::North | crate::viewer::RoiHandle::South => {
                            egui::CursorIcon::ResizeVertical
                        }
                        crate::viewer::RoiHandle::East | crate::viewer::RoiHandle::West => {
                            egui::CursorIcon::ResizeHorizontal
                        }
                        crate::viewer::RoiHandle::NorthEast
                        | crate::viewer::RoiHandle::SouthWest => egui::CursorIcon::ResizeNeSw,
                        crate::viewer::RoiHandle::NorthWest
                        | crate::viewer::RoiHandle::SouthEast => egui::CursorIcon::ResizeNwSe,
                    };
                plot_ui.ctx().set_cursor_icon(icon);
            } else if self
                .roi_state
                .hit_test_vertex(pos, interaction.handle_radius)
                .is_some()
            {
                plot_ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
            } else if self
                .roi_state
                .hit_test_edge(pos, interaction.handle_radius)
                .is_some()
            {
                plot_ui
                    .ctx()
                    .set_cursor_icon(egui::CursorIcon::PointingHand);
            } else if self.roi_state.hit_test(pos).is_some() {
                plot_ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
            }
        }
    }

    fn handle_histogram_zoom(
        &mut self,
        plot_ui: &mut egui_plot::PlotUi,
        interaction: &HistogramInteraction,
        response: &egui::Response,
        pointer_pos: Option<PlotPoint>,
    ) {
        match interaction.zoom_mode {
            ZoomMode::In | ZoomMode::Out => {
                if response.clicked() {
                    let center = pointer_pos.unwrap_or_else(|| {
                        let bounds = plot_ui.plot_bounds();
                        let min = bounds.min();
                        let max = bounds.max();
                        PlotPoint::new((min[0] + max[0]) * 0.5, (min[1] + max[1]) * 0.5)
                    });
                    let factor = if interaction.zoom_mode == ZoomMode::In {
                        1.25
                    } else {
                        0.8
                    };
                    plot_ui.zoom_bounds(Vec2::splat(zoom_factor_to_f32(factor)), center);
                }
            }
            ZoomMode::Box => {
                if response.drag_started() {
                    self.ui_state.hist_zoom_start = pointer_pos;
                }
                if response.dragged() {
                    if let (Some(start), Some(current)) =
                        (self.ui_state.hist_zoom_start, pointer_pos)
                    {
                        let start_screen = plot_ui.screen_from_plot(start);
                        let current_screen = plot_ui.screen_from_plot(current);
                        let rect = Rect::from_two_pos(start_screen, current_screen);
                        Self::draw_zoom_rect(plot_ui, response, rect);
                    }
                }
                if response.drag_stopped() {
                    if let (Some(start), Some(end)) = (self.ui_state.hist_zoom_start, pointer_pos) {
                        let min_x = start.x.min(end.x);
                        let max_x = start.x.max(end.x);
                        let min_y = start.y.min(end.y);
                        let max_y = start.y.max(end.y);
                        if (max_x - min_x) > 1.0 && (max_y - min_y) > 1.0 {
                            plot_ui.set_plot_bounds(PlotBounds::from_min_max(
                                [min_x, min_y],
                                [max_x, max_y],
                            ));
                        }
                    }
                    self.ui_state.hist_zoom_start = None;
                }
            }
            ZoomMode::None => {}
        }
    }

    fn handle_histogram_roi_drawing(
        &mut self,
        response: &egui::Response,
        pointer_pos: Option<PlotPoint>,
        rect_drawing: bool,
        poly_drawing: bool,
        min_roi_size: f64,
    ) {
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
        }
    }

    fn handle_histogram_roi_drag(&mut self, drag: &mut HistogramDragContext<'_>) {
        if drag.response.drag_started() {
            if let Some(pos) = drag.pointer_pos {
                if let Some((roi_id, index)) =
                    self.roi_state.hit_test_vertex(pos, drag.handle_radius)
                {
                    let bounds = drag.plot_ui.plot_bounds();
                    self.roi_state.start_vertex_drag(roi_id, index, pos, bounds);
                } else if let Some((hit_id, handle)) =
                    self.roi_state.hit_test_handle(pos, drag.handle_radius)
                {
                    let bounds = drag.plot_ui.plot_bounds();
                    self.roi_state.start_edit_drag(hit_id, handle, pos, bounds);
                } else if let Some(hit_id) = self.roi_state.hit_test(pos) {
                    let bounds = drag.plot_ui.plot_bounds();
                    self.roi_state.start_drag(hit_id, pos, bounds);
                }
            }
        }
        if drag.response.dragged() {
            if let Some(pos) = drag.pointer_pos {
                if self.roi_state.is_edit_dragging() {
                    self.roi_state.update_vertex_drag(pos);
                    self.roi_state
                        .update_edit_drag(pos, drag.min_roi_size, 0.0, drag.data_extent);
                } else {
                    self.roi_state.update_drag(pos, 0.0, drag.data_extent);
                }
            }
        }
        if drag.response.drag_stopped() {
            if let Err(err) = self.roi_state.end_vertex_drag() {
                self.notify_roi_error(drag.ctx, err);
            }
            self.roi_state.end_edit_drag();
            self.roi_state.end_drag();
        }
    }

    fn update_histogram_cursor_info(
        &mut self,
        plot_ui: &egui_plot::PlotUi,
        inputs: &CentralPanelInputs,
    ) {
        if let Some(curr) = plot_ui.pointer_coordinate() {
            let x = curr.x;
            let y = curr.y;
            if x >= 0.0 && y >= 0.0 && x < inputs.data_width_f64 && y < inputs.data_height_f64 {
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let (Some(xi), Some(yi)) = (
                    f64_to_usize_bounded(x, inputs.data_width),
                    f64_to_usize_bounded(y, inputs.data_height),
                ) else {
                    self.cursor_info = None;
                    return;
                };
                let transform = self.ui_state.histogram_view.transform;
                let Some((src_x, src_y)) =
                    transform.apply_inverse(xi, yi, inputs.data_width_raw, inputs.data_height_raw)
                else {
                    self.cursor_info = None;
                    return;
                };
                let count = inputs
                    .counts_for_cursor
                    .as_ref()
                    .map_or(0, |c| c[src_y * inputs.data_width_raw + src_x]);
                self.cursor_info = Some((xi, yi, count));
            } else {
                self.cursor_info = None;
            }
        } else {
            self.cursor_info = None;
        }
    }

    fn handle_histogram_roi_clicks(
        &mut self,
        ctx: &egui::Context,
        response: &egui::Response,
        pointer_pos: Option<PlotPoint>,
        handle_radius: f64,
        shift_down: bool,
    ) {
        if response.clicked()
            && self.roi_state.draft.is_none()
            && self.roi_state.polygon_draft.is_none()
            && !shift_down
            && !self.roi_state.is_dragging()
            && !self.roi_state.is_edit_dragging()
        {
            if let Some(pos) = pointer_pos {
                match self.roi_state.insert_vertex_at(pos, handle_radius) {
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
                        self.notify_roi_error(ctx, err);
                    }
                }
            }
        }

        if response.double_clicked() && self.roi_state.draft.is_none() {
            if let Some(pos) = pointer_pos {
                if let Some(hit_id) = self.roi_state.hit_test(pos) {
                    self.roi_state.set_edit_mode(hit_id, true);
                } else {
                    self.roi_state.clear_edit_mode();
                }
            }
        }
    }

    fn handle_histogram_roi_context_menu(
        &mut self,
        response: &egui::Response,
        pointer_pos: Option<PlotPoint>,
        handle_radius: f64,
    ) {
        let mut suppress_context_menu = false;
        if response.secondary_clicked() {
            let mut target = None;
            if let Some(pos) = pointer_pos {
                if self.roi_state.delete_vertex_at(pos, handle_radius) {
                    self.roi_state.set_context_menu(None);
                    suppress_context_menu = true;
                } else if let Some(hit_id) = self.roi_state.hit_test(pos) {
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
            if let Some(target) = self.roi_state.context_menu_target() {
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
    }

    fn finalize_histogram_plot(&mut self, plot_ui: &mut egui_plot::PlotUi) {
        self.roi_state.draw(plot_ui);
        self.roi_state.draw_draft(plot_ui);
        self.ui_state.roi_last_plot_bounds = Some(plot_ui.plot_bounds());
        self.ui_state.roi_last_plot_rect = Some(plot_ui.response().rect);
    }

    fn notify_roi_error(&mut self, ctx: &egui::Context, err: crate::viewer::RoiCommitError) {
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
    }

    fn draw_zoom_rect(plot_ui: &egui_plot::PlotUi, response: &egui::Response, rect: Rect) {
        let painter = plot_ui
            .ctx()
            .layer_painter(LayerId::new(Order::Foreground, response.id))
            .with_clip_rect(response.rect);
        painter.rect_filled(
            rect,
            Rounding::same(2.0),
            Color32::from_rgba_unmultiplied(58, 130, 246, 32),
        );
        painter.rect_stroke(
            rect,
            Rounding::same(2.0),
            Stroke::new(1.0, Color32::from_rgb(58, 130, 246)),
        );
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
        egui::Frame::none()
            .stroke(Stroke::new(1.0, colors.border_light))
            .rounding(Rounding::same(4.0))
            .inner_margin(egui::Margin::symmetric(4.0, 2.0))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let selection_mode = self.render_roi_mode_menu(ui, &colors);
                    if selection_mode != self.roi_state.mode {
                        self.roi_state.mode = selection_mode;
                        self.roi_state.cancel_draft();
                    }

                    self.render_roi_close_button(ui);

                    if Self::roi_icon_button(ui, RoiToolbarIcon::Clear, "Clear all ROIs").clicked()
                    {
                        self.roi_state.clear();
                    }

                    self.render_roi_settings_menu(ui, &colors);
                    self.render_roi_help_button(ui, &colors);
                });
            });

        ui.add_space(8.0);
    }

    fn render_roi_mode_menu(
        &mut self,
        ui: &mut egui::Ui,
        colors: &ThemeColors,
    ) -> RoiSelectionMode {
        let mut selection_mode = self.roi_state.mode;
        let menu_button = egui::Button::new("")
            .min_size(egui::vec2(34.0, 22.0))
            .fill(Color32::TRANSPARENT)
            .stroke(Stroke::new(1.0, colors.border_light))
            .rounding(Rounding::same(4.0));
        let menu_response = egui::menu::menu_custom_button(ui, menu_button, |ui| {
            ui.horizontal(|ui| {
                Self::paint_roi_icon_in_ui(ui, RoiToolbarIcon::Rectangle, colors.text_muted);
                if ui
                    .selectable_label(
                        self.roi_state.mode == RoiSelectionMode::Rectangle,
                        "Rectangle",
                    )
                    .clicked()
                {
                    selection_mode = RoiSelectionMode::Rectangle;
                }
            });
            ui.horizontal(|ui| {
                Self::paint_roi_icon_in_ui(ui, RoiToolbarIcon::Polygon, colors.text_muted);
                if ui
                    .selectable_label(self.roi_state.mode == RoiSelectionMode::Polygon, "Polygon")
                    .clicked()
                {
                    selection_mode = RoiSelectionMode::Polygon;
                }
            });
        });
        let icon_rect = menu_response.response.rect.shrink2(egui::vec2(4.0, 4.0));
        let icon_rect = Rect::from_min_max(
            icon_rect.min,
            Pos2::new(icon_rect.center().x + 2.0, icon_rect.max.y),
        );
        let image = Self::roi_icon_image(
            match self.roi_state.mode {
                RoiSelectionMode::Rectangle => RoiToolbarIcon::Rectangle,
                RoiSelectionMode::Polygon => RoiToolbarIcon::Polygon,
            },
            colors.text_muted,
        );
        image.paint_at(ui, icon_rect);
        Self::paint_dropdown_caret(ui.painter(), menu_response.response.rect, colors.text_muted);
        selection_mode
    }

    fn render_roi_close_button(&mut self, ui: &mut egui::Ui) {
        if self.roi_state.mode == RoiSelectionMode::Polygon
            && self.roi_state.polygon_draft.is_some()
        {
            let close_response =
                Self::roi_icon_button(ui, RoiToolbarIcon::Close, "Close polygon (Enter)");
            if close_response.clicked() {
                let ctx = ui.ctx().clone();
                self.commit_polygon_draft(&ctx);
            }
        }
    }

    fn render_roi_settings_menu(&mut self, ui: &mut egui::Ui, colors: &ThemeColors) {
        let gear_button = egui::Button::new("")
            .min_size(egui::vec2(28.0, 22.0))
            .fill(Color32::TRANSPARENT)
            .stroke(Stroke::new(1.0, colors.border_light))
            .rounding(Rounding::same(4.0));
        let gear_response = egui::menu::menu_custom_button(ui, gear_button, |ui| {
            ui.checkbox(
                &mut self.roi_state.debounce_updates,
                "Debounce spectrum updates",
            );
        });
        let image = Self::roi_icon_image(RoiToolbarIcon::Gear, colors.text_muted);
        image.paint_at(ui, gear_response.response.rect.shrink(4.0));
        gear_response.response.on_hover_text("ROI settings");
    }

    fn render_roi_help_button(&mut self, ui: &mut egui::Ui, colors: &ThemeColors) {
        let help_button = egui::Button::new("?")
            .min_size(egui::vec2(24.0, 22.0))
            .fill(Color32::TRANSPARENT)
            .stroke(Stroke::new(1.0, colors.border_light))
            .rounding(Rounding::same(4.0));
        let response = ui.add(help_button);
        if response.clicked() {
            self.ui_state.panel_popups.show_roi_help = !self.ui_state.panel_popups.show_roi_help;
        }
        response.on_hover_text("ROI help");
    }

    fn render_roi_help_panel(&mut self, ctx: &egui::Context) {
        if !self.ui_state.panel_popups.show_roi_help {
            return;
        }
        let mut open = self.ui_state.panel_popups.show_roi_help;
        egui::Window::new("ROI Help")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(300.0)
            .show(ctx, |ui| {
                ui.label(egui::RichText::new("Create").strong());
                ui.label("• Shift + drag: rectangle ROI");
                ui.label("• Shift + click: add polygon vertex");
                ui.label("• Enter: close polygon (min 3 points)");
                ui.label("• Esc: cancel draft");
                ui.add_space(6.0);
                ui.label(egui::RichText::new("Select & move").strong());
                ui.label("• Click ROI to select");
                ui.label("• Drag ROI to move");
                ui.add_space(6.0);
                ui.label(egui::RichText::new("Edit & delete").strong());
                ui.label("• Double-click or right-click → Edit");
                ui.label("• Drag handles/vertices to reshape");
                ui.label("• Delete/Backspace removes selected ROI");
                ui.label("• Right-click → Delete, Clear removes all");
                ui.add_space(6.0);
                ui.label(egui::RichText::new("Tips").strong());
                ui.label("• ROIs stick to the image during pan/zoom");
                ui.label("• Use ROI settings to debounce spectrum updates");
            });
        self.ui_state.panel_popups.show_roi_help = open;
    }

    fn commit_polygon_draft(&mut self, ctx: &egui::Context) {
        if let Err(err) = self.roi_state.commit_polygon(3) {
            self.notify_roi_error(ctx, err);
            self.roi_state.cancel_draft();
        }
    }

    /// Render the spectrum panel with toolbar.
    fn render_spectrum_panel(
        &mut self,
        ctx: &egui::Context,
        ui: &mut egui::Ui,
        inputs: &mut SpectrumPanelInputs,
    ) {
        let colors = ThemeColors::from_ui(ui);
        self.update_roi_spectra(ctx);

        let mut spectrum_reset_clicked = self.ensure_energy_axis();
        let has_full_spectrum = inputs.spectrum.as_ref().is_some_and(|s| !s.is_empty());
        let has_visible_spectrum = self.has_visible_spectrum(inputs.spectrum.as_deref());

        let toolbar_actions =
            self.render_spectrum_toolbar(ui, &colors, has_full_spectrum, has_visible_spectrum);
        spectrum_reset_clicked |= toolbar_actions.reset_clicked;

        ui.add_space(4.0);
        self.render_roi_data_panel(ctx);
        self.render_spectrum_range_panel(ctx);
        self.render_spectrum_help_panel(ctx);

        let Some(plot_data) = self.build_spectrum_plot_data(&colors, inputs) else {
            Self::render_spectrum_empty(ui);
            return;
        };

        self.render_spectrum_plot(ui, &plot_data, inputs, spectrum_reset_clicked);
        self.handle_spectrum_exports(&plot_data, inputs, &toolbar_actions, colors);
        self.render_spectrum_legend_if_needed(ui, &plot_data.legend_items);
    }

    fn ensure_energy_axis(&mut self) -> bool {
        if self.ui_state.spectrum_x_axis == SpectrumXAxis::EnergyEv && self.flight_path_m <= 0.0 {
            self.ui_state.spectrum_x_axis = SpectrumXAxis::ToFMs;
            return true;
        }
        false
    }

    fn has_visible_spectrum(&self, spectrum: Option<&[u64]>) -> bool {
        let has_full_spectrum = spectrum.is_some_and(|s| !s.is_empty());
        (self.ui_state.spectrum.full_fov_visible && has_full_spectrum)
            || self.roi_state.rois.iter().any(|roi| {
                roi.visibility.spectrum_visible
                    && self
                        .roi_spectrum_data(roi.id)
                        .is_some_and(|data| !data.counts.is_empty())
            })
    }

    fn render_spectrum_toolbar(
        &mut self,
        ui: &mut egui::Ui,
        colors: &ThemeColors,
        has_full_spectrum: bool,
        has_visible_spectrum: bool,
    ) -> SpectrumToolbarActions {
        let mut actions = SpectrumToolbarActions::default();
        egui::Frame::none()
            .fill(colors.bg_panel)
            .stroke(Stroke::new(1.0, colors.border))
            .rounding(Rounding::same(4.0))
            .inner_margin(egui::Margin::symmetric(12.0, 8.0))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let colors = ThemeColors::from_ui(ui);
                    let energy_available = self.flight_path_m > 0.0;
                    if self.render_spectrum_axis_selector(ui, energy_available) {
                        actions.reset_clicked = true;
                    }
                    self.render_spectrum_settings_button(ui, &colors);
                    self.render_spectrum_data_button(ui, &colors);
                    self.render_spectrum_help_button(ui, &colors);

                    ui.add_space(8.0);
                    if self.render_spectrum_log_toggles(ui, &colors) {
                        actions.reset_clicked = true;
                    }
                    self.render_spectrum_range_button(ui, &colors);

                    ui.add_space(8.0);
                    Self::toolbar_divider(ui);
                    ui.add_space(8.0);

                    self.render_spectrum_export_buttons(
                        ui,
                        &colors,
                        has_full_spectrum,
                        has_visible_spectrum,
                        &mut actions,
                    );
                    self.render_spectrum_reset_controls(ui, &colors, &mut actions);
                });
            });
        actions
    }

    fn render_spectrum_axis_selector(&mut self, ui: &mut egui::Ui, energy_available: bool) -> bool {
        let prev_axis = self.ui_state.spectrum_x_axis;
        let response = egui::ComboBox::from_id_salt("tof_unit")
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
        response
            .response
            .on_hover_text("X-axis units (Energy requires flight path + TOF offset)");
        prev_axis != self.ui_state.spectrum_x_axis
    }

    fn render_spectrum_settings_button(&mut self, ui: &mut egui::Ui, colors: &ThemeColors) {
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
            self.ui_state.panels.show_spectrum_settings =
                !self.ui_state.panels.show_spectrum_settings;
        }
    }

    fn render_spectrum_data_button(&mut self, ui: &mut egui::Ui, colors: &ThemeColors) {
        let data_button = egui::Button::new("")
            .min_size(egui::vec2(28.0, 22.0))
            .fill(if self.ui_state.panel_popups.show_roi_panel {
                colors.bg_header
            } else {
                Color32::TRANSPARENT
            })
            .stroke(Stroke::new(1.0, colors.border_light))
            .rounding(Rounding::same(4.0));
        let data_response = ui.add(data_button);
        let data_icon = Self::roi_icon_image(RoiToolbarIcon::Data, colors.text_muted);
        data_icon.paint_at(ui, data_response.rect.shrink(4.0));
        if data_response
            .on_hover_text("Choose Full FOV / ROI curves to display")
            .clicked()
        {
            self.ui_state.panel_popups.show_roi_panel = !self.ui_state.panel_popups.show_roi_panel;
            if !self.ui_state.panel_popups.show_roi_panel {
                self.ui_state.roi_rename_id = None;
            }
        }
    }

    fn render_spectrum_range_button(&mut self, ui: &mut egui::Ui, colors: &ThemeColors) {
        let range_btn = egui::Button::new(
            egui::RichText::new("↔ Range")
                .size(10.0)
                .color(colors.text_dim),
        )
        .fill(Color32::TRANSPARENT)
        .stroke(Stroke::new(1.0, colors.border_light))
        .rounding(Rounding::same(4.0));
        if ui.add(range_btn).clicked() {
            let opening = !self.ui_state.panel_popups.show_spectrum_range;
            self.ui_state.panel_popups.show_spectrum_range = opening;
            if opening {
                self.populate_spectrum_range_inputs();
            }
        }
    }

    fn render_spectrum_help_button(&mut self, ui: &mut egui::Ui, colors: &ThemeColors) {
        let help_button = egui::Button::new("?")
            .min_size(egui::vec2(22.0, 22.0))
            .fill(if self.ui_state.panel_popups.show_spectrum_help {
                colors.bg_header
            } else {
                Color32::TRANSPARENT
            })
            .stroke(Stroke::new(1.0, colors.border_light))
            .rounding(Rounding::same(4.0));
        if ui.add(help_button).on_hover_text("Spectrum help").clicked() {
            self.ui_state.panel_popups.show_spectrum_help =
                !self.ui_state.panel_popups.show_spectrum_help;
        }
    }

    fn render_spectrum_export_buttons(
        &mut self,
        ui: &mut egui::Ui,
        colors: &ThemeColors,
        has_full_spectrum: bool,
        has_visible_spectrum: bool,
        actions: &mut SpectrumToolbarActions,
    ) {
        let png_btn = egui::Button::new(
            egui::RichText::new("📷 PNG")
                .size(10.0)
                .color(colors.text_dim),
        )
        .fill(Color32::TRANSPARENT)
        .stroke(Stroke::new(1.0, colors.border_light))
        .rounding(Rounding::same(4.0));
        if ui
            .add_enabled(has_full_spectrum, png_btn)
            .on_hover_text("Export spectrum as PNG")
            .clicked()
        {
            actions.export_png_clicked = true;
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
            .add_enabled(has_visible_spectrum, csv_btn)
            .on_hover_text("Export spectrum as CSV")
            .clicked()
        {
            actions.export_csv_clicked = true;
        }
    }

    fn render_spectrum_reset_controls(
        &mut self,
        ui: &mut egui::Ui,
        colors: &ThemeColors,
        actions: &mut SpectrumToolbarActions,
    ) {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            self.render_spectrum_zoom_group(ui);
            ui.add_space(6.0);
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
                actions.reset_clicked = true;
            }
        });
    }

    fn render_spectrum_log_toggles(&mut self, ui: &mut egui::Ui, colors: &ThemeColors) -> bool {
        let mut reset = false;
        if Self::render_log_toggle(ui, colors, "logX", self.ui_state.spectrum.log_x) {
            self.ui_state.spectrum.log_x = !self.ui_state.spectrum.log_x;
            reset = true;
        }

        if Self::render_log_toggle(ui, colors, "logY", self.ui_state.spectrum.log_y) {
            self.ui_state.spectrum.log_y = !self.ui_state.spectrum.log_y;
            reset = true;
        }
        reset
    }

    fn render_log_toggle(
        ui: &mut egui::Ui,
        colors: &ThemeColors,
        label: &str,
        enabled: bool,
    ) -> bool {
        let text_color = if enabled {
            Color32::WHITE
        } else {
            colors.text_dim
        };
        let fill = if enabled {
            accent::BLUE
        } else {
            Color32::TRANSPARENT
        };
        let button = egui::Button::new(egui::RichText::new(label).size(10.0).color(text_color))
            .fill(fill)
            .stroke(Stroke::new(1.0, colors.border_light))
            .rounding(Rounding::same(4.0));
        ui.add(button).clicked()
    }

    fn build_spectrum_plot_data(
        &self,
        colors: &ThemeColors,
        inputs: &SpectrumPanelInputs,
    ) -> Option<SpectrumPlotData> {
        let log_x = self.ui_state.spectrum.log_x;
        let log_y = self.ui_state.spectrum.log_y;
        let axis = self.ui_state.spectrum_x_axis;
        let flight_path_m = self.flight_path_m;
        let tof_offset_ns = self.tof_offset_ns;

        let (spec_bins, max_ms, bin_width_ms) =
            self.spectrum_bin_params(inputs.spectrum.as_deref(), inputs.n_bins);
        let line_config = SpectrumLineConfig {
            axis,
            log_x,
            log_y,
            bin_width_ms,
            spec_bins,
            flight_path_m,
            tof_offset_ns,
        };

        let mut lines: Vec<(String, Color32, Vec<[f64; 2]>)> = Vec::new();
        let mut legend_items: Vec<(String, Color32)> = Vec::new();
        let mut x_min = f64::INFINITY;
        let mut x_max = f64::NEG_INFINITY;
        let mut y_max: f64 = 0.0;

        if self.ui_state.spectrum.full_fov_visible {
            if let Some(full) = inputs.spectrum.as_ref() {
                if let Some((points, stats)) = Self::build_spectrum_line(full, line_config) {
                    x_min = x_min.min(stats.x_min);
                    x_max = x_max.max(stats.x_max);
                    y_max = y_max.max(stats.y_max);
                    legend_items.push(("Full FOV".to_string(), colors.text_muted));
                    lines.push(("Full FOV".to_string(), colors.text_muted, points));
                }
            }
        }

        for roi in &self.roi_state.rois {
            if !roi.visibility.spectrum_visible {
                continue;
            }
            let Some(data) = self.roi_spectrum_data(roi.id) else {
                continue;
            };
            if let Some((points, stats)) = Self::build_spectrum_line(&data.counts, line_config) {
                x_min = x_min.min(stats.x_min);
                x_max = x_max.max(stats.x_max);
                y_max = y_max.max(stats.y_max);
                legend_items.push((roi.name.clone(), roi.color));
                lines.push((roi.name.clone(), roi.color, points));
            }
        }

        if lines.is_empty() {
            return None;
        }

        let (x_min, x_max, y_max) = Self::sanitize_spectrum_bounds(x_min, x_max, y_max);
        let (x_label, y_label) = Self::spectrum_axis_labels(axis, log_x, log_y);
        let manual_bounds = self.spectrum_manual_bounds(log_x, log_y, x_min, x_max, y_max);
        let export_bounds =
            manual_bounds.unwrap_or_else(|| PlotBounds::from_min_max([x_min, 0.0], [x_max, y_max]));

        Some(SpectrumPlotData {
            axis,
            log_x,
            log_y,
            bin_width_ms,
            spec_bins,
            max_ms,
            x_min,
            x_max,
            y_max,
            x_label,
            y_label,
            lines,
            legend_items,
            manual_bounds,
            export_bounds,
            flight_path_m,
            tof_offset_ns,
        })
    }

    fn spectrum_bin_params(&self, spectrum: Option<&[u64]>, n_bins: usize) -> (usize, f64, f64) {
        let tdc_period = 1.0 / self.tdc_frequency;
        let max_ms = tdc_period * 1e3;
        let spec_bins = spectrum.map_or(n_bins, <[u64]>::len);
        let bin_width_ms = if spec_bins > 0 {
            max_ms / usize_to_f64(spec_bins)
        } else {
            0.0
        };
        (spec_bins, max_ms, bin_width_ms)
    }

    fn build_spectrum_line(
        counts: &[u64],
        config: SpectrumLineConfig,
    ) -> Option<(Vec<[f64; 2]>, SpectrumLineStats)> {
        if counts.is_empty() || config.spec_bins == 0 {
            return None;
        }
        let mut points = Vec::with_capacity(counts.len());
        let mut local_y_max: f64 = 0.0;
        let mut x_min_local = f64::INFINITY;
        let mut x_max_local = f64::NEG_INFINITY;
        for (i, &c) in counts.iter().enumerate() {
            let tof_ms = usize_to_f64(i) * config.bin_width_ms;
            let mut x = match config.axis {
                SpectrumXAxis::ToFMs => tof_ms,
                SpectrumXAxis::EnergyEv => {
                    let Some(e) =
                        tof_ms_to_energy_ev(tof_ms, config.flight_path_m, config.tof_offset_ns)
                    else {
                        continue;
                    };
                    e
                }
            };
            if config.log_x {
                if x <= 0.0 {
                    continue;
                }
                x = x.log10();
            }

            let mut y = u64_to_f64(c);
            if config.log_y {
                y = u64_to_f64(c.max(1)).log10();
            }
            local_y_max = local_y_max.max(y);
            x_min_local = x_min_local.min(x);
            x_max_local = x_max_local.max(x);
            points.push([x, y]);
        }

        if points.is_empty() {
            return None;
        }
        Some((
            points,
            SpectrumLineStats {
                x_min: x_min_local,
                x_max: x_max_local,
                y_max: local_y_max,
            },
        ))
    }

    fn sanitize_spectrum_bounds(x_min: f64, x_max: f64, y_max: f64) -> (f64, f64, f64) {
        let x_span = x_max - x_min;
        let mut x_min = x_min;
        let mut x_max = x_max;
        let mut y_max = y_max;
        if !x_min.is_finite() || !x_max.is_finite() || x_span.abs() <= f64::EPSILON {
            x_min = 0.0;
            x_max = 1.0;
        }
        if y_max <= 0.0 {
            y_max = 1.0;
        } else {
            y_max *= 1.05;
        }
        (x_min, x_max, y_max)
    }

    fn spectrum_axis_labels(axis: SpectrumXAxis, log_x: bool, log_y: bool) -> (String, String) {
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
        (x_label.to_string(), y_label.to_string())
    }

    fn spectrum_manual_bounds(
        &self,
        log_x: bool,
        log_y: bool,
        x_min: f64,
        x_max: f64,
        y_max: f64,
    ) -> Option<PlotBounds> {
        let x_override = self
            .ui_state
            .spectrum_x_range
            .and_then(|(min_val, max_val)| {
                if min_val >= max_val {
                    None
                } else if log_x {
                    if min_val > 0.0 && max_val > 0.0 {
                        Some((min_val.log10(), max_val.log10()))
                    } else {
                        None
                    }
                } else {
                    Some((min_val, max_val))
                }
            });
        let y_override = self
            .ui_state
            .spectrum_y_range
            .and_then(|(min_val, max_val)| {
                if min_val >= max_val {
                    None
                } else if log_y {
                    if min_val > 0.0 && max_val > 0.0 {
                        Some((min_val.log10(), max_val.log10()))
                    } else {
                        None
                    }
                } else {
                    Some((min_val, max_val))
                }
            });
        if x_override.is_some() || y_override.is_some() {
            let mut min = [x_min, 0.0];
            let mut max = [x_max, y_max];
            if let Some((min_val, max_val)) = x_override {
                min[0] = min_val;
                max[0] = max_val;
            }
            if let Some((min_val, max_val)) = y_override {
                min[1] = min_val;
                max[1] = max_val;
            }
            Some(PlotBounds::from_min_max(min, max))
        } else {
            None
        }
    }

    fn render_spectrum_plot(
        &mut self,
        ui: &mut egui::Ui,
        data: &SpectrumPlotData,
        inputs: &mut SpectrumPanelInputs,
        spectrum_reset_clicked: bool,
    ) {
        let zoom_mode = self.ui_state.spectrum_zoom_mode;
        let zoom_active = zoom_mode != ZoomMode::None;
        let mut zoom_start = self.ui_state.spectrum_zoom_start;

        let mut spectrum_plot = Plot::new("spectrum")
            .height(140.0)
            .x_axis_label(&data.x_label)
            .y_axis_label(&data.y_label)
            .include_x(data.x_min)
            .include_x(data.x_max)
            .include_y(0.0)
            .allow_drag(!zoom_active);

        if spectrum_reset_clicked {
            spectrum_plot = spectrum_plot.reset();
        }

        let plot_response = spectrum_plot.show(ui, |plot_ui| {
            if let Some(bounds) = data.manual_bounds {
                plot_ui.set_plot_bounds(bounds);
            }
            if spectrum_reset_clicked || plot_ui.response().double_clicked() {
                plot_ui.set_plot_bounds(PlotBounds::from_min_max(
                    [data.x_min, 0.0],
                    [data.x_max, data.y_max],
                ));
            }

            for (name, color, points) in &data.lines {
                plot_ui.line(
                    Line::new(PlotPoints::new(points.clone()))
                        .color(*color)
                        .name(name.as_str()),
                );
            }

            let response = plot_ui.response().clone();
            if response.hovered() && zoom_active {
                let icon = match zoom_mode {
                    ZoomMode::In => egui::CursorIcon::ZoomIn,
                    ZoomMode::Out => egui::CursorIcon::ZoomOut,
                    ZoomMode::Box => egui::CursorIcon::Crosshair,
                    ZoomMode::None => egui::CursorIcon::Default,
                };
                plot_ui.ctx().set_cursor_icon(icon);
            }

            self.handle_spectrum_zoom(plot_ui, &response, zoom_mode, &mut zoom_start);
            self.draw_spectrum_slice_marker(plot_ui, data, inputs);
            self.handle_spectrum_slice_drag(plot_ui, data, inputs, zoom_active);
        });

        self.ui_state.spectrum_zoom_start = zoom_start;
        self.ui_state.spectrum_last_plot_bounds = Some(*plot_response.transform.bounds());
        self.ui_state.spectrum_last_plot_rect = Some(plot_response.response.rect);

        self.handle_spectrum_slice_click(&plot_response, data, inputs, zoom_active);
    }

    fn handle_spectrum_zoom(
        &self,
        plot_ui: &mut egui_plot::PlotUi,
        response: &egui::Response,
        zoom_mode: ZoomMode,
        zoom_start: &mut Option<PlotPoint>,
    ) {
        if zoom_mode == ZoomMode::None {
            return;
        }
        match zoom_mode {
            ZoomMode::In | ZoomMode::Out => {
                if response.clicked() {
                    let center = plot_ui.pointer_coordinate().unwrap_or_else(|| {
                        let bounds = plot_ui.plot_bounds();
                        let min = bounds.min();
                        let max = bounds.max();
                        PlotPoint::new((min[0] + max[0]) * 0.5, (min[1] + max[1]) * 0.5)
                    });
                    let factor = if zoom_mode == ZoomMode::In { 1.25 } else { 0.8 };
                    plot_ui.zoom_bounds(Vec2::splat(zoom_factor_to_f32(factor)), center);
                }
            }
            ZoomMode::Box => {
                if response.drag_started() {
                    *zoom_start = plot_ui.pointer_coordinate();
                }
                if response.dragged() {
                    if let (Some(start), Some(current)) =
                        (*zoom_start, plot_ui.pointer_coordinate())
                    {
                        let start_screen = plot_ui.screen_from_plot(start);
                        let current_screen = plot_ui.screen_from_plot(current);
                        let rect = Rect::from_two_pos(start_screen, current_screen);
                        Self::draw_zoom_rect(plot_ui, response, rect);
                    }
                }
                if response.drag_stopped() {
                    if let (Some(start), Some(end)) = (*zoom_start, plot_ui.pointer_coordinate()) {
                        let min_x = start.x.min(end.x);
                        let max_x = start.x.max(end.x);
                        let min_y = start.y.min(end.y);
                        let max_y = start.y.max(end.y);
                        if (max_x - min_x) > f64::EPSILON && (max_y - min_y) > f64::EPSILON {
                            plot_ui.set_plot_bounds(PlotBounds::from_min_max(
                                [min_x, min_y],
                                [max_x, max_y],
                            ));
                        }
                    }
                    *zoom_start = None;
                }
            }
            ZoomMode::None => {}
        }
    }

    fn draw_spectrum_slice_marker(
        &self,
        plot_ui: &mut egui_plot::PlotUi,
        data: &SpectrumPlotData,
        inputs: &SpectrumPanelInputs,
    ) {
        if inputs.slicer_enabled && inputs.current_tof_bin < data.spec_bins {
            let slice_tof_ms = usize_to_f64(inputs.current_tof_bin) * data.bin_width_ms;
            let slice_x = match data.axis {
                SpectrumXAxis::ToFMs => Some(slice_tof_ms),
                SpectrumXAxis::EnergyEv => {
                    tof_ms_to_energy_ev(slice_tof_ms, data.flight_path_m, data.tof_offset_ns)
                }
            };

            if let Some(mut slice_x) = slice_x {
                if data.log_x {
                    if slice_x > 0.0 {
                        slice_x = slice_x.log10();
                    } else {
                        slice_x = data.x_min;
                    }
                }
                plot_ui.vline(
                    VLine::new(slice_x)
                        .color(accent::RED)
                        .width(1.0)
                        .style(egui_plot::LineStyle::Dashed { length: 4.0 })
                        .name(format!("Slice {}", inputs.current_tof_bin + 1)),
                );
            }
        }
    }

    fn handle_spectrum_slice_drag(
        &self,
        plot_ui: &mut egui_plot::PlotUi,
        data: &SpectrumPlotData,
        inputs: &mut SpectrumPanelInputs,
        zoom_active: bool,
    ) {
        if zoom_active || !inputs.slicer_enabled || data.spec_bins == 0 {
            return;
        }
        let drag_delta = plot_ui.pointer_coordinate_drag_delta();
        if drag_delta.x.abs() > 0.0 {
            if let Some(coord) = plot_ui.pointer_coordinate() {
                let mut x_axis = coord.x;
                if data.log_x {
                    x_axis = 10_f64.powf(x_axis);
                }
                let x_ms = match data.axis {
                    SpectrumXAxis::ToFMs => Some(x_axis),
                    SpectrumXAxis::EnergyEv => {
                        energy_ev_to_tof_ms(x_axis, data.flight_path_m, data.tof_offset_ns)
                    }
                };
                let Some(x_ms) = x_ms else {
                    return;
                };
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let bin = if x_ms <= 0.0 {
                    0
                } else if x_ms >= data.max_ms {
                    data.spec_bins - 1
                } else {
                    ((x_ms / data.bin_width_ms) as usize).min(data.spec_bins - 1)
                };
                if bin != inputs.current_tof_bin {
                    *inputs.new_tof_bin = Some(bin);
                }
            }
        }
    }

    fn handle_spectrum_slice_click(
        &self,
        plot_response: &egui_plot::PlotResponse<()>,
        data: &SpectrumPlotData,
        inputs: &mut SpectrumPanelInputs,
        zoom_active: bool,
    ) {
        if zoom_active
            || !inputs.slicer_enabled
            || data.spec_bins == 0
            || !plot_response.response.clicked()
        {
            return;
        }
        if let Some(pos) = plot_response.response.interact_pointer_pos() {
            let plot_bounds = plot_response.transform.bounds();
            let plot_rect = plot_response.response.rect;
            let x_frac = f64::from(pos.x - plot_rect.left()) / f64::from(plot_rect.width());
            let x_plot =
                plot_bounds.min()[0] + x_frac * (plot_bounds.max()[0] - plot_bounds.min()[0]);
            let mut x_axis = x_plot;
            if data.log_x {
                x_axis = 10_f64.powf(x_axis);
            }
            let x_ms = match data.axis {
                SpectrumXAxis::ToFMs => Some(x_axis),
                SpectrumXAxis::EnergyEv => {
                    energy_ev_to_tof_ms(x_axis, data.flight_path_m, data.tof_offset_ns)
                }
            };
            let Some(x_ms) = x_ms else {
                return;
            };
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let bin = if x_ms <= 0.0 {
                0
            } else if x_ms >= data.max_ms {
                data.spec_bins - 1
            } else {
                ((x_ms / data.bin_width_ms) as usize).min(data.spec_bins - 1)
            };
            if bin != inputs.current_tof_bin {
                *inputs.new_tof_bin = Some(bin);
            }
        }
    }

    fn render_spectrum_empty(ui: &mut egui::Ui) {
        let colors = ThemeColors::from_ui(ui);
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
                        egui::RichText::new("Spectrum: No visible data")
                            .size(11.0)
                            .color(colors.text_dim),
                    );
                });
            });
    }

    fn handle_spectrum_exports(
        &mut self,
        data: &SpectrumPlotData,
        inputs: &SpectrumPanelInputs,
        actions: &SpectrumToolbarActions,
        colors: ThemeColors,
    ) {
        if actions.export_csv_clicked {
            self.force_roi_spectra_update();
            let full = inputs.spectrum.as_ref().map(Vec::as_slice);
            let axis_config = SpectrumAxisConfig {
                axis: data.axis,
                flight_path_m: data.flight_path_m,
                tof_offset_ns: data.tof_offset_ns,
            };
            if let Err(err) = Self::export_spectrum_csv(
                full,
                &self.roi_state.rois,
                self.roi_spectra_map(),
                self.ui_state.spectrum.full_fov_visible,
                data.bin_width_ms,
                axis_config,
            ) {
                log::error!("Failed to export spectrum CSV: {err}");
            }
        }

        if actions.export_png_clicked && !data.lines.is_empty() {
            let export_config = SpectrumExportConfig {
                axis: data.axis,
                log_x: data.log_x,
                log_y: data.log_y,
            };
            if let Err(err) =
                Self::export_spectrum_png(&data.lines, data.export_bounds, colors, &export_config)
            {
                log::error!("Failed to export spectrum PNG: {err}");
            }
        }
    }

    fn render_spectrum_legend_if_needed(
        &self,
        ui: &mut egui::Ui,
        legend_items: &[(String, Color32)],
    ) {
        if !legend_items.is_empty() {
            ui.add_space(4.0);
            Self::render_spectrum_legend(ui, legend_items);
        }
    }

    fn render_roi_data_panel(&mut self, ctx: &egui::Context) {
        if !self.ui_state.panel_popups.show_roi_panel {
            return;
        }
        let mut open = self.ui_state.panel_popups.show_roi_panel;
        egui::Window::new("Spectrum Data")
            .open(&mut open)
            .resizable(false)
            .collapsible(false)
            .default_width(240.0)
            .show(ctx, |ui| {
                self.render_roi_data_panel_contents(ui);
            });
        self.ui_state.panel_popups.show_roi_panel = open;
        if !open {
            self.ui_state.roi_rename_id = None;
        }
    }

    fn render_roi_data_panel_contents(&mut self, ui: &mut egui::Ui) {
        let colors = ThemeColors::from_ui(ui);
        ui.label(
            egui::RichText::new("Data selection")
                .size(11.0)
                .color(colors.text_dim),
        );
        ui.separator();
        ui.checkbox(&mut self.ui_state.spectrum.full_fov_visible, "Full FOV");

        self.sync_roi_rename_id();
        if self.roi_state.rois.is_empty() {
            Self::render_roi_data_empty(ui, &colors);
        } else {
            self.render_roi_data_list(ui, &colors);
        }

        ui.separator();
        self.render_roi_visibility_buttons(ui);
    }

    fn sync_roi_rename_id(&mut self) {
        if let Some(active_id) = self.ui_state.roi_rename_id {
            let exists = self.roi_state.rois.iter().any(|roi| roi.id == active_id);
            if !exists {
                self.ui_state.roi_rename_id = None;
            }
        }
    }

    fn render_roi_data_empty(ui: &mut egui::Ui, colors: &ThemeColors) {
        ui.add_space(6.0);
        ui.label(
            egui::RichText::new("No ROIs yet")
                .size(10.0)
                .color(colors.text_dim),
        );
    }

    fn render_roi_data_list(&mut self, ui: &mut egui::Ui, colors: &ThemeColors) {
        let (ui_state, roi_state) = (&mut self.ui_state, &mut self.roi_state);
        ui.add_space(6.0);
        for roi in &mut roi_state.rois {
            ui.horizontal(|ui| {
                ui.checkbox(&mut roi.visibility.spectrum_visible, "");
                ui.add(Self::legend_box(roi.color));
                if ui_state.roi_rename_id == Some(roi.id) {
                    let response = ui.add(
                        egui::TextEdit::singleline(&mut ui_state.roi_rename_text)
                            .desired_width(140.0),
                    );
                    let commit =
                        response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    let cancel = ui.input(|i| i.key_pressed(egui::Key::Escape));
                    if commit {
                        let name = ui_state.roi_rename_text.trim();
                        if !name.is_empty() {
                            roi.name = name.to_string();
                        }
                        ui_state.roi_rename_id = None;
                    } else if cancel {
                        ui_state.roi_rename_id = None;
                    }
                } else {
                    let label = ui.selectable_label(false, roi.name.clone());
                    if label.double_clicked() {
                        ui_state.roi_rename_id = Some(roi.id);
                        ui_state.roi_rename_text.clone_from(&roi.name);
                    }
                    let rename_icon =
                        egui::Image::new(egui::include_image!("../../assets/icons/roi-rename.svg"))
                            .tint(colors.text_muted)
                            .fit_to_exact_size(egui::vec2(12.0, 12.0));
                    if ui
                        .add(egui::ImageButton::new(rename_icon).frame(true))
                        .on_hover_text("Rename ROI")
                        .clicked()
                    {
                        ui_state.roi_rename_id = Some(roi.id);
                        ui_state.roi_rename_text.clone_from(&roi.name);
                    }
                }
            });
        }
    }

    fn render_roi_visibility_buttons(&mut self, ui: &mut egui::Ui) {
        let (ui_state, roi_state) = (&mut self.ui_state, &mut self.roi_state);
        ui.horizontal_wrapped(|ui| {
            if ui.button("Show Full FOV Only").clicked() {
                ui_state.spectrum.full_fov_visible = true;
                for roi in &mut roi_state.rois {
                    roi.visibility.spectrum_visible = false;
                }
            }
            if ui.button("Show All ROIs").clicked() {
                for roi in &mut roi_state.rois {
                    roi.visibility.spectrum_visible = true;
                }
            }
            if ui.button("Hide All ROIs").clicked() {
                for roi in &mut roi_state.rois {
                    roi.visibility.spectrum_visible = false;
                }
            }
        });
    }

    fn render_spectrum_range_panel(&mut self, ctx: &egui::Context) {
        if !self.ui_state.panel_popups.show_spectrum_range {
            return;
        }
        let mut open = self.ui_state.panel_popups.show_spectrum_range;
        let axis_label = match self.ui_state.spectrum_x_axis {
            SpectrumXAxis::ToFMs => "TOF (ms)",
            SpectrumXAxis::EnergyEv => "Energy (eV)",
        };
        egui::Window::new("Spectrum Range")
            .open(&mut open)
            .resizable(false)
            .collapsible(false)
            .default_width(240.0)
            .show(ctx, |ui| {
                self.render_spectrum_range_contents(ui, axis_label);
            });

        self.ui_state.panel_popups.show_spectrum_range = open;
    }

    fn render_spectrum_help_panel(&mut self, ctx: &egui::Context) {
        if !self.ui_state.panel_popups.show_spectrum_help {
            return;
        }
        let mut open = self.ui_state.panel_popups.show_spectrum_help;
        egui::Window::new("Spectrum Help")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(320.0)
            .show(ctx, |ui| {
                ui.label(egui::RichText::new("Axes").strong());
                ui.label("• Switch TOF / Energy in the dropdown.");
                ui.label("• Energy axis needs flight path + TOF offset.");
                ui.add_space(6.0);
                ui.label(egui::RichText::new("Visibility").strong());
                ui.label("• Use the data button to toggle Full FOV and ROIs.");
                ui.add_space(6.0);
                ui.label(egui::RichText::new("Scaling & range").strong());
                ui.label("• logX/logY toggles adjust scaling.");
                ui.label("• Range panel constrains x/y bounds.");
                ui.add_space(6.0);
                ui.label(egui::RichText::new("Zoom & export").strong());
                ui.label("• Zoom with buttons or selection box.");
                ui.label("• Export PNG/CSV from the toolbar.");
            });
        self.ui_state.panel_popups.show_spectrum_help = open;
    }

    fn render_spectrum_range_contents(&mut self, ui: &mut egui::Ui, axis_label: &str) {
        let colors = ThemeColors::from_ui(ui);
        ui.label(
            egui::RichText::new(format!("X axis: {axis_label}"))
                .size(10.0)
                .color(colors.text_dim),
        );
        ui.add_space(6.0);
        self.render_spectrum_range_inputs(ui);
        ui.add_space(6.0);
        let error = self.render_spectrum_range_actions(ui);
        if let Some(message) = error {
            ui.add_space(4.0);
            ui.label(egui::RichText::new(message).size(10.0).color(accent::RED));
        }
    }

    fn render_spectrum_range_inputs(&mut self, ui: &mut egui::Ui) {
        let ui_state = &mut self.ui_state;
        ui.horizontal(|ui| {
            ui.label("X min");
            ui.add(
                egui::TextEdit::singleline(&mut ui_state.spectrum_x_min_input)
                    .desired_width(70.0)
                    .hint_text("auto"),
            );
            ui.label("X max");
            ui.add(
                egui::TextEdit::singleline(&mut ui_state.spectrum_x_max_input)
                    .desired_width(70.0)
                    .hint_text("auto"),
            );
        });
        ui.horizontal(|ui| {
            ui.label("Y min");
            ui.add(
                egui::TextEdit::singleline(&mut ui_state.spectrum_y_min_input)
                    .desired_width(70.0)
                    .hint_text("auto"),
            );
            ui.label("Y max");
            ui.add(
                egui::TextEdit::singleline(&mut ui_state.spectrum_y_max_input)
                    .desired_width(70.0)
                    .hint_text("auto"),
            );
        });
    }

    fn render_spectrum_range_actions(&mut self, ui: &mut egui::Ui) -> Option<&'static str> {
        let ui_state = &mut self.ui_state;
        let mut error: Option<&'static str> = None;
        ui.horizontal(|ui| {
            if ui.button("Apply").clicked() {
                match parse_spectrum_range(
                    &ui_state.spectrum_x_min_input,
                    &ui_state.spectrum_x_max_input,
                ) {
                    Ok(range) => ui_state.spectrum_x_range = range,
                    Err(()) => error = Some("Invalid X range"),
                }

                match parse_spectrum_range(
                    &ui_state.spectrum_y_min_input,
                    &ui_state.spectrum_y_max_input,
                ) {
                    Ok(range) => ui_state.spectrum_y_range = range,
                    Err(()) => {
                        if error.is_none() {
                            error = Some("Invalid Y range");
                        }
                    }
                }
            }
            if ui.button("Clear").clicked() {
                ui_state.spectrum_x_range = None;
                ui_state.spectrum_y_range = None;
                ui_state.spectrum_x_min_input.clear();
                ui_state.spectrum_x_max_input.clear();
                ui_state.spectrum_y_min_input.clear();
                ui_state.spectrum_y_max_input.clear();
            }
        });
        error
    }

    fn populate_spectrum_range_inputs(&mut self) {
        if let Some((min_val, max_val)) = self.ui_state.spectrum_x_range {
            self.ui_state.spectrum_x_min_input = format!("{min_val:.3}");
            self.ui_state.spectrum_x_max_input = format!("{max_val:.3}");
        } else {
            self.ui_state.spectrum_x_min_input.clear();
            self.ui_state.spectrum_x_max_input.clear();
        }
        if let Some((min_val, max_val)) = self.ui_state.spectrum_y_range {
            self.ui_state.spectrum_y_min_input = format!("{min_val:.3}");
            self.ui_state.spectrum_y_max_input = format!("{max_val:.3}");
        } else {
            self.ui_state.spectrum_y_min_input.clear();
            self.ui_state.spectrum_y_max_input.clear();
        }
    }

    fn render_spectrum_legend(ui: &mut egui::Ui, items: &[(String, Color32)]) {
        let colors = ThemeColors::from_ui(ui);
        egui::Frame::none()
            .fill(colors.bg_panel)
            .stroke(Stroke::new(1.0, colors.border))
            .rounding(Rounding::same(4.0))
            .inner_margin(egui::Margin::symmetric(10.0, 6.0))
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        egui::RichText::new("Legend")
                            .size(10.0)
                            .color(colors.text_dim),
                    );
                    ui.add_space(6.0);
                    for (name, color) in items {
                        ui.add(Self::legend_box(*color));
                        ui.label(
                            egui::RichText::new(name.clone())
                                .size(10.0)
                                .color(colors.text_muted),
                        );
                        ui.add_space(6.0);
                    }
                });
            });
    }

    fn render_histogram_zoom_group(&mut self, ui: &mut egui::Ui) {
        let mut mode = self.ui_state.hist_zoom_mode;
        mode = Self::zoom_mode_button(ui, mode, ZoomMode::In, "Zoom in");
        mode = Self::zoom_mode_button(ui, mode, ZoomMode::Out, "Zoom out");
        mode = Self::zoom_mode_button(ui, mode, ZoomMode::Box, "Zoom to selection");
        if mode != ZoomMode::Box {
            self.ui_state.hist_zoom_start = None;
        }
        self.ui_state.hist_zoom_mode = mode;
    }

    fn render_spectrum_zoom_group(&mut self, ui: &mut egui::Ui) {
        let mut mode = self.ui_state.spectrum_zoom_mode;
        mode = Self::zoom_mode_button(ui, mode, ZoomMode::In, "Zoom in");
        mode = Self::zoom_mode_button(ui, mode, ZoomMode::Out, "Zoom out");
        mode = Self::zoom_mode_button(ui, mode, ZoomMode::Box, "Zoom to selection");
        if mode != ZoomMode::Box {
            self.ui_state.spectrum_zoom_start = None;
        }
        self.ui_state.spectrum_zoom_mode = mode;
    }

    fn zoom_mode_button(
        ui: &mut egui::Ui,
        current: ZoomMode,
        target: ZoomMode,
        tooltip: &str,
    ) -> ZoomMode {
        let icon = match target {
            ZoomMode::In => ZoomToolbarIcon::In,
            ZoomMode::Out => ZoomToolbarIcon::Out,
            ZoomMode::Box | ZoomMode::None => ZoomToolbarIcon::Box,
        };
        let active = current == target;
        let response = Self::zoom_icon_button(ui, icon, active, tooltip);
        if response.clicked() {
            if active {
                ZoomMode::None
            } else {
                target
            }
        } else {
            current
        }
    }

    fn zoom_icon_button(
        ui: &mut egui::Ui,
        icon: ZoomToolbarIcon,
        active: bool,
        tooltip: &str,
    ) -> egui::Response {
        let colors = ThemeColors::from_ui(ui);
        let tint = if active {
            Color32::WHITE
        } else {
            colors.text_muted
        };
        let button = egui::Button::new("")
            .min_size(egui::vec2(30.0, 28.0))
            .fill(if active {
                accent::BLUE
            } else {
                Color32::TRANSPARENT
            })
            .stroke(Stroke::new(1.0, colors.border_light))
            .rounding(Rounding::same(4.0));
        let response = ui.add(button);
        let image = Self::zoom_icon_image(icon, tint);
        image.paint_at(ui, response.rect.shrink(5.0));
        response.on_hover_text(tooltip)
    }

    fn zoom_icon_image(icon: ZoomToolbarIcon, tint: Color32) -> egui::Image<'static> {
        let source = match icon {
            ZoomToolbarIcon::In => egui::include_image!("../../assets/icons/zoom-in.svg"),
            ZoomToolbarIcon::Out => egui::include_image!("../../assets/icons/zoom-out.svg"),
            ZoomToolbarIcon::Box => egui::include_image!("../../assets/icons/zoom-box.svg"),
        };
        egui::Image::new(source)
            .tint(tint)
            .fit_to_exact_size(egui::vec2(16.0, 16.0))
    }

    /// Render a toolbar divider.
    fn toolbar_divider(ui: &mut egui::Ui) {
        let colors = ThemeColors::from_ui(ui);
        let rect = ui.allocate_space(egui::vec2(1.0, 24.0));
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

    fn roi_icon_button(ui: &mut egui::Ui, icon: RoiToolbarIcon, tooltip: &str) -> egui::Response {
        let colors = ThemeColors::from_ui(ui);
        let button = egui::Button::new("")
            .min_size(egui::vec2(28.0, 22.0))
            .fill(Color32::TRANSPARENT)
            .stroke(Stroke::new(1.0, colors.border_light))
            .rounding(Rounding::same(4.0));
        let response = ui.add(button);
        let image = Self::roi_icon_image(icon, colors.text_muted);
        image.paint_at(ui, response.rect.shrink(4.0));
        response.on_hover_text(tooltip)
    }

    fn paint_roi_icon_in_ui(ui: &mut egui::Ui, icon: RoiToolbarIcon, color: Color32) {
        let image = Self::roi_icon_image(icon, color);
        ui.add(image);
    }

    fn roi_icon_image(icon: RoiToolbarIcon, tint: Color32) -> egui::Image<'static> {
        let source = match icon {
            RoiToolbarIcon::Rectangle => {
                egui::include_image!("../../assets/icons/roi-rectangle.svg")
            }
            RoiToolbarIcon::Polygon => egui::include_image!("../../assets/icons/roi-polygon.svg"),
            RoiToolbarIcon::Clear => egui::include_image!("../../assets/icons/roi-clear.svg"),
            RoiToolbarIcon::Gear => egui::include_image!("../../assets/icons/roi-gear.svg"),
            RoiToolbarIcon::Close => egui::include_image!("../../assets/icons/roi-close.svg"),
            RoiToolbarIcon::Data => egui::include_image!("../../assets/icons/roi-data.svg"),
        };
        egui::Image::new(source)
            .tint(tint)
            .fit_to_exact_size(egui::vec2(16.0, 16.0))
    }

    fn paint_dropdown_caret(painter: &egui::Painter, rect: Rect, color: Color32) {
        let center = rect.center();
        let size = 3.5;
        let points = vec![
            Pos2::new(rect.right() - 8.0, center.y - size),
            Pos2::new(rect.right() - 2.0, center.y - size),
            Pos2::new(rect.right() - 5.0, center.y + size),
        ];
        painter.add(egui::Shape::convex_polygon(points, color, Stroke::NONE));
    }

    fn export_spectrum_csv(
        full: Option<&[u64]>,
        rois: &[Roi],
        roi_spectra: &HashMap<usize, RoiSpectrumEntry>,
        full_visible: bool,
        bin_width_ms: f64,
        axis_config: SpectrumAxisConfig,
    ) -> anyhow::Result<()> {
        let Some(path) = FileDialog::new().set_file_name("spectrum.csv").save_file() else {
            return Ok(());
        };

        let mut file = File::create(path)?;
        let axis = axis_config.axis;
        let flight_path_m = axis_config.flight_path_m;
        let tof_offset_ns = axis_config.tof_offset_ns;
        let include_energy = flight_path_m > 0.0;
        let include_full = full_visible && full.is_some();
        let full = full.unwrap_or(&[]);
        let mut visible_rois = Vec::new();
        for roi in rois {
            if !roi.visibility.spectrum_visible {
                continue;
            }
            let Some(entry) = roi_spectra.get(&roi.id) else {
                continue;
            };
            visible_rois.push((roi, &entry.data));
        }

        let mut header_cols = Vec::new();
        header_cols.push("TOF (ms)".to_string());
        if include_energy {
            header_cols.push("Energy (eV)".to_string());
        }
        if include_full {
            header_cols.push("Full FOV (counts)".to_string());
        }
        for (roi, _) in &visible_rois {
            header_cols.push(format!("{} (counts)", roi.name));
        }
        writeln!(file, "# Spectrum axis: {axis}")?;
        if include_energy {
            writeln!(file, "# Flight path (m): {flight_path_m:.4}")?;
            writeln!(file, "# TOF offset (ns): {tof_offset_ns:.4}")?;
        }
        writeln!(file, "# {}", header_cols.join(", "))?;
        writeln!(file, "#")?;

        for (roi, data) in &visible_rois {
            match &roi.shape {
                crate::viewer::RoiShape::Rectangle { x1, y1, x2, y2 } => {
                    writeln!(
                        file,
                        "# {}: Rectangle (x1={}, y1={}, x2={}, y2={})",
                        roi.name,
                        round_to_i32_clamped(*x1),
                        round_to_i32_clamped(*y1),
                        round_to_i32_clamped(*x2),
                        round_to_i32_clamped(*y2)
                    )?;
                }
                crate::viewer::RoiShape::Polygon { vertices } => {
                    let vertices_str = vertices
                        .iter()
                        .map(|(x, y)| {
                            format!(
                                "({}, {})",
                                round_to_i32_clamped(*x),
                                round_to_i32_clamped(*y)
                            )
                        })
                        .collect::<Vec<_>>()
                        .join(", ");
                    writeln!(file, "# {}: Polygon (vertices: {})", roi.name, vertices_str)?;
                }
            }
            writeln!(file, "#   - Area (continuous): {:.0} px^2", data.area.abs())?;
            writeln!(file, "#   - Included pixels: {}", data.pixel_count)?;
            writeln!(file, "#")?;
        }

        let mut max_bins = full.len();
        for (_, data) in &visible_rois {
            max_bins = max_bins.max(data.counts.len());
        }

        for i in 0..max_bins {
            let tof_ms = usize_to_f64(i) * bin_width_ms;
            let energy = if include_energy {
                tof_ms_to_energy_ev(tof_ms, flight_path_m, tof_offset_ns)
            } else {
                None
            };
            if include_energy && energy.is_none() {
                continue;
            }
            let mut row = Vec::new();
            row.push(format!("{tof_ms:.6}"));
            if let Some(energy) = energy {
                row.push(format!("{energy:.6}"));
            }
            if include_full {
                let count = full.get(i).copied().unwrap_or(0);
                row.push(count.to_string());
            }
            for (_, data) in &visible_rois {
                let count = data.counts.get(i).copied().unwrap_or(0);
                row.push(count.to_string());
            }
            writeln!(file, "{}", row.join(","))?;
        }

        Ok(())
    }

    fn export_spectrum_png(
        lines: &[(String, Color32, Vec<[f64; 2]>)],
        bounds: PlotBounds,
        colors: ThemeColors,
        export: &SpectrumExportConfig,
    ) -> anyhow::Result<()> {
        let Some(path) = FileDialog::new().set_file_name("spectrum.png").save_file() else {
            return Ok(());
        };
        let (mut img, geometry) = Self::spectrum_export_canvas(bounds, colors);
        Self::draw_spectrum_grid(&mut img, &geometry);
        Self::draw_spectrum_axes(&mut img, &geometry);
        Self::draw_spectrum_ticks(&mut img, &geometry);
        Self::draw_spectrum_axis_labels(&mut img, &geometry, export);
        Self::draw_spectrum_lines(&mut img, &geometry, lines);
        Self::draw_spectrum_legend(&mut img, &geometry, lines);

        img.save(path)?;
        Ok(())
    }

    fn spectrum_export_canvas(
        bounds: PlotBounds,
        colors: ThemeColors,
    ) -> (RgbaImage, SpectrumExportGeometry) {
        let width: u32 = 800;
        let height: u32 = 240;
        let pad: i32 = 32;
        let width_i32 = i32::try_from(width).unwrap_or(i32::MAX);
        let height_i32 = i32::try_from(height).unwrap_or(i32::MAX);
        let pad_f64 = f64::from(pad);
        let bg = Self::color32_to_rgba(colors.bg_panel);
        let axis_color = Self::color32_to_rgba(colors.border);
        let grid_color = Self::color32_to_rgba(Color32::from_rgba_unmultiplied(
            colors.border_light.r(),
            colors.border_light.g(),
            colors.border_light.b(),
            80,
        ));
        let label_color = Self::color32_to_rgba(colors.text_muted);

        let img = RgbaImage::from_pixel(width, height, bg);

        let min = bounds.min();
        let max = bounds.max();
        let mut x_min = min[0];
        let mut x_max = max[0];
        let mut y_min = min[1];
        let mut y_max = max[1];

        if !x_min.is_finite() || !x_max.is_finite() || x_min >= x_max {
            x_min = 0.0;
            x_max = 1.0;
        }
        if !y_min.is_finite() || !y_max.is_finite() || y_min >= y_max {
            y_min = 0.0;
            y_max = 1.0;
        }

        let plot_w = f64::from((width_i32 - pad * 2).max(1));
        let plot_h = f64::from((height_i32 - pad * 2).max(1));
        let x_scale = plot_w / (x_max - x_min).max(1e-9);
        let y_scale = plot_h / (y_max - y_min).max(1e-9);

        let plot_left = pad;
        let plot_right = width_i32 - pad;
        let plot_top = pad;
        let plot_bottom = height_i32 - pad;

        (
            img,
            SpectrumExportGeometry {
                width,
                height,
                pad,
                pad_f64,
                plot_left,
                plot_right,
                plot_top,
                plot_bottom,
                plot_w,
                plot_h,
                x_min,
                x_max,
                y_min,
                y_max,
                x_scale,
                y_scale,
                axis_color,
                grid_color,
                label_color,
            },
        )
    }

    fn draw_spectrum_grid(img: &mut RgbaImage, geo: &SpectrumExportGeometry) {
        let width_i32 = i32::try_from(geo.width).unwrap_or(i32::MAX);
        let height_i32 = i32::try_from(geo.height).unwrap_or(i32::MAX);
        for i in 1..5 {
            let t = f64::from(i) / 5.0;
            let x = geo.pad_f64 + t * geo.plot_w;
            let y = geo.pad_f64 + t * geo.plot_h;
            Self::draw_line(
                img,
                round_to_i32_clamped(x),
                geo.pad,
                round_to_i32_clamped(x),
                height_i32 - geo.pad,
                geo.grid_color,
            );
            Self::draw_line(
                img,
                geo.pad,
                round_to_i32_clamped(y),
                width_i32 - geo.pad,
                round_to_i32_clamped(y),
                geo.grid_color,
            );
        }
    }

    fn draw_spectrum_axes(img: &mut RgbaImage, geo: &SpectrumExportGeometry) {
        let width_i32 = i32::try_from(geo.width).unwrap_or(i32::MAX);
        let height_i32 = i32::try_from(geo.height).unwrap_or(i32::MAX);
        Self::draw_line(
            img,
            geo.pad,
            height_i32 - geo.pad,
            width_i32 - geo.pad,
            height_i32 - geo.pad,
            geo.axis_color,
        );
        Self::draw_line(
            img,
            geo.pad,
            geo.pad,
            geo.pad,
            height_i32 - geo.pad,
            geo.axis_color,
        );
    }

    fn draw_spectrum_ticks(img: &mut RgbaImage, geo: &SpectrumExportGeometry) {
        let tick_count: i32 = 5;
        for i in 0..=tick_count {
            let t = f64::from(i) / f64::from(tick_count);
            let x_val = geo.x_min + t * (geo.x_max - geo.x_min);
            let y_val = geo.y_min + t * (geo.y_max - geo.y_min);
            let x_pos = geo.pad_f64 + t * geo.plot_w;
            let y_pos = geo.pad_f64 + (1.0 - t) * geo.plot_h;

            let x_i = round_to_i32_clamped(x_pos);
            let y_i = round_to_i32_clamped(y_pos);

            Self::draw_line(
                img,
                x_i,
                geo.plot_bottom,
                x_i,
                geo.plot_bottom + 4,
                geo.axis_color,
            );
            let x_label = Self::format_tick(x_val);
            let x_label_width = text_width_px(&x_label);
            Self::draw_text(
                img,
                x_i - (x_label_width / 2),
                geo.plot_bottom + 8,
                &x_label,
                geo.label_color,
            );

            Self::draw_line(
                img,
                geo.plot_left - 4,
                y_i,
                geo.plot_left,
                y_i,
                geo.axis_color,
            );
            let y_label = Self::format_tick(y_val);
            let y_label_width = text_width_px(&y_label);
            Self::draw_text(
                img,
                geo.plot_left - 6 - y_label_width,
                y_i - 4,
                &y_label,
                geo.label_color,
            );
        }
    }

    fn draw_spectrum_axis_labels(
        img: &mut RgbaImage,
        geo: &SpectrumExportGeometry,
        export: &SpectrumExportConfig,
    ) {
        let axis_label = match export.axis {
            SpectrumXAxis::ToFMs => "TOF (MS)",
            SpectrumXAxis::EnergyEv => "ENERGY (EV)",
        };
        let x_label_text = if export.log_x {
            format!("LOG10({axis_label})")
        } else {
            axis_label.to_string()
        };
        let y_label_text = if export.log_y {
            "LOG10(COUNTS)".to_string()
        } else {
            "COUNTS".to_string()
        };

        let x_label = x_label_text.to_ascii_uppercase();
        let y_label = y_label_text.to_ascii_uppercase();
        let x_label_width = text_width_px(&x_label);
        Self::draw_text(
            img,
            i32::midpoint(geo.plot_left, geo.plot_right) - (x_label_width / 2),
            geo.plot_bottom + 20,
            &x_label,
            geo.label_color,
        );
        Self::draw_text_vertical(
            img,
            geo.plot_left - 24,
            i32::midpoint(geo.plot_top, geo.plot_bottom) - 18,
            &y_label,
            geo.label_color,
        );
    }

    fn draw_spectrum_lines(
        img: &mut RgbaImage,
        geo: &SpectrumExportGeometry,
        lines: &[(String, Color32, Vec<[f64; 2]>)],
    ) {
        let height_i32 = i32::try_from(geo.height).unwrap_or(i32::MAX);
        for (_, color, points) in lines {
            let line_color = Self::color32_to_rgba(*color);
            let mut prev: Option<(i32, i32)> = None;
            for point in points {
                let px = geo.pad_f64 + (point[0] - geo.x_min) * geo.x_scale;
                let py = f64::from(height_i32) - geo.pad_f64 - (point[1] - geo.y_min) * geo.y_scale;
                let pixel = (round_to_i32_clamped(px), round_to_i32_clamped(py));
                if pixel.0 < geo.plot_left
                    || pixel.0 > geo.plot_right
                    || pixel.1 < geo.plot_top
                    || pixel.1 > geo.plot_bottom
                {
                    prev = None;
                    continue;
                }
                if let Some((prev_x, prev_y)) = prev {
                    Self::draw_line(img, prev_x, prev_y, pixel.0, pixel.1, line_color);
                }
                prev = Some(pixel);
            }
        }
    }

    fn draw_spectrum_legend(
        img: &mut RgbaImage,
        geo: &SpectrumExportGeometry,
        lines: &[(String, Color32, Vec<[f64; 2]>)],
    ) {
        let legend_x = geo.plot_left + 6;
        let legend_y = geo.plot_top + 6;
        let cursor_x = legend_x;
        let mut cursor_y = legend_y;
        Self::draw_text(img, cursor_x, cursor_y, "LEGEND", geo.label_color);
        cursor_y += 10;
        for (name, color, _) in lines {
            let name = name.to_ascii_uppercase();
            let box_color = Self::color32_to_rgba(*color);
            Self::draw_rect_filled(
                img,
                cursor_x,
                cursor_y + 2,
                cursor_x + 8,
                cursor_y + 10,
                box_color,
            );
            Self::draw_text(img, cursor_x + 12, cursor_y, &name, geo.label_color);
            cursor_y += 10;
        }
    }

    fn format_tick(value: f64) -> String {
        let abs = value.abs();
        if abs >= 100.0 {
            format!("{value:.0}")
        } else if abs >= 10.0 {
            format!("{value:.1}")
        } else if abs >= 1.0 {
            format!("{value:.2}")
        } else {
            format!("{value:.3}")
        }
    }

    fn draw_text(img: &mut RgbaImage, x: i32, y: i32, text: &str, color: Rgba<u8>) {
        let mut cursor_x = x;
        for ch in text.chars() {
            if ch == '\n' {
                continue;
            }
            Self::draw_char(img, cursor_x, y, ch, color);
            cursor_x += 6;
        }
    }

    fn draw_text_vertical(img: &mut RgbaImage, x: i32, y: i32, text: &str, color: Rgba<u8>) {
        let mut cursor_y = y;
        for ch in text.chars() {
            if ch == '\n' {
                continue;
            }
            Self::draw_char(img, x, cursor_y, ch, color);
            cursor_y += 8;
        }
    }

    fn draw_char(img: &mut RgbaImage, x: i32, y: i32, ch: char, color: Rgba<u8>) {
        let glyph = match ch {
            '0' => [0x1e, 0x33, 0x35, 0x39, 0x31, 0x33, 0x1e],
            '1' => [0x0c, 0x1c, 0x0c, 0x0c, 0x0c, 0x0c, 0x1e],
            '2' => [0x1e, 0x33, 0x03, 0x06, 0x0c, 0x18, 0x3f],
            '3' => [0x1e, 0x33, 0x03, 0x0e, 0x03, 0x33, 0x1e],
            '4' => [0x06, 0x0e, 0x1e, 0x36, 0x3f, 0x06, 0x06],
            '5' => [0x3f, 0x30, 0x3e, 0x03, 0x03, 0x33, 0x1e],
            '6' => [0x0e, 0x18, 0x30, 0x3e, 0x33, 0x33, 0x1e],
            '7' => [0x3f, 0x03, 0x06, 0x0c, 0x18, 0x18, 0x18],
            '8' => [0x1e, 0x33, 0x33, 0x1e, 0x33, 0x33, 0x1e],
            '9' => [0x1e, 0x33, 0x33, 0x1f, 0x03, 0x06, 0x1c],
            '.' => [0x00, 0x00, 0x00, 0x00, 0x00, 0x0c, 0x0c],
            '-' => [0x00, 0x00, 0x00, 0x1e, 0x00, 0x00, 0x00],
            'E' => [0x3f, 0x30, 0x30, 0x3e, 0x30, 0x30, 0x3f],
            'N' => [0x33, 0x3b, 0x3f, 0x37, 0x33, 0x33, 0x33],
            'R' => [0x3e, 0x33, 0x33, 0x3e, 0x36, 0x33, 0x33],
            'G' => [0x1e, 0x33, 0x30, 0x37, 0x33, 0x33, 0x1e],
            'Y' => [0x33, 0x33, 0x1e, 0x0c, 0x0c, 0x0c, 0x0c],
            'T' => [0x3f, 0x0c, 0x0c, 0x0c, 0x0c, 0x0c, 0x0c],
            'O' => [0x1e, 0x33, 0x33, 0x33, 0x33, 0x33, 0x1e],
            'F' => [0x3f, 0x30, 0x30, 0x3e, 0x30, 0x30, 0x30],
            'M' => [0x33, 0x3f, 0x3f, 0x33, 0x33, 0x33, 0x33],
            'S' => [0x1e, 0x33, 0x30, 0x1e, 0x03, 0x33, 0x1e],
            'C' => [0x1e, 0x33, 0x30, 0x30, 0x30, 0x33, 0x1e],
            'U' => [0x33, 0x33, 0x33, 0x33, 0x33, 0x33, 0x1e],
            'L' => [0x30, 0x30, 0x30, 0x30, 0x30, 0x30, 0x3f],
            'A' => [0x0c, 0x1e, 0x33, 0x33, 0x3f, 0x33, 0x33],
            'D' => [0x3e, 0x33, 0x33, 0x33, 0x33, 0x33, 0x3e],
            'I' => [0x1e, 0x0c, 0x0c, 0x0c, 0x0c, 0x0c, 0x1e],
            'V' => [0x33, 0x33, 0x33, 0x33, 0x33, 0x1e, 0x0c],
            '(' => [0x06, 0x0c, 0x18, 0x18, 0x18, 0x0c, 0x06],
            ')' => [0x18, 0x0c, 0x06, 0x06, 0x06, 0x0c, 0x18],
            _ => [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        };

        let width_i32 = i32::try_from(img.width()).unwrap_or(i32::MAX);
        let height_i32 = i32::try_from(img.height()).unwrap_or(i32::MAX);
        for (row, bits) in glyph.iter().enumerate() {
            let row_i32 = usize_to_i32_saturating(row);
            for col in 0..5 {
                if (bits >> (4 - col)) & 1 == 1 {
                    let px = x + col;
                    let py = y + row_i32;
                    if px >= 0 && py >= 0 && px < width_i32 && py < height_i32 {
                        if let (Ok(xu), Ok(yu)) = (u32::try_from(px), u32::try_from(py)) {
                            img.put_pixel(xu, yu, color);
                        }
                    }
                }
            }
        }
    }

    fn draw_rect_filled(img: &mut RgbaImage, x0: i32, y0: i32, x1: i32, y1: i32, color: Rgba<u8>) {
        let (min_x, max_x) = if x0 <= x1 { (x0, x1) } else { (x1, x0) };
        let (min_y, max_y) = if y0 <= y1 { (y0, y1) } else { (y1, y0) };
        let width_i32 = i32::try_from(img.width()).unwrap_or(i32::MAX);
        let height_i32 = i32::try_from(img.height()).unwrap_or(i32::MAX);
        for y in min_y..=max_y {
            if y < 0 || y >= height_i32 {
                continue;
            }
            for x in min_x..=max_x {
                if x < 0 || x >= width_i32 {
                    continue;
                }
                if let (Ok(xu), Ok(yu)) = (u32::try_from(x), u32::try_from(y)) {
                    img.put_pixel(xu, yu, color);
                }
            }
        }
    }
    fn color32_to_rgba(color: Color32) -> Rgba<u8> {
        Rgba([color.r(), color.g(), color.b(), color.a()])
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
