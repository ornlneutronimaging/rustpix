//! Main application state and logic.
//!
//! Contains the `RustpixApp` struct which manages the GUI state,
//! data, and message handling.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread;
use std::time::Instant;

use eframe::egui;

use crate::histogram::Hyperstack3D;
use crate::message::AppMessage;
use crate::pipeline::{
    load_file_worker, run_clustering_worker, AlgorithmType, ClusteringWorkerConfig,
};
use crate::state::{ProcessingState, Statistics, UiState, ViewMode, ZoomMode};
use crate::util::{f64_to_usize_bounded, usize_to_f64};
use crate::viewer::{generate_histogram_image, Colormap, RoiShape, RoiState};
use rustpix_core::neutron::NeutronBatch;
use rustpix_core::soa::HitBatch;

#[derive(Clone)]
pub(crate) struct RoiSpectrumData {
    pub counts: Vec<u64>,
    pub area: f64,
    pub pixel_count: u64,
}

#[derive(Default)]
struct RoiSpectraCache {
    roi_revision: u64,
    data_revision: u64,
    spectra: HashMap<usize, RoiSpectrumData>,
}

struct RoiSpectrumPending {
    roi_revision: u64,
    data_revision: u64,
    last_change: f64,
}

/// Main application state.
pub struct RustpixApp {
    /// Currently selected file path.
    pub(crate) selected_file: Option<PathBuf>,

    /// Selected clustering algorithm.
    pub(crate) algo_type: AlgorithmType,
    /// Clustering radius parameter.
    pub(crate) radius: f64,
    /// Temporal window in nanoseconds.
    pub(crate) temporal_window_ns: f64,
    /// Minimum cluster size.
    pub(crate) min_cluster_size: u16,
    /// DBSCAN minimum points parameter.
    pub(crate) dbscan_min_points: usize,

    /// Loaded hit batch data.
    pub(crate) hit_batch: Option<HitBatch>,
    /// 3D hyperstack histogram (TOF × Y × X).
    pub(crate) hyperstack: Option<Hyperstack3D>,
    /// Cached 2D projection for visualization (sum over TOF).
    pub(crate) hit_counts: Option<Vec<u64>>,
    /// Cached TOF spectrum (sum over all pixels).
    pub(crate) tof_spectrum: Option<Vec<u64>>,
    /// Extracted neutron events.
    pub(crate) neutrons: NeutronBatch,
    /// 3D hyperstack for neutron data.
    pub(crate) neutron_hyperstack: Option<Hyperstack3D>,
    /// Cached 2D projection for neutron visualization.
    pub(crate) neutron_counts: Option<Vec<u64>>,
    /// Cached TOF spectrum for neutrons.
    pub(crate) neutron_spectrum: Option<Vec<u64>>,
    /// Current cursor info (x, y, hit count).
    pub(crate) cursor_info: Option<(usize, usize, u64)>,

    /// TDC frequency in Hz.
    pub(crate) tdc_frequency: f64,
    /// Flight path length in meters (for energy conversion).
    pub(crate) flight_path_m: f64,
    /// TOF offset in nanoseconds (for energy conversion).
    pub(crate) tof_offset_ns: f64,
    /// TOF bins for hits hyperstack.
    pub(crate) hit_tof_bins: usize,
    /// TOF bins for neutron hyperstack.
    pub(crate) neutron_tof_bins: usize,
    /// Super-resolution factor for clustering extraction.
    pub(crate) super_resolution_factor: f64,
    /// UI display state.
    pub(crate) ui_state: UiState,
    /// ROI session state.
    pub(crate) roi_state: RoiState,
    /// Cached ROI spectra for hits view.
    roi_spectra_hits: RoiSpectraCache,
    /// Cached ROI spectra for neutrons view.
    roi_spectra_neutrons: RoiSpectraCache,
    /// Pending debounce state for ROI spectrum updates.
    roi_spectrum_pending: Option<RoiSpectrumPending>,
    /// Revision counter for hit hyperstack data changes.
    pub(crate) hit_data_revision: u64,
    /// Revision counter for neutron hyperstack data changes.
    pub(crate) neutron_data_revision: u64,

