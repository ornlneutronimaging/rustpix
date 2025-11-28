#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release

use eframe::egui;
use egui_plot::{Plot, PlotImage, PlotPoint};
use rfd::FileDialog;
use rustpix_algorithms::{AbsClustering, DbscanClustering, GraphClustering, GridClustering, HitClustering};
use rustpix_core::clustering::ClusteringConfig;
use rustpix_core::extraction::{ExtractionConfig, NeutronExtraction, SimpleCentroidExtraction};
use rustpix_core::neutron::Neutron;
use rustpix_io::Tpx3FileReader;
use std::path::PathBuf;
use std::time::{Duration, Instant};

fn main() -> eframe::Result<()> {
    env_logger::init(); // Log to stderr (if you run with `RUST_LOG=debug`)
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1280.0, 720.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Rustpix GUI",
        options,
        Box::new(|_cc| Ok(Box::new(RustpixApp::default()))),
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Algorithm {
    Abs,
    Dbscan,
    Graph,
    Grid,
}

impl std::fmt::Display for Algorithm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Algorithm::Abs => write!(f, "ABS (Age-Based Spatial)"),
            Algorithm::Dbscan => write!(f, "DBSCAN"),
            Algorithm::Graph => write!(f, "Graph (Union-Find)"),
            Algorithm::Grid => write!(f, "Grid (Spatial Index)"),
        }
    }
}

struct RustpixApp {
    // File state
    selected_file: Option<PathBuf>,
    file_info: String,

    // Processing config
    algorithm: Algorithm,
    radius: f64,
    temporal_window_ns: f64,
    min_cluster_size: u16,

    // Processing state
    is_processing: bool,
    last_processing_time: Option<Duration>,
    neutrons: Vec<Neutron>,
    
    // Visualization
    histogram_bins: usize,
    texture: Option<egui::TextureHandle>,
}

impl Default for RustpixApp {
    fn default() -> Self {
        Self {
            selected_file: None,
            file_info: "No file selected".to_owned(),
            algorithm: Algorithm::Abs,
            radius: 5.0,
            temporal_window_ns: 75.0,
            min_cluster_size: 1,
            is_processing: false,
            last_processing_time: None,
            neutrons: Vec::new(),
            histogram_bins: 512,
            texture: None,
        }
    }
}

impl RustpixApp {
    fn process_file(&mut self) {
        if let Some(path) = &self.selected_file {
            self.is_processing = true;
            let start = Instant::now();

            // Clone config to avoid borrow checker issues
            let path = path.clone();
            let algorithm = self.algorithm;
            let config = ClusteringConfig {
                radius: self.radius,
                temporal_window_ns: self.temporal_window_ns,
                min_cluster_size: self.min_cluster_size,
                max_cluster_size: None,
            };

            // This should ideally be done in a separate thread, but for simplicity we do it here
            // In a real app, use a channel or promise
            match self.run_processing(&path, algorithm, config) {
                Ok(neutrons) => {
                    self.neutrons = neutrons;
                    self.last_processing_time = Some(start.elapsed());
                    self.update_texture();
                }
                Err(e) => {
                    self.file_info = format!("Error processing file: {}", e);
                }
            }
            
            self.is_processing = false;
        }
    }

    fn run_processing(&self, path: &PathBuf, algorithm: Algorithm, config: ClusteringConfig) -> anyhow::Result<Vec<Neutron>> {
        let reader = Tpx3FileReader::open(path)?;
        let hits = reader.read_hits()?;
        
        let mut labels = vec![0; hits.len()];
        
        let num_clusters = match algorithm {
            Algorithm::Abs => {
                let mut algo = AbsClustering::default();
                algo.configure(&config);
                let mut state = algo.create_state();
                algo.cluster(&hits, &mut state, &mut labels)?
            },
            Algorithm::Dbscan => {
                let mut algo = DbscanClustering::default();
                algo.configure(&config);
                let mut state = algo.create_state();
                algo.cluster(&hits, &mut state, &mut labels)?
            },
            Algorithm::Graph => {
                let mut algo = GraphClustering::default();
                algo.configure(&config);
                let mut state = algo.create_state();
                algo.cluster(&hits, &mut state, &mut labels)?
            },
            Algorithm::Grid => {
                let mut algo = GridClustering::default();
                algo.configure(&config);
                let mut state = algo.create_state();
                algo.cluster(&hits, &mut state, &mut labels)?
            },
        };

        let mut extractor = SimpleCentroidExtraction::new();
        extractor.configure(ExtractionConfig::default());
        let neutrons = extractor.extract(&hits, &labels, num_clusters)?;
        
        Ok(neutrons)
    }

