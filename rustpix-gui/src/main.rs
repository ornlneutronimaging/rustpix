#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use eframe::egui;
use egui_plot::{Bar, BarChart, Plot, PlotImage, PlotPoint};
use rfd::FileDialog;
use std::collections::BinaryHeap;
use std::fmt::Write;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, sync_channel, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant};

// Logic Imports
use rustpix_algorithms::{cluster_and_extract_batch, AlgorithmParams, ClusteringAlgorithm};

use rustpix_core::clustering::ClusteringConfig;
use rustpix_core::extraction::ExtractionConfig;
use rustpix_core::neutron::NeutronBatch;
use rustpix_core::soa::HitBatch;
use rustpix_io::scanner::PacketScanner;
use rustpix_io::Tpx3FileReader;
use rustpix_tpx::ordering::{PulseBatch, PulseReader};
use rustpix_tpx::ChipTransform;
use rustpix_tpx::section::{scan_section_tdc, Tpx3Section};
use rustpix_tpx::DetectorConfig;

// We probably need to re-export MappedFileReader or use Tpx3FileReader internals?
// rustpix-io exports `PacketScanner` and `Mmap...` if public.
// I'll check imports later.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AlgorithmType {
    Abs,
    Dbscan,
    Grid,
}

impl std::fmt::Display for AlgorithmType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AlgorithmType::Abs => write!(f, "ABS (Age-Based Spatial)"),
            AlgorithmType::Dbscan => write!(f, "DBSCAN"),
            AlgorithmType::Grid => write!(f, "Grid (Spatial Partition)"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Colormap {
    Green,
    Hot,
    Grayscale,
    Viridis,
}

impl std::fmt::Display for Colormap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Colormap::Green => write!(f, "Green (Matrix)"),
            Colormap::Hot => write!(f, "Hot (Thermal)"),
            Colormap::Grayscale => write!(f, "Grayscale"),
            Colormap::Viridis => write!(f, "Viridis"),
        }
    }
}

enum AppMessage {
    LoadProgress(f32, String), // Progress, Status text
    LoadComplete(Box<HitBatch>, Vec<u32>, Vec<u64>, Duration, String), // batch, counts, hist, dur, debug_info
    LoadError(String),

    ProcessingProgress(f32, String),
    ProcessingComplete(NeutronBatch, Duration),
    ProcessingError(String),
}

#[derive(Default)]
struct UiState {
    log_plot: bool,
    show_histogram: bool,
}

struct ProcessingState {
    is_loading: bool,
    is_processing: bool,
    progress: f32,
    status_text: String,
}

impl Default for ProcessingState {
    fn default() -> Self {
        Self {
            is_loading: false,
            is_processing: false,
            progress: 0.0,
            status_text: "Ready".to_string(),
        }
    }
}

struct RustpixApp {
    selected_file: Option<PathBuf>,

    // UI State
    algo_type: AlgorithmType,
    // Config parameters
    radius: f64,
    temporal_window_ns: f64,
    min_cluster_size: u16,
    dbscan_min_points: usize,

    // Data
    hit_batch: Option<HitBatch>,
    hit_counts: Option<Vec<u32>>, // 512x512 grid for visualization/tooltips
    neutrons: NeutronBatch,
    tof_hist_full: Option<Vec<u64>>, // Cached histogram
    // Cursor Info: x, y, hits
    cursor_info: Option<(usize, usize, u32)>,

    // Config
    tdc_frequency: f64,
    ui_state: UiState,

    // App Logic
    rx: Receiver<AppMessage>,
    tx: Sender<AppMessage>,

    processing: ProcessingState,

    // Stats - Removed as per original code edit, but not explicitly requested to remove.
    // load_time: Option<Duration>,
    // proc_time: Option<Duration>,

    // Vis
    texture: Option<egui::TextureHandle>,
    // histogram_bins: usize, // Fixed to 512 for now matching sensor

    // Visualization
    colormap: Colormap,
}

