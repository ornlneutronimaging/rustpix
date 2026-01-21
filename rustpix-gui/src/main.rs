#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use eframe::egui;
use egui_plot::{Bar, BarChart, Plot, PlotImage, PlotPoint};
use rayon::prelude::*;
use rfd::FileDialog;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant};

// Logic Imports
use rustpix_algorithms::{
    AbsClustering, AbsConfig, AbsState, DbscanClustering, DbscanConfig, DbscanState,
    GridClustering, GridConfig, GridState,
};

use rustpix_core::neutron::Neutron;
use rustpix_core::soa::HitBatch;
use rustpix_io::scanner::PacketScanner;
use rustpix_tpx::section::{process_section_into_batch, scan_section_tdc, Tpx3Section};

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
    ProcessingComplete(Vec<Neutron>, Duration),
    ProcessingError(String),
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
    neutrons: Vec<Neutron>,
    tof_hist_full: Option<Vec<u64>>, // Cached histogram
    // Cursor Info: x, y, hits
    cursor_info: Option<(usize, usize, u32)>,

    // Config
    tdc_frequency: f64,
    log_plot: bool,

    // App Logic
    rx: Receiver<AppMessage>,
    tx: Sender<AppMessage>,

    is_loading: bool,
    is_processing: bool,
    progress: f32,
    status_text: String,

    // Stats - Removed as per original code edit, but not explicitly requested to remove.
    // load_time: Option<Duration>,
    // proc_time: Option<Duration>,

    // Vis
    texture: Option<egui::TextureHandle>,
    // histogram_bins: usize, // Fixed to 512 for now matching sensor

    // Visualization
    colormap: Colormap,
    show_histogram: bool,
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
            neutrons: Vec::new(),
            tof_hist_full: None,
            cursor_info: None,
            // debug_info removed

            // Config
            tdc_frequency: 60.0,
            log_plot: false,

            rx,
            tx,

            is_loading: false,
            is_processing: false,
            progress: 0.0,
            status_text: "Ready".to_owned(),

            // load_time: None, // Removed
            // proc_time: None, // Removed
            texture: None,
            // histogram_bins: 512,
            colormap: Colormap::Green,
            show_histogram: false,
        }
    }
}