    /// Message receiver for async operations.
    pub(crate) rx: Receiver<AppMessage>,
    /// Message sender for async operations.
    pub(crate) tx: Sender<AppMessage>,

    /// Processing state (loading/clustering progress).
    pub(crate) processing: ProcessingState,
    /// Session statistics.
    pub(crate) statistics: Statistics,

    /// Cached histogram texture.
    pub(crate) texture: Option<egui::TextureHandle>,
    /// Current colormap selection.
    pub(crate) colormap: Colormap,
}

impl Default for RustpixApp {
    fn default() -> Self {
        let (tx, rx) = channel();
        let ui_state = UiState {
            full_fov_visible: true,
            ..Default::default()
        };
        Self {
            selected_file: None,
            algo_type: AlgorithmType::Abs, // Default to ABS per design doc
            radius: 5.0,
            temporal_window_ns: 75.0,
            min_cluster_size: 1,
            dbscan_min_points: 2,

            hit_batch: None,
            hyperstack: None,
            hit_counts: None,
            tof_spectrum: None,
            neutrons: NeutronBatch::default(),
            neutron_hyperstack: None,
            neutron_counts: None,
            neutron_spectrum: None,
            cursor_info: None,

            tdc_frequency: 60.0,
            flight_path_m: 0.0,
            tof_offset_ns: 0.0,
            hit_tof_bins: 200,
            neutron_tof_bins: 200,
            super_resolution_factor: 1.0,
            ui_state,
            roi_state: RoiState::default(),
            roi_spectra_hits: RoiSpectraCache::default(),
            roi_spectra_neutrons: RoiSpectraCache::default(),
            roi_spectrum_pending: None,
            hit_data_revision: 0,
            neutron_data_revision: 0,
            rx,
            tx,

            processing: ProcessingState::default(),
            statistics: Statistics::default(),

            texture: None,
            colormap: Colormap::Grayscale,
        }
    }
}

impl RustpixApp {
    /// Load a file asynchronously.
    pub fn load_file(&mut self, path: PathBuf) {
        self.reset_load_state(path.as_path());

        let tx = self.tx.clone();
        let tdc_frequency = self.tdc_frequency;
        let hit_tof_bins = self.hit_tof_bins;
        thread::spawn(move || load_file_worker(path.as_path(), &tx, tdc_frequency, hit_tof_bins));
    }

    /// Reset application state for a new file load.
    fn reset_load_state(&mut self, path: &Path) {
        self.selected_file = Some(path.to_path_buf());
        self.processing.is_loading = true;
        self.processing.progress = 0.0;
        self.processing.reset_cancel();
        self.processing.status_text.clear();
        self.processing.status_text.push_str("Loading file...");
        self.hit_batch = None;
        self.hyperstack = None;
        self.hit_counts = None;
        self.tof_spectrum = None;
        self.neutrons.clear();
        self.neutron_hyperstack = None;
        self.neutron_counts = None;
        self.neutron_spectrum = None;
        self.ui_state.view_mode = ViewMode::Hits;
        self.ui_state.full_fov_visible = true;
        self.ui_state.show_roi_panel = false;
        self.ui_state.roi_status = None;
        self.ui_state.show_spectrum_range = false;
        self.ui_state.spectrum_x_range = None;
        self.ui_state.spectrum_y_range = None;
        self.ui_state.spectrum_x_min_input.clear();
        self.ui_state.spectrum_x_max_input.clear();
        self.ui_state.spectrum_y_min_input.clear();
        self.ui_state.spectrum_y_max_input.clear();
        self.ui_state.hist_zoom_mode = ZoomMode::None;
        self.ui_state.spectrum_zoom_mode = ZoomMode::None;
        self.ui_state.hist_zoom_start = None;
        self.ui_state.spectrum_zoom_start = None;
        self.ui_state.spectrum_last_plot_bounds = None;
        self.ui_state.spectrum_last_plot_rect = None;
        self.ui_state.roi_rename_id = None;
        self.ui_state.roi_rename_text.clear();
        self.roi_state.clear();
        self.roi_spectra_hits = RoiSpectraCache::default();
        self.roi_spectra_neutrons = RoiSpectraCache::default();
        self.roi_spectrum_pending = None;
        self.hit_data_revision = self.hit_data_revision.wrapping_add(1);
        self.neutron_data_revision = self.neutron_data_revision.wrapping_add(1);
        self.texture = None;
        self.statistics.clear();
    }