impl Default for RustpixApp {
    fn default() -> Self {
        let (tx, rx) = channel();
        Self {
            selected_file: None,
            algo_type: AlgorithmType::Grid, // Default to Grid (Fastest)
            radius: 5.0,
            temporal_window_ns: 75.0,
            min_cluster_size: 1,
            dbscan_min_points: 2,

            hit_batch: None,
            hit_counts: None,
            neutrons: NeutronBatch::default(),
            tof_hist_full: None,
            cursor_info: None,
            // debug_info removed

            // Config
            tdc_frequency: 60.0,
            ui_state: UiState::default(),
            rx,
            tx,

            processing: ProcessingState::default(),

            // load_time: None, // Removed
            // proc_time: None, // Removed
            texture: None,
            // histogram_bins: 512,
            colormap: Colormap::Green,
        }
    }
}

impl RustpixApp {
    fn load_file(&mut self, path: PathBuf) {
        self.reset_load_state(path.as_path());

        let tx = self.tx.clone();
        let tdc_frequency = self.tdc_frequency;
        thread::spawn(move || load_file_worker(path.as_path(), &tx, tdc_frequency));
    }

    fn reset_load_state(&mut self, path: &Path) {
        self.selected_file = Some(path.to_path_buf());
        self.processing.is_loading = true;
        self.processing.progress = 0.0;
        self.processing.status_text.clear();
        self.processing.status_text.push_str("Loading file...");
        self.hit_batch = None;
        self.hit_counts = None;
        self.neutrons.clear();
        self.texture = None;
        self.tof_hist_full = None;
    }

    fn run_processing(&mut self) {
        if let Some(path) = self.selected_file.clone() {
            self.processing.is_processing = true;
            self.processing.progress = 0.0;
            self.processing.status_text.clear();
            self.processing.status_text.push_str("Clustering...");

            let tx = self.tx.clone();
            let algo_type = self.algo_type;
            let config = (
                self.radius,
                self.temporal_window_ns,
                self.min_cluster_size,
                self.dbscan_min_points,
            );
            let total_hits = self.hit_batch.as_ref().map_or(0, HitBatch::len);
            let tdc_frequency = self.tdc_frequency;

            thread::spawn(move || {
                let start = Instant::now();
                let (radius, window, min_size, min_points) = config;

                let det_config = DetectorConfig {
                    tdc_frequency_hz: tdc_frequency,
                    ..DetectorConfig::venus_defaults()
                };

                let reader = match Tpx3FileReader::open(&path) {
                    Ok(r) => r.with_config(det_config),
                    Err(e) => {
                        let _ = tx.send(AppMessage::ProcessingError(e.to_string()));
                        return;
                    }
                };

                let algo = match algo_type {
                    AlgorithmType::Abs => ClusteringAlgorithm::Abs,
                    AlgorithmType::Dbscan => ClusteringAlgorithm::Dbscan,
                    AlgorithmType::Grid => ClusteringAlgorithm::Grid,
                };

                let clustering = ClusteringConfig {
                    radius,
                    temporal_window_ns: window,
                    min_cluster_size: min_size,
                    max_cluster_size: None,
                };

                let params = AlgorithmParams {
                    abs_scan_interval: 100,
                    dbscan_min_points: min_points,
                    grid_cell_size: 32,
                };

                // Preserve legacy GUI behavior: naive centroid, no TOT filtering, no super-res.
                let extraction = ExtractionConfig {
                    super_resolution_factor: 1.0,
                    weighted_by_tot: false,
                    min_tot_threshold: 0,
                };

                let stream = match reader.stream_time_ordered() {
                    Ok(s) => s,
                    Err(e) => {
                        let _ = tx.send(AppMessage::ProcessingError(e.to_string()));
                        return;
                    }
                };

                let mut processed_hits = 0usize;
                let mut last_update = Instant::now();
                let mut neutrons = NeutronBatch::default();

                for mut batch in stream {
                    processed_hits = processed_hits.saturating_add(batch.len());
                    let res = cluster_and_extract_batch(
                        &mut batch,
                        algo,
                        &clustering,
                        &extraction,
                        &params,
                    );

                    match res {
                        Ok(n) => neutrons.append(&n),
                        Err(e) => {
                            let _ = tx.send(AppMessage::ProcessingError(e.to_string()));
                            return;
                        }
                    }

                    if total_hits > 0 && last_update.elapsed() > Duration::from_millis(200) {
                        let progress =
                            (usize_to_f32(processed_hits) / usize_to_f32(total_hits)).min(0.95);
                        let _ = tx.send(AppMessage::ProcessingProgress(
                            progress,
                            format!("Processing... {:.0}%", progress * 100.0),
                        ));
                        last_update = Instant::now();
                    }
                }

                tx.send(AppMessage::ProcessingComplete(neutrons, start.elapsed()))
                    .unwrap();
            }); // Close thread::spawn
        } // Close if let Some(batch)
    } // Close run_processing