impl RustpixApp {
    fn load_file(&mut self, path: PathBuf) {
        self.selected_file = Some(path.clone());
        self.is_loading = true;
        self.progress = 0.0;
        self.status_text = "Loading file...".to_owned();
        self.hit_batch = None;
        self.hit_counts = None;
        self.neutrons.clear();
        self.texture = None;
        self.tof_hist_full = None;

        let tx = self.tx.clone();
        let tdc_frequency = self.tdc_frequency;

        thread::spawn(move || {
            let start = Instant::now();

            let file = match std::fs::File::open(&path) {
                Ok(f) => f,
                Err(e) => {
                    let _ = tx.send(AppMessage::LoadError(e.to_string()));
                    return;
                }
            };

            let mmap = unsafe {
                match memmap2::Mmap::map(&file) {
                    Ok(m) => m,
                    Err(e) => {
                        let _ = tx.send(AppMessage::LoadError(e.to_string()));
                        return;
                    }
                }
            };

            tx.send(AppMessage::LoadProgress(
                0.1,
                "Scanning sections...".to_string(),
            ))
            .unwrap();

            // 2. Scan Sections
            let (io_sections, _) = PacketScanner::scan_sections(&mmap, true);
            let total_sections = io_sections.len();

            tx.send(AppMessage::LoadProgress(
                0.15,
                format!("Found {} sections. Prescanning TDCs...", total_sections),
            ))
            .unwrap();

            // 2.5 Prescan TDCs
            // We need to build Tpx3Sections with valid TDCs.
            // We iterate strictly in order to propagate TDCs per chip.
            let mut tpx_sections = Vec::with_capacity(total_sections);
            let mut chip_tdc_state = [None; 256];

            for s in io_sections {
                let initial = chip_tdc_state[s.chip_id as usize];

                let mut rules = Tpx3Section {
                    start_offset: s.start_offset,
                    end_offset: s.end_offset,
                    chip_id: s.chip_id,
                    initial_tdc: initial,
                    final_tdc: None,
                };

                // Scan for final TDC to propagate
                if let Some(final_t) = scan_section_tdc(&mmap, &rules) {
                    rules.final_tdc = Some(final_t);
                    chip_tdc_state[s.chip_id as usize] = Some(final_t);
                }

                tpx_sections.push(rules);
            }

            tx.send(AppMessage::LoadProgress(
                0.25,
                "Processing hits...".to_string(),
            ))
            .unwrap();

            use rustpix_tpx::DetectorConfig;

            // 3. Process into Batch
            let mut det_config = DetectorConfig::venus_defaults();
            // Override with user config
            det_config.tdc_frequency_hz = tdc_frequency;
            let tdc_correction = det_config.tdc_correction_25ns();

            // Debug Info Calculation
            let mut debug_str = format!("TDC Correction (25ns): {}\n", tdc_correction);
            if let Some(sec) = tpx_sections.iter().find(|s| s.initial_tdc.is_some()) {
                if let Some(tdc) = sec.initial_tdc {
                    debug_str.push_str(&format!("Sec TDC Ref: {}\n", tdc));
                    // Find first hit manually
                    let sdata = &mmap[sec.start_offset..sec.end_offset];
                    let mut found = false;
                    for ch in sdata.chunks_exact(8) {
                        let raw = u64::from_le_bytes(ch.try_into().unwrap());
                        let p = rustpix_tpx::Tpx3Packet::new(raw);
                        if p.is_hit() {
                            let raw_ts = p.timestamp_coarse();
                            let ts = rustpix_tpx::correct_timestamp_rollover(raw_ts, tdc);
                            let raw_tof = ts.wrapping_sub(tdc);
                            let tof = rustpix_tpx::calculate_tof(ts, tdc, tdc_correction);
                            debug_str.push_str(&format!("Sample Hit:\n  RawTS: {}\n  CorrTS: {}\n  RawDelta: {}\n  CalcTOF: {}\n", raw_ts, ts, raw_tof, tof));
                            found = true;
                            break;
                        }
                    }
                    if !found {
                        debug_str.push_str("Section has no hits.\n");
                    }
                }
            } else {
                debug_str.push_str("No sections with valid Initial TDC found.\n");
            }

            let num_packets: usize = tpx_sections.iter().map(|s| s.packet_count()).sum();
            let mut full_batch = HitBatch::with_capacity(num_packets);

            let chunk_size = 100;
            let chunks: Vec<_> = tpx_sections.chunks(chunk_size).collect();
            let total_chunks = chunks.len();

            for (i, chunk) in chunks.iter().enumerate() {
                let batches: Vec<HitBatch> = chunk
                    .par_iter()
                    .map(|sec| {
                        let mut b = HitBatch::with_capacity(sec.packet_count());

                        // precise-capture the reference to config if needed, or just clone lightweight struct?
                        // DetectorConfig is Clone.
                        let dc = det_config.clone();
                        let transform = move |cid, x, y| dc.map_chip_to_global(cid, x, y);

                        process_section_into_batch(&mmap, sec, tdc_correction, transform, &mut b);
                        b
                    })
                    .collect();

                for b in batches {
                    full_batch.append(&b);
                }

                let p = 0.25 + 0.75 * (i as f32 / total_chunks as f32);
                tx.send(AppMessage::LoadProgress(
                    p,
                    format!("Processed {}/{} hits...", full_batch.len(), num_packets),
                ))
                .unwrap();
            }

            // 4. Compute Metadata (Counts & Histogram) in background to avoid Main Thread hang
            tx.send(AppMessage::LoadProgress(
                0.95,
                "Generating visualization data...".to_string(),
            ))
            .unwrap();

            // Counts Grid (512x512)
            let mut counts = vec![0u32; 512 * 512];
            for i in 0..full_batch.len() {
                let x = full_batch.x[i] as usize;
                let y = full_batch.y[i] as usize;
                if x < 512 && y < 512 {
                    counts[y * 512 + x] += 1;
                }
            }

            // ToF Histogram (Fixed 200 bins)
            let n_bins = 200;
            let mut hist = vec![0u64; n_bins];

            // Use TDC period (frame time) as the natural maximum for the histogram
            // instead of the raw max, because outliers (u32::MAX due to negativity) skew the scale.
            let hist_max = tdc_correction as f64;
            let bin_width = hist_max / n_bins as f64;

            if bin_width > 0.0 {
                for &t in &full_batch.tof {
                    let val = t as f64;
                    // Clamp to histogram range
                    if val < hist_max {
                        let bin = (val / bin_width) as usize;
                        if bin < n_bins {
                            hist[bin] += 1;
                        }
                    }
                }
            }

            tx.send(AppMessage::LoadComplete(
                Box::new(full_batch),
                counts,
                hist,
                start.elapsed(),
                // Debug info removed
                String::new(),
            ))
            .unwrap();
        });
    }