    /// Cancel the current loading or processing operation.
    pub fn cancel_operation(&mut self) {
        self.processing.request_cancel();
        if self.processing.is_loading {
            self.processing.is_loading = false;
            self.processing.status_text = "Load cancelled".to_string();
        }
        if self.processing.is_processing {
            self.processing.is_processing = false;
            self.processing.status_text = "Processing cancelled".to_string();
        }
    }

    /// Start clustering processing asynchronously.
    pub fn run_processing(&mut self) {
        if let Some(path) = self.selected_file.clone() {
            self.processing.is_processing = true;
            self.processing.progress = 0.0;
            self.processing.status_text.clear();
            self.processing.status_text.push_str("Clustering...");

            let tx = self.tx.clone();
            let algo_type = self.algo_type;
            let config = ClusteringWorkerConfig {
                radius: self.radius,
                temporal_window_ns: self.temporal_window_ns,
                min_cluster_size: self.min_cluster_size,
                dbscan_min_points: self.dbscan_min_points,
                tdc_frequency: self.tdc_frequency,
                super_resolution_factor: self.super_resolution_factor,
                total_hits: self.hit_batch.as_ref().map_or(0, HitBatch::len),
            };

            thread::spawn(move || run_clustering_worker(&path, &tx, algo_type, &config));
        }
    }

    /// Get the active hyperstack based on view mode.
    fn active_hyperstack(&self) -> Option<&Hyperstack3D> {
        match self.ui_state.view_mode {
            ViewMode::Hits => self.hyperstack.as_ref(),
            ViewMode::Neutrons => self.neutron_hyperstack.as_ref(),
        }
    }

    /// Get the active 2D projection based on view mode.
    fn active_counts(&self) -> Option<&[u64]> {
        match self.ui_state.view_mode {
            ViewMode::Hits => self.hit_counts.as_deref(),
            ViewMode::Neutrons => self.neutron_counts.as_deref(),
        }
    }

    /// Generate histogram image from current view (hits or neutrons).
    pub fn generate_histogram(&self) -> egui::ColorImage {
        let counts = if self.ui_state.slicer_enabled {
            // Get current TOF slice from active hyperstack
            self.active_hyperstack()
                .and_then(|hs| hs.slice_tof(self.ui_state.current_tof_bin))
        } else {
            // Full projection
            self.active_counts()
        };

        let Some(counts) = counts else {
            return egui::ColorImage::new([512, 512], egui::Color32::BLACK);
        };
        generate_histogram_image(counts, self.colormap, self.ui_state.log_scale)
    }

    /// Get the cached TOF spectrum (full detector integration).
    pub fn tof_spectrum(&self) -> Option<&[u64]> {
        match self.ui_state.view_mode {
            ViewMode::Hits => self.tof_spectrum.as_deref(),
            ViewMode::Neutrons => self.neutron_spectrum.as_deref(),
        }
    }

    /// Get the number of TOF bins in the active hyperstack.
    pub fn n_tof_bins(&self) -> usize {
        self.active_hyperstack()
            .map_or(0, super::histogram::Hyperstack3D::n_tof_bins)
    }

    /// Get counts for current view (projection or slice).
    pub fn current_counts(&self) -> Option<&[u64]> {
        if self.ui_state.slicer_enabled {
            self.active_hyperstack()
                .and_then(|hs| hs.slice_tof(self.ui_state.current_tof_bin))
        } else {
            self.active_counts()
        }
    }

    fn active_data_revision(&self) -> u64 {
        match self.ui_state.view_mode {
            ViewMode::Hits => self.hit_data_revision,
            ViewMode::Neutrons => self.neutron_data_revision,
        }
    }

    fn active_roi_cache_mut(&mut self) -> &mut RoiSpectraCache {
        match self.ui_state.view_mode {
            ViewMode::Hits => &mut self.roi_spectra_hits,
            ViewMode::Neutrons => &mut self.roi_spectra_neutrons,
        }
    }