    fn generate_histogram(&self) -> egui::ColorImage {
        let Some(counts) = &self.hit_counts else {
            return egui::ColorImage::new([512, 512], egui::Color32::BLACK);
        };

        // Find max for scaling
        let max_count = u32_to_f32(counts.iter().max().copied().unwrap_or(1));
        let mut pixels = Vec::with_capacity(512 * 512 * 4);

        for &count in counts {
            let val = (u32_to_f32(count) / max_count).sqrt(); // Sqrt scale
            let v = f32_to_u8(val * 255.0);
            if count == 0 {
                pixels.extend_from_slice(&[0, 0, 0, 255]);
            } else {
                match self.colormap {
                    Colormap::Green => pixels.extend_from_slice(&[0, v, 0, 255]),
                    Colormap::Grayscale => pixels.extend_from_slice(&[v, v, v, 255]),
                    Colormap::Hot => {
                        // Simple Red-Yellow-White heatmap
                        if val < 0.5 {
                            // Red to Yellow
                            let r = 255;
                            let g = f32_to_u8(val * 2.0 * 255.0);
                            pixels.extend_from_slice(&[r, g, 0, 255]);
                        } else {
                            // Yellow to White
                            let r = 255;
                            let g = 255;
                            let b = f32_to_u8((val - 0.5) * 2.0 * 255.0);
                            pixels.extend_from_slice(&[r, g, b, 255]);
                        }
                    }
                    Colormap::Viridis => {
                        // Approximate Viridis (Blue -> Teal -> Green -> Yellow)
                        let r = f32_to_u8(255.0 * val.powf(2.0));
                        let g = f32_to_u8(255.0 * val);
                        let b = f32_to_u8(255.0 * (1.0 - val));
                        pixels.extend_from_slice(&[r, g, b, 255]);
                    }
                }
            }
        }

        egui::ColorImage::from_rgba_unmultiplied([512, 512], &pixels)
    }