    fn run_processing(&mut self) {
        if let Some(batch) = &self.hit_batch {
            self.is_processing = true;
            self.progress = 0.0;
            self.status_text = "Clustering...".to_owned();

            let tx = self.tx.clone();
            let mut working_batch = batch.clone();
            let algo_type = self.algo_type;
            let config = (
                self.radius,
                self.temporal_window_ns,
                self.min_cluster_size,
                self.dbscan_min_points,
            );

            thread::spawn(move || {
                let start = Instant::now();
                let (radius, window, min_size, min_points) = config;

                let num_clusters = match algo_type {
                    AlgorithmType::Grid => {
                        let cfg = GridConfig {
                            cell_size: 32,
                            radius,
                            temporal_window_ns: window,
                            min_cluster_size: min_size,
                            max_cluster_size: None,
                        };
                        let algo = GridClustering::new(cfg);
                        let mut state = GridState::default();
                        algo.cluster(&mut working_batch, &mut state)
                    }
                    AlgorithmType::Abs => {
                        let cfg = AbsConfig {
                            radius,
                            neutron_correlation_window_ns: window,
                            min_cluster_size: min_size,
                            scan_interval: 100,
                        };
                        let algo = AbsClustering::new(cfg);
                        let mut state = AbsState::default();
                        algo.cluster(&mut working_batch, &mut state)
                    }
                    AlgorithmType::Dbscan => {
                        let cfg = DbscanConfig {
                            epsilon: radius,
                            temporal_window_ns: window,
                            min_points,
                            min_cluster_size: min_size,
                        };
                        let algo = DbscanClustering::new(cfg);
                        let mut state = DbscanState::default();
                        algo.cluster(&mut working_batch, &mut state)
                    }
                };

                match num_clusters {
                    Ok(n) => {
                        tx.send(AppMessage::ProcessingProgress(
                            0.9,
                            format!("Found {} clusters. Extracting...", n),
                        ))
                        .unwrap();

                        let mut neutrons = Vec::with_capacity(n);
                        if n > 0 {
                            struct Acc {
                                count: u32,
                                sum_x: f64,
                                sum_y: f64,
                                sum_tof: f64,
                            }

                            let mut accs = Vec::with_capacity(n);
                            accs.resize_with(n, || Acc {
                                count: 0,
                                sum_x: 0.0,
                                sum_y: 0.0,
                                sum_tof: 0.0,
                            });

                            for i in 0..working_batch.len() {
                                let cid = working_batch.cluster_id[i];
                                if cid >= 0 {
                                    let a = &mut accs[cid as usize];
                                    a.count += 1;
                                    a.sum_x += working_batch.x[i] as f64;
                                    a.sum_y += working_batch.y[i] as f64;
                                    a.sum_tof += working_batch.tof[i] as f64;
                                }
                            }

                            for a in accs.iter() {
                                if a.count > 0 {
                                    neutrons.push(Neutron {
                                        x: a.sum_x / a.count as f64,
                                        y: a.sum_y / a.count as f64,
                                        tof: (a.sum_tof / a.count as f64) as u32, // approx
                                        tot: 0,
                                        n_hits: a.count as u16,
                                        chip_id: 0,
                                        _reserved: [0; 3],
                                    });
                                }
                            }
                        }

                        tx.send(AppMessage::ProcessingComplete(neutrons, start.elapsed()))
                            .unwrap();
                    }
                    Err(e) => {
                        let _ = tx.send(AppMessage::ProcessingError(e.to_string()));
                    }
                }
            });
        }
    }