    fn active_roi_cache(&self) -> &RoiSpectraCache {
        match self.ui_state.view_mode {
            ViewMode::Hits => &self.roi_spectra_hits,
            ViewMode::Neutrons => &self.roi_spectra_neutrons,
        }
    }

    pub(crate) fn roi_spectrum_data(&self, roi_id: usize) -> Option<&RoiSpectrumData> {
        self.active_roi_cache().spectra.get(&roi_id)
    }

    pub(crate) fn roi_spectra_map(&self) -> &HashMap<usize, RoiSpectrumData> {
        &self.active_roi_cache().spectra
    }

    pub(crate) fn update_roi_spectra(&mut self, ctx: &egui::Context) {
        let roi_revision = self.roi_state.revision();
        let data_revision = self.active_data_revision();
        let needs_update = {
            let cache = self.active_roi_cache();
            cache.roi_revision != roi_revision || cache.data_revision != data_revision
        };
        if !needs_update {
            self.roi_spectrum_pending = None;
            return;
        }

        let debounce = self.roi_state.debounce_updates;
        if debounce {
            let now = ctx.input(|i| i.time);
            match self.roi_spectrum_pending.as_mut() {
                Some(pending) => {
                    if pending.roi_revision != roi_revision
                        || pending.data_revision != data_revision
                    {
                        pending.roi_revision = roi_revision;
                        pending.data_revision = data_revision;
                        pending.last_change = now;
                    }
                }
                None => {
                    self.roi_spectrum_pending = Some(RoiSpectrumPending {
                        roi_revision,
                        data_revision,
                        last_change: now,
                    });
                }
            }

            if let Some(pending) = &self.roi_spectrum_pending {
                if now - pending.last_change < 0.25 {
                    ctx.request_repaint();
                    return;
                }
            }
        }

        let start = Instant::now();
        let spectra = self.compute_roi_spectra();
        let elapsed = start.elapsed().as_secs_f64();
        if elapsed > 0.05 {
            let expires_at = ctx.input(|i| i.time) + 1.5;
            self.ui_state.roi_status = Some(("Computing spectrum...".to_string(), expires_at));
            ctx.request_repaint();
        }
        let cache = self.active_roi_cache_mut();
        cache.spectra = spectra;
        cache.roi_revision = roi_revision;
        cache.data_revision = data_revision;
        self.roi_spectrum_pending = None;
    }

    pub(crate) fn force_roi_spectra_update(&mut self) {
        let roi_revision = self.roi_state.revision();
        let data_revision = self.active_data_revision();
        let needs_update = {
            let cache = self.active_roi_cache();
            cache.roi_revision != roi_revision || cache.data_revision != data_revision
        };
        if !needs_update {
            return;
        }
        let spectra = self.compute_roi_spectra();
        let cache = self.active_roi_cache_mut();
        cache.spectra = spectra;
        cache.roi_revision = roi_revision;
        cache.data_revision = data_revision;
        self.roi_spectrum_pending = None;
    }