    fn handle_messages(&mut self, ctx: &egui::Context) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                AppMessage::LoadProgress(p, s) | AppMessage::ProcessingProgress(p, s) => {
                    self.processing.progress = p;
                    self.processing.status_text = s;
                }
                AppMessage::LoadComplete(batch, counts, hist, dur, _dbg) => {
                    self.processing.is_loading = false;
                    self.processing.progress = 1.0;
                    self.processing.status_text =
                        format!("Loaded {} hits in {:.2}s", batch.len(), dur.as_secs_f64());

                    self.hit_counts = Some(counts);
                    self.tof_hist_full = Some(hist);
                    self.hit_batch = Some(*batch);

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
                    self.processing.status_text = format!(
                        "Found {} neutrons in {:.2}ms",
                        neutrons.len(),
                        dur.as_secs_f64() * 1000.0
                    );
                    self.neutrons = neutrons;
                }
                AppMessage::ProcessingError(e) => {
                    self.processing.is_processing = false;
                    self.processing.status_text = format!("Error: {e}");
                }
            }
        }
    }

    fn render_side_panel(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("ctrl").show(ctx, |ui| {
            ui.heading("Rustpix GUI");
            ui.separator();

            self.render_file_controls(ui);
            ui.separator();

            self.render_status(ui);
            ui.separator();

            self.render_cursor_info(ui);
            ui.separator();

            self.render_visualization_controls(ctx, ui);
            ui.separator();

            self.render_processing_controls(ui);
            ui.separator();

            let neutron_count = self.neutrons.len();
            ui.label(format!("Neutrons: {neutron_count}"));
        });
    }

    fn render_file_controls(&mut self, ui: &mut egui::Ui) {
        if ui.button("Open File").clicked() && !self.processing.is_loading {
            if let Some(path) = FileDialog::new().add_filter("TPX3", &["tpx3"]).pick_file() {
                self.load_file(path);
            }
        }
        if let Some(p) = &self.selected_file {
            ui.label(p.file_name().unwrap_or_default().to_string_lossy());
        }
    }

    fn render_status(&self, ui: &mut egui::Ui) {
        if self.processing.is_loading {
            ui.add(
                egui::ProgressBar::new(self.processing.progress).text(&self.processing.status_text),
            );
        } else {
            ui.label(&self.processing.status_text);
        }
    }

    fn render_cursor_info(&self, ui: &mut egui::Ui) {
        if let Some((x, y, count)) = self.cursor_info {
            ui.label(egui::RichText::new("Pixel Info:").strong());
            ui.label(format!("X: {x}\nY: {y}\nHits: {count}"));
        } else {
            ui.label("Hover over image for details.");
        }
    }

    fn render_visualization_controls(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        ui.heading("Visualization");
        egui::ComboBox::from_label("Color")
            .selected_text(self.colormap.to_string())
            .show_ui(ui, |ui| {
                if ui
                    .selectable_value(&mut self.colormap, Colormap::Green, "Green")
                    .clicked()
                {
                    self.texture = None;
                }
                if ui
                    .selectable_value(&mut self.colormap, Colormap::Hot, "Hot")
                    .clicked()
                {
                    self.texture = None;
                }
                if ui
                    .selectable_value(&mut self.colormap, Colormap::Grayscale, "Gray")
                    .clicked()
                {
                    self.texture = None;
                }
                if ui
                    .selectable_value(&mut self.colormap, Colormap::Viridis, "Viridis")
                    .clicked()
                {
                    self.texture = None;
                }
            });
        if self.texture.is_none() && self.hit_counts.is_some() {
            let img = self.generate_histogram();
            self.texture = Some(ctx.load_texture("hist", img, egui::TextureOptions::NEAREST));
        }

        ui.toggle_value(&mut self.ui_state.show_histogram, "Show TOF Histogram");
    }

    fn render_processing_controls(&mut self, ui: &mut egui::Ui) {
        ui.heading("Processing");
        ui.add(
            egui::DragValue::new(&mut self.tdc_frequency)
                .speed(0.1)
                .range(1.0..=120.0)
                .prefix("TDC (Hz): "),
        );

        egui::ComboBox::from_label("Algo")
            .selected_text(self.algo_type.to_string())
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut self.algo_type, AlgorithmType::Grid, "Grid");
                ui.selectable_value(&mut self.algo_type, AlgorithmType::Abs, "ABS");
                ui.selectable_value(&mut self.algo_type, AlgorithmType::Dbscan, "DBSCAN");
            });

        ui.add(egui::Slider::new(&mut self.radius, 1.0..=50.0).text("Radius"));
        ui.add(
            egui::Slider::new(&mut self.temporal_window_ns, 10.0..=500.0).text("Time Window (ns)"),
        );
        ui.add(egui::Slider::new(&mut self.min_cluster_size, 1..=10).text("Min Cluster"));

        if self.algo_type == AlgorithmType::Dbscan {
            ui.add(egui::Slider::new(&mut self.dbscan_min_points, 1..=10).text("Min Points"));
        }

        if ui
            .add_enabled(
                !self.processing.is_processing && self.hit_batch.is_some(),
                egui::Button::new("Run Clustering"),
            )
            .clicked()
        {
            self.run_processing();
        }

        if self.processing.is_processing {
            ui.add(
                egui::ProgressBar::new(self.processing.progress).text(&self.processing.status_text),
            );
        }
    }

    fn render_central_panel(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            if let Some(tex) = &self.texture {
                Plot::new("plot").data_aspect(1.0).show(ui, |plot_ui| {
                    plot_ui.image(PlotImage::new(
                        tex,
                        PlotPoint::new(256.0, 256.0),
                        [512.0, 512.0],
                    ));

                    if let Some(curr) = plot_ui.pointer_coordinate() {
                        let x = curr.x;
                        let y = curr.y;
                        if x >= 0.0 && y >= 0.0 && x < 512.0 && y < 512.0 {
                            let (Some(xi), Some(yi)) =
                                (f64_to_usize_bounded(x, 512), f64_to_usize_bounded(y, 512))
                            else {
                                self.cursor_info = None;
                                return;
                            };
                            let count = if let Some(counts) = &self.hit_counts {
                                counts[yi * 512 + xi]
                            } else {
                                0
                            };
                            self.cursor_info = Some((xi, yi, count));
                        } else {
                            self.cursor_info = None;
                        }
                    } else {
                        self.cursor_info = None;
                    }
                });
            } else {
                ui.centered_and_justified(|ui| ui.label("No Data"));
            }
        });
    }

    fn render_histogram_window(&mut self, ctx: &egui::Context) {
        if !self.ui_state.show_histogram {
            return;
        }

        egui::Window::new("TOF Histogram").show(ctx, |ui| {
            if let Some(full) = &self.tof_hist_full {
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
                    });
            } else {
                ui.label("No Data");
            }
        });
    }
}