    fn generate_histogram(&self) -> egui::ColorImage {
        let counts = match &self.hit_counts {
            Some(c) => c,
            None => return egui::ColorImage::new([512, 512], egui::Color32::BLACK),
        };

        // Find max for scaling
        let max_count = counts.iter().max().copied().unwrap_or(1) as f32;
        let mut pixels = Vec::with_capacity(512 * 512 * 4);

        for &count in counts {
            let val = (count as f32 / max_count).sqrt(); // Sqrt scale
            let v = (val * 255.0) as u8;
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
                            let g = (val * 2.0 * 255.0) as u8;
                            pixels.extend_from_slice(&[r, g, 0, 255]);
                        } else {
                            // Yellow to White
                            let r = 255;
                            let g = 255;
                            let b = ((val - 0.5) * 2.0 * 255.0) as u8;
                            pixels.extend_from_slice(&[r, g, b, 255]);
                        }
                    }
                    Colormap::Viridis => {
                        // Approximate Viridis (Blue -> Teal -> Green -> Yellow)
                        let r = (255.0 * val.powf(2.0)) as u8;
                        let g = (255.0 * val) as u8;
                        let b = (255.0 * (1.0 - val)) as u8;
                        pixels.extend_from_slice(&[r, g, b, 255]);
                    }
                }
            }
        }

        egui::ColorImage::from_rgba_unmultiplied([512, 512], &pixels)
    }
}