    fn compute_roi_spectra(&self) -> HashMap<usize, RoiSpectrumData> {
        let Some(hyperstack) = self.active_hyperstack() else {
            return HashMap::new();
        };
        let width = hyperstack.width();
        let height = hyperstack.height();
        let n_bins = hyperstack.n_tof_bins();
        let mut spectra = HashMap::new();

        for roi in &self.roi_state.rois {
            match &roi.shape {
                RoiShape::Rectangle { x1, y1, x2, y2 } => {
                    let (x_start, x_end) = clamp_span(*x1, *x2, width);
                    let (y_start, y_end) = clamp_span(*y1, *y2, height);
                    let counts = if x_start < x_end && y_start < y_end {
                        hyperstack.spectrum(x_start..x_end, y_start..y_end)
                    } else {
                        vec![0; n_bins]
                    };
                    let area = (x2 - x1).abs() * (y2 - y1).abs();
                    let pixel_count = (x_end - x_start) as u64 * (y_end - y_start) as u64;
                    spectra.insert(
                        roi.id,
                        RoiSpectrumData {
                            counts,
                            area,
                            pixel_count,
                        },
                    );
                }
                RoiShape::Polygon { vertices } => {
                    if vertices.len() < 3 {
                        continue;
                    }
                    let (mut min_x, mut max_x) = (f64::INFINITY, f64::NEG_INFINITY);
                    let (mut min_y, mut max_y) = (f64::INFINITY, f64::NEG_INFINITY);
                    for (x, y) in vertices {
                        min_x = min_x.min(*x);
                        max_x = max_x.max(*x);
                        min_y = min_y.min(*y);
                        max_y = max_y.max(*y);
                    }
                    let (x_start, x_end) = clamp_span(min_x, max_x, width);
                    let (y_start, y_end) = clamp_span(min_y, max_y, height);
                    let mut mask = Vec::new();
                    for y in y_start..y_end {
                        let py = usize_to_f64(y) + 0.5;
                        for x in x_start..x_end {
                            let px = usize_to_f64(x) + 0.5;
                            if point_in_polygon_xy(px, py, vertices) {
                                mask.push(y * width + x);
                            }
                        }
                    }
                    let pixel_count = mask.len() as u64;
                    let mut counts = vec![0; n_bins];
                    for (tof_bin, count) in counts.iter_mut().enumerate() {
                        let Some(slice) = hyperstack.slice_tof(tof_bin) else {
                            continue;
                        };
                        let mut sum = 0u64;
                        for idx in &mask {
                            sum += slice[*idx];
                        }
                        *count = sum;
                    }
                    let area = polygon_area(vertices).abs();
                    spectra.insert(
                        roi.id,
                        RoiSpectrumData {
                            counts,
                            area,
                            pixel_count,
                        },
                    );
                }
            }
        }

        spectra
    }

    /// Check if neutron data is available.
    pub fn has_neutrons(&self) -> bool {
        self.neutron_hyperstack.is_some()
    }

    /// Rebuild the hits hyperstack with current settings.
    pub fn rebuild_hit_hyperstack(&mut self) {
        let Some(hit_batch) = &self.hit_batch else {
            return;
        };
        let tof_max = self.statistics.tof_max;
        let bins = self.hit_tof_bins.max(1);
        let hyperstack = Hyperstack3D::from_hits(hit_batch, bins, tof_max, 512, 512);
        self.hit_counts = Some(hyperstack.project_xy());
        self.tof_spectrum = Some(hyperstack.full_spectrum());
        self.hyperstack = Some(hyperstack);
        self.hit_data_revision = self.hit_data_revision.wrapping_add(1);
        self.ui_state.current_tof_bin = 0;
        self.ui_state.needs_plot_reset = true;
        self.texture = None;
    }

    /// Rebuild the neutron hyperstack with current settings.
    pub fn rebuild_neutron_hyperstack(&mut self) {
        if self.neutrons.is_empty() {
            return;
        }
        let tof_max = self.statistics.tof_max;
        let bins = self.neutron_tof_bins.max(1);
        let neutron_hs = Hyperstack3D::from_neutrons(
            &self.neutrons,
            bins,
            tof_max,
            512,
            512,
            self.super_resolution_factor,
        );
        self.neutron_counts = Some(neutron_hs.project_xy());
        self.neutron_spectrum = Some(neutron_hs.full_spectrum());
        self.neutron_hyperstack = Some(neutron_hs);
        self.neutron_data_revision = self.neutron_data_revision.wrapping_add(1);
        self.ui_state.current_tof_bin = 0;
        self.ui_state.needs_plot_reset = true;
        self.texture = None;
    }