fn load_file_worker(path: &Path, tx: &Sender<AppMessage>, tdc_frequency: f64) {
    let start = Instant::now();
    let file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(e) => {
            let _ = tx.send(AppMessage::LoadError(e.to_string()));
            return;
        }
    };

    // SAFETY: The file is opened read-only and we assume it is not modified concurrently.
    #[allow(unsafe_code)]
    let mmap = unsafe {
        match memmap2::Mmap::map(&file) {
            Ok(m) => m,
            Err(e) => {
                let _ = tx.send(AppMessage::LoadError(e.to_string()));
                return;
            }
        }
    };

    let _ = tx.send(AppMessage::LoadProgress(
        0.1,
        "Scanning sections...".to_string(),
    ));

    let io_sections = scan_sections_with_progress(&mmap, tx);
    let total_sections = io_sections.len();
    let _ = tx.send(AppMessage::LoadProgress(
        0.15,
        format!("Found {total_sections} sections. Prescanning TDCs..."),
    ));

    let tpx_sections = build_tpx_sections(&mmap, io_sections);

    let mut det_config = DetectorConfig::venus_defaults();
    det_config.tdc_frequency_hz = tdc_frequency;
    let tdc_correction = det_config.tdc_correction_25ns();
    let debug_str = build_debug_info(&mmap, &tpx_sections, tdc_correction);

    let _ = tx.send(AppMessage::LoadProgress(
        0.25,
        "Processing hits...".to_string(),
    ));

    let full_batch = process_sections_to_batch(&mmap, &tpx_sections, &det_config, tx);

    let _ = tx.send(AppMessage::LoadProgress(
        0.95,
        "Generating visualization data...".to_string(),
    ));
    let (counts, hist) = compute_counts_and_hist(&full_batch, tdc_correction);

    let _ = tx.send(AppMessage::LoadComplete(
        Box::new(full_batch),
        counts,
        hist,
        start.elapsed(),
        debug_str,
    ));
}

fn scan_sections_with_progress(
    mmap: &memmap2::Mmap,
    tx: &Sender<AppMessage>,
) -> Vec<rustpix_io::scanner::Section> {
    let mut io_sections = Vec::new();
    let mut offset = 0;
    let chunk_size = 50 * 1024 * 1024; // 50MB chunks
    let total_bytes = mmap.len().max(1);

    while offset < total_bytes {
        let end = (offset + chunk_size).min(total_bytes);
        let is_eof = end == total_bytes;
        let data = &mmap[offset..end];

        let (sections, consumed) = PacketScanner::scan_sections(data, is_eof);
        for mut section in sections {
            section.start_offset += offset;
            section.end_offset += offset;
            io_sections.push(section);
        }

        offset = offset.saturating_add(consumed);

        let ratio = usize_to_f32(offset) / usize_to_f32(total_bytes);
        let _ = tx.send(AppMessage::LoadProgress(
            0.15 * ratio,
            format!("Scanning sections... {:.0}%", ratio * 100.0),
        ));

        if consumed == 0 && !is_eof {
            offset = offset.saturating_add(chunk_size);
        }
    }

    io_sections
}