impl eframe::App for RustpixApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                AppMessage::LoadProgress(p, s) => {
                    self.progress = p;
                    self.status_text = s;
                }
                AppMessage::LoadComplete(batch, counts, hist, dur, _dbg) => {
                    self.is_loading = false;
                    self.progress = 1.0;
                    self.status_text =
                        format!("Loaded {} hits in {:.2}s", batch.len(), dur.as_secs_f64());

                    self.hit_counts = Some(counts);
                    self.tof_hist_full = Some(hist); // Cached full histogram
                    self.hit_batch = Some(*batch);
                    // self.debug_info = dbg; // Removed

                    // Generate texture
                    let img = self.generate_histogram();
                    self.texture =
                        Some(ctx.load_texture("hist", img, egui::TextureOptions::NEAREST));
                }
                AppMessage::LoadError(e) => {
                    self.is_loading = false;
                    self.status_text = format!("Error: {}", e);
                }
                AppMessage::ProcessingProgress(p, s) => {
                    self.progress = p;
                    self.status_text = s;
                }
                AppMessage::ProcessingComplete(neutrons, dur) => {
                    self.is_processing = false;
                    self.progress = 1.0;
                    self.status_text = format!(
                        "Found {} neutrons in {:.2}ms",
                        neutrons.len(),
                        dur.as_secs_f64() * 1000.0
                    );
                    self.neutrons = neutrons;
                }
                AppMessage::ProcessingError(e) => {
                    self.is_processing = false;
                    self.status_text = format!("Error: {}", e);
                }
            }
        }

        egui::SidePanel::left("ctrl").show(ctx, |ui| {
            ui.heading("Rustpix GUI");
            ui.separator();

            if ui.button("Open File").clicked() && !self.is_loading {
                if let Some(path) = FileDialog::new().add_filter("TPX3", &["tpx3"]).pick_file() {
                    self.load_file(path);
                }
            }
            if let Some(p) = &self.selected_file {
                ui.label(p.file_name().unwrap_or_default().to_string_lossy());
            }

            ui.separator();

            if self.is_loading {
                ui.add(egui::ProgressBar::new(self.progress).text(&self.status_text));
            } else {
                ui.label(&self.status_text);
            }

            ui.separator();

            if let Some((x, y, count)) = self.cursor_info {
                ui.label(egui::RichText::new("Pixel Info:").strong());
                ui.label(format!("X: {}\nY: {}\nHits: {}", x, y, count));
            } else {
                ui.label("Hover over image for details.");
            }

            ui.separator();

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

            ui.toggle_value(&mut self.show_histogram, "Show TOF Histogram");

            ui.separator();

            ui.heading("Processing");
            // ... same processing UI ...
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
                egui::Slider::new(&mut self.temporal_window_ns, 10.0..=500.0)
                    .text("Time Window (ns)"),
            );
            ui.add(egui::Slider::new(&mut self.min_cluster_size, 1..=10).text("Min Cluster"));

            if self.algo_type == AlgorithmType::Dbscan {
                ui.add(egui::Slider::new(&mut self.dbscan_min_points, 1..=10).text("Min Points"));
            }

            if ui
                .add_enabled(
                    !self.is_processing && self.hit_batch.is_some(),
                    egui::Button::new("Run Clustering"),
                )
                .clicked()
            {
                self.run_processing();
            }

            if self.is_processing {
                ui.add(egui::ProgressBar::new(self.progress).text(&self.status_text));
            }

            ui.separator();
            ui.label(format!("Neutrons: {}", self.neutrons.len()));
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            if let Some(tex) = &self.texture {
                Plot::new("plot").data_aspect(1.0).show(ui, |plot_ui| {
                    plot_ui.image(PlotImage::new(
                        tex,
                        PlotPoint::new(256.0, 256.0),
                        [512.0, 512.0],
                    ));

                    // Update Cursor Information
                    if let Some(curr) = plot_ui.pointer_coordinate() {
                        let x = curr.x;
                        let y = curr.y;
                        if x >= 0.0 && y >= 0.0 && x < 512.0 && y < 512.0 {
                            let xi = x as usize;
                            let yi = y as usize;
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

        if self.show_histogram {
            egui::Window::new("TOF Histogram").show(ctx, |ui| {
                // Use pre-computed histogram ONLY. No heavy calculation here.
                // Use pre-computed histogram ONLY. No heavy calculation here.
                if let Some(full) = &self.tof_hist_full {
                    // Recompute conversion for display
                    let tdc_period = 1.0 / self.tdc_frequency; // s
                    let max_us = tdc_period * 1e6; // microseconds
                                                   // max_ticks removed
                    let n_bins = full.len();
                    let bin_width_us = max_us / n_bins as f64;
                    // bin_width_ticks removed

                    ui.horizontal(|ui| {
                        ui.checkbox(&mut self.log_plot, "Log Scale");
                        ui.label(format!(
                            "Range: 0 - {:.0} µs ({:.1} Hz)",
                            max_us, self.tdc_frequency
                        ));
                    });

                    Plot::new("tof_hist")
                        .x_axis_label("Time-of-Flight (µs)")
                        .y_axis_label(if self.log_plot {
                            "Log10(Counts)"
                        } else {
                            "Counts"
                        })
                        .include_x(0.0)
                        .include_x(max_us)
                        .include_y(0.0) // Start Y at 0
                        .show(ui, |plot_ui: &mut egui_plot::PlotUi| {
                            // Plot full (Blue)
                            let bars: Vec<Bar> = full
                                .iter()
                                .enumerate()
                                .map(|(i, &c)| {
                                    // Scale x to microseconds
                                    let x = i as f64 * bin_width_us;

                                    // Manual log adjustment
                                    // For log plot, we take log10(c). If c=0, use 0.
                                    let val = if self.log_plot {
                                        if c > 0 {
                                            (c as f64).log10()
                                        } else {
                                            0.0
                                        }
                                    } else {
                                        c as f64
                                    };

                                    Bar::new(x, val)
                                        .width(bin_width_us)
                                        .fill(egui::Color32::BLUE)
                                })
                                .collect();
                            plot_ui.bar_chart(BarChart::new(bars).name("Full"));
                        });

                    // ui.label(...) moved to top
                } else {
                    ui.label("No Data");
                }
            });
        }

        if self.is_loading || self.is_processing {
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