    /// Handle pending messages from async workers.
    pub fn handle_messages(&mut self, ctx: &egui::Context) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                AppMessage::LoadProgress(p, s) | AppMessage::ProcessingProgress(p, s) => {
                    self.processing.progress = p;
                    self.processing.status_text = s;
                }
                AppMessage::LoadComplete(batch, hyperstack, dur, _dbg) => {
                    self.processing.is_loading = false;
                    self.processing.progress = 1.0;
                    self.processing.status_text = "Ready".to_string();

                    // Update statistics
                    self.statistics.hit_count = batch.len();
                    self.statistics.load_duration = Some(dur);
                    self.statistics.tof_max = hyperstack.tof_max();

                    // Cache projections for visualization
                    self.hit_counts = Some(hyperstack.project_xy());
                    self.tof_spectrum = Some(hyperstack.full_spectrum());
                    self.hyperstack = Some(*hyperstack);
                    self.hit_batch = Some(*batch);
                    self.hit_data_revision = self.hit_data_revision.wrapping_add(1);

                    // Trigger auto-fit of plot view
                    self.ui_state.needs_plot_reset = true;

                    let img = self.generate_histogram();
                    self.texture =
                        Some(ctx.load_texture("hist", img, egui::TextureOptions::NEAREST));
                }
                AppMessage::LoadError(e) => {
                    self.processing.is_loading = false;
                    self.processing.status_text = format!("Error: {e}");
                }
                AppMessage::ProcessingComplete(neutrons, dur) => {
                    self.processing.is_processing = false;
                    self.processing.progress = 1.0;
                    self.processing.status_text = "Ready".to_string();

                    // Update statistics
                    self.statistics.neutron_count = neutrons.len();
                    self.statistics.cluster_duration = Some(dur);
                    if !neutrons.is_empty() && self.statistics.hit_count > 0 {
                        #[allow(clippy::cast_precision_loss)]
                        {
                            self.statistics.avg_cluster_size =
                                self.statistics.hit_count as f64 / neutrons.len() as f64;
                        }
                    }

                    // Build neutron hyperstack using same TOF parameters as hits
                    if let Some(hit_hs) = &self.hyperstack {
                        let neutron_hs = Hyperstack3D::from_neutrons(
                            &neutrons,
                            self.neutron_tof_bins.max(1),
                            hit_hs.tof_max(),
                            hit_hs.width(),
                            hit_hs.height(),
                            self.super_resolution_factor,
                        );
                        self.neutron_counts = Some(neutron_hs.project_xy());
                        self.neutron_spectrum = Some(neutron_hs.full_spectrum());
                        self.neutron_hyperstack = Some(neutron_hs);
                        self.neutron_data_revision = self.neutron_data_revision.wrapping_add(1);
                    }

                    self.neutrons = neutrons;
                }
                AppMessage::ProcessingError(e) => {
                    self.processing.is_processing = false;
                    self.processing.status_text = format!("Error: {e}");
                }
            }
        }
    }
}

fn clamp_span(a: f64, b: f64, limit: usize) -> (usize, usize) {
    let min = a.min(b);
    let max = a.max(b);
    let max_f64 = usize_to_f64(limit);
    let start_f64 = min.floor().clamp(0.0, max_f64);
    let end_f64 = max.ceil().clamp(0.0, max_f64);
    let max_exclusive = limit.saturating_add(1);
    let start = f64_to_usize_bounded(start_f64, max_exclusive).unwrap_or(limit);
    let end = f64_to_usize_bounded(end_f64, max_exclusive).unwrap_or(limit);
    (start, end)
}

fn polygon_area(vertices: &[(f64, f64)]) -> f64 {
    if vertices.len() < 3 {
        return 0.0;
    }
    let mut area2 = 0.0;
    for i in 0..vertices.len() {
        let (x0, y0) = vertices[i];
        let (x1, y1) = vertices[(i + 1) % vertices.len()];
        area2 += (x0 * y1) - (x1 * y0);
    }
    area2 * 0.5
}

fn point_in_polygon_xy(x: f64, y: f64, vertices: &[(f64, f64)]) -> bool {
    if vertices.len() < 3 {
        return false;
    }
    let mut inside = false;
    let mut j = vertices.len() - 1;
    for i in 0..vertices.len() {
        let (xi, yi) = vertices[i];
        let (xj, yj) = vertices[j];
        let intersects = ((yi > y) != (yj > y)) && (x < (xj - xi) * (y - yi) / (yj - yi) + xi);
        if intersects {
            inside = !inside;
        }
        j = i;
    }
    inside
}

impl eframe::App for RustpixApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Apply system theme (follows system light/dark preference)
        crate::ui::theme::apply_system_theme(ctx);

        self.handle_messages(ctx);

        // Render panels in order: top, bottom, side, central
        self.render_top_panel(ctx);
        self.render_bottom_panel(ctx);
        self.render_side_panel(ctx);
        self.render_central_panel(ctx);
        self.render_settings_windows(ctx);

        if self.processing.is_loading || self.processing.is_processing {
            ctx.request_repaint();
        }
    }
}