fn build_tpx_sections(
    mmap: &memmap2::Mmap,
    io_sections: Vec<rustpix_io::scanner::Section>,
) -> Vec<Tpx3Section> {
    let mut tpx_sections = Vec::with_capacity(io_sections.len());
    let mut chip_tdc_state = [None; 256];

    for section in io_sections {
        let initial = chip_tdc_state[usize::from(section.chip_id)];
        let mut rules = Tpx3Section {
            start_offset: section.start_offset,
            end_offset: section.end_offset,
            chip_id: section.chip_id,
            initial_tdc: initial,
            final_tdc: None,
        };

        if let Some(final_t) = scan_section_tdc(mmap, &rules) {
            rules.final_tdc = Some(final_t);
            chip_tdc_state[usize::from(section.chip_id)] = Some(final_t);
        }

        tpx_sections.push(rules);
    }

    tpx_sections
}

fn build_debug_info(mmap: &memmap2::Mmap, sections: &[Tpx3Section], tdc_correction: u32) -> String {
    let mut debug_str = String::new();
    let _ = writeln!(debug_str, "TDC Correction (25ns): {tdc_correction}");

    if let Some(sec) = sections.iter().find(|s| s.initial_tdc.is_some()) {
        if let Some(tdc) = sec.initial_tdc {
            let _ = writeln!(debug_str, "Sec TDC Ref: {tdc}");
            let sdata = &mmap[sec.start_offset..sec.end_offset];
            let mut found = false;
            for ch in sdata.chunks_exact(8) {
                let raw = u64::from_le_bytes(ch.try_into().unwrap());
                let packet = rustpix_tpx::Tpx3Packet::new(raw);
                if packet.is_hit() {
                    let raw_ts = packet.timestamp_coarse();
                    let ts = rustpix_tpx::correct_timestamp_rollover(raw_ts, tdc);
                    let raw_tof = ts.wrapping_sub(tdc);
                    let tof = rustpix_tpx::calculate_tof(ts, tdc, tdc_correction);
                    let _ = writeln!(
                        debug_str,
                        "Sample Hit:\n  RawTS: {raw_ts}\n  CorrTS: {ts}\n  RawDelta: {raw_tof}\n  CalcTOF: {tof}"
                    );
                    found = true;
                    break;
                }
            }
            if !found {
                let _ = writeln!(debug_str, "Section has no hits.");
            }
        }
    } else {
        let _ = writeln!(debug_str, "No sections with valid Initial TDC found.");
    }

    debug_str
}