    fn update_texture(&mut self) {
        // Simple 2D histogram
        if self.neutrons.is_empty() {
            self.texture = None;
            return;
        }

        let bins = self.histogram_bins;
        let mut grid = vec![0u32; bins * bins];
        
        // Find bounds (assuming 256x256 pixel detector scaled by super-resolution)
        // Default super-res is 8.0, so 256*8 = 2048.0
        let max_coord = 256.0 * 8.0; 
        
        for n in &self.neutrons {
            let x = (n.x / max_coord * bins as f64) as usize;
            let y = (n.y / max_coord * bins as f64) as usize;
            
            if x < bins && y < bins {
                grid[y * bins + x] += 1;
            }
        }
        
        let max_count = grid.iter().max().copied().unwrap_or(1) as f32;
        
        // Convert to RGBA image
        let mut pixels = Vec::with_capacity(bins * bins * 4);
        for &count in &grid {
            let intensity = (count as f32 / max_count).sqrt(); // Sqrt scaling for better visibility
            // Viridis-like colormap (simplified)
            let r = (intensity * 255.0) as u8;
            let g = (intensity * 200.0) as u8;
            let b = (intensity * 100.0 + 50.0).min(255.0) as u8;
            let a = if count > 0 { 255 } else { 0 };
            
            pixels.push(r);
            pixels.push(g);
            pixels.push(b);
            pixels.push(a);
        }

        let _image = egui::ColorImage::from_rgba_unmultiplied(
            [bins, bins],
            &pixels,
        );
        
        // We can't easily get the context here to allocate the texture immediately if called from background
        // But since we are single threaded for now, we can just store the image data?
        // Actually, egui textures need to be allocated with ctx.
        // We'll just store the ColorImage and allocate in update()
    }
    
    // Helper to generate image data
    fn generate_histogram_image(&self) -> egui::ColorImage {
        let bins = self.histogram_bins;
        if self.neutrons.is_empty() {
            return egui::ColorImage::new([bins, bins], egui::Color32::BLACK);
        }

        let mut grid = vec![0u32; bins * bins];
        let max_coord = 256.0 * 8.0; 
        
        for n in &self.neutrons {
            let x = (n.x / max_coord * bins as f64) as usize;
            let y = (n.y / max_coord * bins as f64) as usize;
            
            if x < bins && y < bins {
                // Flip Y for visualization if needed, but standard image coords usually have 0,0 top left
                // Detector usually has 0,0 top left too.
                grid[y * bins + x] += 1;
            }
        }
        
        let max_count = grid.iter().max().copied().unwrap_or(1) as f32;
        
        let mut pixels = Vec::with_capacity(bins * bins * 4);
        for &count in &grid {
            if count == 0 {
                pixels.extend_from_slice(&[0, 0, 0, 255]); // Black background
            } else {
                let val = (count as f32 / max_count).sqrt();
                // Heatmap colors: Blue -> Green -> Red -> Yellow
                let (r, g, b) = if val < 0.25 {
                    (0, (val * 4.0 * 255.0) as u8, 255)
                } else if val < 0.5 {
                    (0, 255, (255.0 - (val - 0.25) * 4.0 * 255.0) as u8)
                } else if val < 0.75 {
                    (((val - 0.5) * 4.0 * 255.0) as u8, 255, 0)
                } else {
                    (255, (255.0 - (val - 0.75) * 4.0 * 255.0) as u8, 0)
                };
                pixels.extend_from_slice(&[r, g, b, 255]);
            }
        }

        egui::ColorImage::from_rgba_unmultiplied(
            [bins, bins],
            &pixels,
        )
    }
}