fn process_sections_to_batch(
    mmap: &memmap2::Mmap,
    sections: &[Tpx3Section],
    det_config: &DetectorConfig,
    tx: &Sender<AppMessage>,
) -> HitBatch {
    let num_packets: usize = sections.iter().map(Tpx3Section::packet_count).sum();
    let mut full_batch = HitBatch::with_capacity(num_packets);
    let tdc_correction = det_config.tdc_correction_25ns();

    let max_chip = sections.iter().map(|s| s.chip_id).max().unwrap_or(0) as usize;
    let mut sections_by_chip = vec![Vec::new(); max_chip + 1];
    for section in sections {
        sections_by_chip[section.chip_id as usize].push(section.clone());
    }

    let total_hits = num_packets.max(1);
    let mut receivers: Vec<Option<std::sync::mpsc::Receiver<PulseBatch>>> =
        vec![None; max_chip + 1];
    let mut heap = BinaryHeap::new();

    std::thread::scope(|scope| {
        for (chip_id, chip_sections) in sections_by_chip.iter().enumerate() {
            if chip_sections.is_empty() {
                continue;
            }

            let (tx_batch, rx_batch) = sync_channel::<PulseBatch>(2);
            receivers[chip_id] = Some(rx_batch);

            let chip_sections = chip_sections.clone();
            let transform = det_config
                .chip_transforms
                .get(chip_id)
                .cloned()
                .unwrap_or_else(ChipTransform::identity);

            scope.spawn(move || {
                let transform_closure = move |_cid, x, y| transform.apply(x, y);
                let mut reader = PulseReader::new(
                    mmap,
                    &chip_sections,
                    tdc_correction,
                    transform_closure,
                );
                while let Some(batch) = reader.next_pulse() {
                    if tx_batch.send(batch).is_err() {
                        break;
                    }
                }
            });
        }

        for rx_opt in receivers.iter().flatten() {
            if let Ok(batch) = rx_opt.recv() {
                heap.push(batch);
            }
        }

        while let Some(head) = heap.peek() {
            let min_tdc = head.extended_tdc();
            let mut merged = HitBatch::default();

            while let Some(batch) = heap.peek() {
                if batch.extended_tdc() != min_tdc {
                    break;
                }
                let batch = heap.pop().expect("heap not empty");

                if let Some(rx) = receivers
                    .get(batch.chip_id as usize)
                    .and_then(|opt| opt.as_ref())
                {
                    if let Ok(next) = rx.recv() {
                        heap.push(next);
                    }
                }

                merged.append(&batch.hits);
            }

            if merged.is_empty() {
                continue;
            }
            merged.sort_by_tof();
            full_batch.append(&merged);

            let progress =
                0.25 + 0.75 * (usize_to_f32(full_batch.len()) / usize_to_f32(total_hits));
            let _ = tx.send(AppMessage::LoadProgress(
                progress,
                format!("Processed {}/{} hits...", full_batch.len(), num_packets),
            ));
        }
    });

    full_batch
}

fn compute_counts_and_hist(batch: &HitBatch, tdc_correction: u32) -> (Vec<u32>, Vec<u64>) {
    let mut counts = vec![0u32; 512 * 512];
    for i in 0..batch.len() {
        let x = usize::from(batch.x[i]);
        let y = usize::from(batch.y[i]);
        if x < 512 && y < 512 {
            counts[y * 512 + x] += 1;
        }
    }

    let n_bins = 200;
    let mut hist = vec![0u64; n_bins];
    let hist_max = u32_to_f64(tdc_correction);
    let bin_width = hist_max / usize_to_f64(n_bins);

    if bin_width > 0.0 {
        for &t in &batch.tof {
            let val = u32_to_f64(t);
            if val < hist_max {
                if let Some(bin) = f64_to_usize_bounded(val / bin_width, n_bins) {
                    hist[bin] += 1;
                }
            }
        }
    }

    (counts, hist)
}

fn usize_to_f32(value: usize) -> f32 {
    #[allow(clippy::cast_precision_loss)]
    {
        value as f32
    }
}

fn usize_to_f64(value: usize) -> f64 {
    #[allow(clippy::cast_precision_loss)]
    {
        value as f64
    }
}

fn u32_to_f32(value: u32) -> f32 {
    #[allow(clippy::cast_precision_loss)]
    {
        value as f32
    }
}

fn u32_to_f64(value: u32) -> f64 {
    f64::from(value)
}

fn u64_to_f64(value: u64) -> f64 {
    #[allow(clippy::cast_precision_loss)]
    {
        value as f64
    }
}

fn f32_to_u8(value: f32) -> u8 {
    let clamped = value.clamp(0.0, 255.0);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    {
        clamped.round() as u8
    }
}

fn f64_to_usize_bounded(value: f64, max_exclusive: usize) -> Option<usize> {
    if !value.is_finite() || value < 0.0 {
        return None;
    }
    let max_f64 = usize_to_f64(max_exclusive);
    if value >= max_f64 {
        return None;
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    {
        Some(value as usize)
    }
}

impl eframe::App for RustpixApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.handle_messages(ctx);
        self.render_side_panel(ctx);
        self.render_central_panel(ctx);
        self.render_histogram_window(ctx);

        if self.processing.is_loading || self.processing.is_processing {
            ctx.request_repaint();
        }
    }
}

fn main() -> eframe::Result<()> {
    env_logger::init();
    let opts = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1200.0, 800.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Rustpix",
        opts,
        Box::new(|_| Ok(Box::new(RustpixApp::default()))),
    )
}