impl eframe::App for RustpixApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::SidePanel::left("controls_panel").show(ctx, |ui| {
            ui.heading("Controls");
            ui.separator();

            // File Loading
            ui.group(|ui| {
                ui.label("File Selection");
                if ui.button("Open TPX3 File...").clicked() {
                    if let Some(path) = FileDialog::new()
                        .add_filter("TPX3", &["tpx3"])
                        .pick_file() 
                    {
                        self.selected_file = Some(path.clone());
                        // Basic info
                        if let Ok(reader) = Tpx3FileReader::open(&path) {
                            self.file_info = format!(
                                "Size: {:.2} MB\nPackets: {}", 
                                reader.file_size() as f64 / 1_000_000.0,
                                reader.packet_count()
                            );
                        } else {
                            self.file_info = "Error reading file".to_owned();
                        }
                    }
                }
                if let Some(path) = &self.selected_file {
                    ui.label(path.file_name().unwrap_or_default().to_string_lossy());
                }
                ui.label(&self.file_info);
            });

            ui.separator();

            // Processing Controls
            ui.group(|ui| {
                ui.label("Clustering Configuration");
                
                egui::ComboBox::from_label("Algorithm")
                    .selected_text(self.algorithm.to_string())
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.algorithm, Algorithm::Abs, "ABS");
                        ui.selectable_value(&mut self.algorithm, Algorithm::Dbscan, "DBSCAN");
                        ui.selectable_value(&mut self.algorithm, Algorithm::Graph, "Graph");
                        ui.selectable_value(&mut self.algorithm, Algorithm::Grid, "Grid");
                    });

                ui.add(egui::Slider::new(&mut self.radius, 1.0..=20.0).text("Radius (px)"));
                ui.add(egui::Slider::new(&mut self.temporal_window_ns, 10.0..=500.0).text("Time Window (ns)"));
                ui.add(egui::Slider::new(&mut self.min_cluster_size, 1..=10).text("Min Cluster Size"));

                ui.separator();

                if ui.add_enabled(!self.is_processing && self.selected_file.is_some(), egui::Button::new("Process")).clicked() {
                    self.process_file();
                    // Update texture after processing
                    let image = self.generate_histogram_image();
                    self.texture = Some(ctx.load_texture(
                        "histogram",
                        image,
                        egui::TextureOptions::NEAREST // Nearest for pixelated look
                    ));
                }
            });

            ui.separator();

            // Statistics
            ui.group(|ui| {
                ui.label("Statistics");
                ui.label(format!("Neutrons: {}", self.neutrons.len()));
                if let Some(duration) = self.last_processing_time {
                    ui.label(format!("Time: {:.2} ms", duration.as_secs_f64() * 1000.0));
                }
                
                if !self.neutrons.is_empty() {
                    let mean_size = self.neutrons.iter().map(|n| n.n_hits as f64).sum::<f64>() / self.neutrons.len() as f64;
                    ui.label(format!("Mean Cluster Size: {:.2}", mean_size));
                }
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Visualization");
            
            if let Some(texture) = &self.texture {
                let _size = texture.size_vec2();
                // Use Plot for zoom/pan
                Plot::new("histogram_plot")
                    .data_aspect(1.0)
                    .show(ui, |plot_ui| {
                        plot_ui.image(
                            PlotImage::new(
                                texture,
                                PlotPoint::new(1024.0, 1024.0), // Center (2048/2)
                                [2048.0, 2048.0] // Size (256 * 8)
                            )
                        );
                    });
            } else {
                ui.label("Load a file and click Process to view data.");
            }
        });
    }
}
