//!
//! This binary will provide a CLI for processing pixel detector data.

use clap::{Parser, Subcommand, ValueEnum};

use rustpix_algorithms::{
    AbsClustering, AbsState, DbscanClustering, DbscanState, GridClustering, GridState,
};
use rustpix_core::extraction::NeutronExtraction;
use rustpix_core::soa::HitBatch;
use rustpix_io::Tpx3FileReader;
use std::path::PathBuf;
use std::time::Instant;
use thiserror::Error;

/// Result type for CLI operations.
type Result<T> = std::result::Result<T, CliError>;

/// CLI error types.
#[derive(Error, Debug)]
enum CliError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("I/O error: {0}")]
    RustpixIo(#[from] rustpix_io::Error),

    #[error("Core error: {0}")]
    Core(#[from] rustpix_core::Error),

    #[error("Clustering error: {0}")]
    Clustering(#[from] rustpix_core::ClusteringError),

    #[error("Extraction error: {0}")]
    Extraction(#[from] rustpix_core::ExtractionError),
}

/// Clustering algorithm selection.
#[derive(Debug, Clone, Copy, ValueEnum)]
enum Algorithm {
    /// Age-Based Spatial clustering (primary, O(n) average)
    Abs,
    /// DBSCAN clustering
    Dbscan,
    /// Grid-based clustering with spatial indexing
    Grid,
}

/// High-performance pixel detector data processor.
#[derive(Parser)]
#[command(name = "rustpix")]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Process TPX3 files to extract neutron events
    Process {
        /// Input TPX3 file(s)
        #[arg(required = true)]
        input: Vec<PathBuf>,

        /// Output file path
        #[arg(short, long)]
        output: PathBuf,

        /// Clustering algorithm to use
        #[arg(short, long, value_enum, default_value = "abs")]
        algorithm: Algorithm,

        /// Spatial radius for clustering (pixels)
        #[arg(long, default_value = "5.0")]
        radius: f64,

        /// Temporal window for clustering (nanoseconds)
        #[arg(long, default_value = "75.0")]
        temporal_window_ns: f64,

        /// Minimum cluster size
        #[arg(long, default_value = "1")]
        min_cluster_size: u16,

        /// Verbose output
        #[arg(short, long)]
        verbose: bool,
    },

    /// Show information about a TPX3 file
    Info {
        /// Input TPX3 file
        input: PathBuf,
    },

    /// Benchmark clustering algorithms
    Benchmark {
        /// Input TPX3 file
        input: PathBuf,

        /// Number of iterations
        #[arg(short, long, default_value = "3")]
        iterations: usize,
    },

    /// Benchmark sorting strategies (Standard vs Streaming)
    OrderingBenchmark {
        /// Input TPX3 file
        input: PathBuf,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Process {
            input,
            output,
            algorithm,
            radius,
            temporal_window_ns,
            min_cluster_size,
            verbose,
        } => {
            // Processing pipeline:
            // 1. Read TPX3 file(s) with section discovery
            // 2. Parse hits with TDC propagation
            // 3. Cluster hits using selected algorithm
            // 4. Extract neutrons from clusters
            // 5. Write output

            if verbose {
                eprintln!("Processing {} file(s)...", input.len());
                eprintln!("Algorithm: {:?}", algorithm);
                eprintln!("Radius: {} pixels", radius);
                eprintln!("Temporal window: {} ns", temporal_window_ns);
                eprintln!("Min cluster size: {}", min_cluster_size);
            }

            let start = Instant::now();
            let mut all_neutrons = Vec::new();

            // Configure extraction
            let extraction_config = rustpix_core::extraction::ExtractionConfig::default();
            let mut extractor = rustpix_core::extraction::SimpleCentroidExtraction::new();
            extractor.configure(extraction_config);

            for path in &input {
                if verbose {
                    eprintln!("Reading: {}", path.display());
                }

                let reader = Tpx3FileReader::open(path)?;
                let hits = reader.read_hits()?;

                if verbose {
                    eprintln!("  {} hits read", hits.len());
                }

                // Convert to HitBatch for SoA clustering
                let mut batch = HitBatch::with_capacity(hits.len());
                for hit in &hits {
                    batch.push(hit.x, hit.y, hit.tof, hit.tot, hit.timestamp, hit.chip_id);
                }

                // Perform clustering
                let num_clusters = match algorithm {
                    Algorithm::Abs => {
                        let algo_config = rustpix_algorithms::AbsConfig {
                            radius,
                            neutron_correlation_window_ns: temporal_window_ns,
                            min_cluster_size,
                            scan_interval: 100,
                        };
                        let algo = AbsClustering::new(algo_config);
                        let mut state = AbsState::default();
                        algo.cluster(&mut batch, &mut state)?
                    }
                    Algorithm::Dbscan => {
                        let algo_config = rustpix_algorithms::DbscanConfig {
                            epsilon: radius,
                            temporal_window_ns,
                            min_points: 2,
                            min_cluster_size,
                        };
                        let algo = DbscanClustering::new(algo_config);
                        let mut state = DbscanState::default();
                        algo.cluster(&mut batch, &mut state)?
                    }
                    Algorithm::Grid => {
                        let algo_config = rustpix_algorithms::GridConfig {
                            radius,
                            temporal_window_ns,
                            min_cluster_size,
                            cell_size: 32,
                            max_cluster_size: None,
                        };
                        let algo = GridClustering::new(algo_config);
                        let mut state = GridState::default();
                        algo.cluster(&mut batch, &mut state)?
                    }
                };

                if verbose {
                    eprintln!("  {} clusters found", num_clusters);
                }

                // Perform extraction
                // Need to extract labels from batch
                let labels = batch.cluster_id.clone();
                let neutrons = extractor.extract(&hits, &labels, num_clusters)?;

                if verbose {
                    eprintln!("  {} neutrons extracted", neutrons.len());
                }

                all_neutrons.extend(neutrons);
            }

            let elapsed = start.elapsed();

            // Write output
            if verbose {
                eprintln!("Writing output to: {}", output.display());
            }

            let mut writer = rustpix_io::DataFileWriter::create(&output)?;

            if let Some(ext) = output.extension().and_then(|e| e.to_str()) {
                match ext.to_lowercase().as_str() {
                    "csv" => writer.write_neutrons_csv(&all_neutrons)?,
                    "bin" | "dat" => writer.write_neutrons_binary(&all_neutrons)?,
                    _ => {
                        if verbose {
                            eprintln!("Unknown extension '{}', defaulting to binary", ext);
                        }
                        writer.write_neutrons_binary(&all_neutrons)?;
                    }
                }
            } else {
                writer.write_neutrons_binary(&all_neutrons)?;
            }

            println!(
                "Processed {} files in {:.2}s",
                input.len(),
                elapsed.as_secs_f64()
            );
            println!("Total neutrons: {}", all_neutrons.len());
        }

        Commands::Info { input } => {
            let reader = Tpx3FileReader::open(&input)?;
            let file_size = reader.file_size();
            let packet_count = reader.packet_count();

            println!("File: {}", input.display());
            println!(
                "Size: {} bytes ({:.2} MB)",
                file_size,
                file_size as f64 / 1_000_000.0
            );
            println!("Packets: {}", packet_count);

            let hits = reader.read_hits()?;
            println!("Hits: {}", hits.len());

            if !hits.is_empty() {
                use rustpix_core::hit::Hit;
                let min_tof = hits.iter().map(|h| h.tof()).min().unwrap();
                let max_tof = hits.iter().map(|h| h.tof()).max().unwrap();
                println!("TOF range: {} - {}", min_tof, max_tof);

                let min_x = hits.iter().map(|h| h.x()).min().unwrap();
                let max_x = hits.iter().map(|h| h.x()).max().unwrap();
                let min_y = hits.iter().map(|h| h.y()).min().unwrap();
                let max_y = hits.iter().map(|h| h.y()).max().unwrap();
                println!("X range: {} - {}", min_x, max_x);
                println!("Y range: {} - {}", min_y, max_y);
            }
        }

        Commands::Benchmark { input, iterations } => {
            let reader = Tpx3FileReader::open(&input)?;
            let hits = reader.read_hits()?;

            println!(
                "Benchmarking with {} hits, {} iterations",
                hits.len(),
                iterations
            );

            let algorithms = [
                (Algorithm::Abs, "ABS"),
                (Algorithm::Dbscan, "DBSCAN"),
                (Algorithm::Grid, "Grid"),
            ];

            println!(
                "{:<10} | {:<15} | {:<15} | {:<15}",
                "Algorithm", "Mean Time (ms)", "Min Time (ms)", "Max Time (ms)"
            );
            println!("{:-<65}", "");

            for (algo_enum, name) in algorithms {
                let mut times = Vec::with_capacity(iterations);

                // Config
                // We construct config inside loop or here

                // Warmup
                {
                    let mut batch = HitBatch::with_capacity(hits.len());
                    for hit in &hits {
                        batch.push(hit.x, hit.y, hit.tof, hit.tot, hit.timestamp, hit.chip_id);
                    }
                    match algo_enum {
                        Algorithm::Abs => {
                            let algo_config = rustpix_algorithms::AbsConfig {
                                radius: 5.0,
                                neutron_correlation_window_ns: 75.0,
                                min_cluster_size: 1,
                                scan_interval: 100,
                            };
                            let algo = AbsClustering::new(algo_config);
                            let mut state = AbsState::default();
                            let _ = algo.cluster(&mut batch, &mut state);
                        }
                        Algorithm::Dbscan => {
                            let algo_config = rustpix_algorithms::DbscanConfig {
                                epsilon: 5.0,
                                temporal_window_ns: 75.0,
                                min_points: 2,
                                min_cluster_size: 1,
                            };
                            let algo = DbscanClustering::new(algo_config);
                            let mut state = DbscanState::default();
                            let _ = algo.cluster(&mut batch, &mut state);
                        }
                        Algorithm::Grid => {
                            let algo_config = rustpix_algorithms::GridConfig {
                                radius: 5.0,
                                temporal_window_ns: 75.0,
                                min_cluster_size: 1,
                                cell_size: 32,
                                max_cluster_size: None,
                            };
                            let algo = GridClustering::new(algo_config);
                            let mut state = GridState::default();
                            let _ = algo.cluster(&mut batch, &mut state);
                        }
                    }
                }

                for _ in 0..iterations {
                    // Re-create batch for each iteration to be fair?
                    // Or just assume batch population part of overhead?
                    // Old benchmark included `create_state`.
                    // We should include batch creation OR exclude it?
                    // The requirement compares "0.14s" which likely includes batch work.
                    // But strictly speaking clustering time is what we want.
                    // I will include batch creation to mimic real pipeline.
                    let start = Instant::now();

                    let mut batch = HitBatch::with_capacity(hits.len());
                    for hit in &hits {
                        batch.push(hit.x, hit.y, hit.tof, hit.tot, hit.timestamp, hit.chip_id);
                    }

                    match algo_enum {
                        Algorithm::Abs => {
                            let algo_config = rustpix_algorithms::AbsConfig {
                                radius: 5.0,
                                neutron_correlation_window_ns: 75.0,
                                min_cluster_size: 1,
                                scan_interval: 100,
                            };
                            let algo = AbsClustering::new(algo_config);
                            let mut state = AbsState::default();
                            let _ = algo.cluster(&mut batch, &mut state)?;
                        }
                        Algorithm::Dbscan => {
                            let algo_config = rustpix_algorithms::DbscanConfig {
                                epsilon: 5.0,
                                temporal_window_ns: 75.0,
                                min_points: 2,
                                min_cluster_size: 1,
                            };
                            let algo = DbscanClustering::new(algo_config);
                            let mut state = DbscanState::default();
                            let _ = algo.cluster(&mut batch, &mut state)?;
                        }
                        Algorithm::Grid => {
                            let algo_config = rustpix_algorithms::GridConfig {
                                radius: 5.0,
                                temporal_window_ns: 75.0,
                                min_cluster_size: 1,
                                cell_size: 32,
                                max_cluster_size: None,
                            };
                            let algo = GridClustering::new(algo_config);
                            let mut state = GridState::default();
                            let _ = algo.cluster(&mut batch, &mut state)?;
                        }
                    }

                    times.push(start.elapsed().as_secs_f64() * 1000.0);
                }

                let min_time = times.iter().fold(f64::INFINITY, |a, &b| a.min(b));
                let max_time = times.iter().fold(f64::NEG_INFINITY, |a, &b| a.max(b));
                let mean_time = times.iter().sum::<f64>() / times.len() as f64;

                println!(
                    "{:<10} | {:<15.2} | {:<15.2} | {:<15.2}",
                    name, mean_time, min_time, max_time
                );
            }
        }

        Commands::OrderingBenchmark { input } => {
            println!("Benchmarking ordering strategies on: {}", input.display());
            let reader = Tpx3FileReader::open(&input)?;

            // 1. Standard approach (Parallel Load + Unstable Sort)
            let start = Instant::now();
            let hits_std = reader.read_hits()?;
            let time_std = start.elapsed();
            println!(
                "Standard (Load + Sort): {:.2?} ({} hits)",
                time_std,
                hits_std.len()
            );

            // 2. Streaming approach (Pulse-based Merge)
            let start = Instant::now();
            let hits_stream = reader.read_hits_time_ordered()?;
            let time_stream = start.elapsed();
            println!(
                "Streaming (K-Way Merge): {:.2?} ({} hits)",
                time_stream,
                hits_stream.len()
            );

            let diff = (time_stream.as_secs_f64() / time_std.as_secs_f64() - 1.0) * 100.0;

            println!("Performance Delta: {:+.2}%", diff);

            // Verify hit counts match
            if !hits_std.is_empty() {
                assert_eq!(hits_std.len(), hits_stream.len(), "Hit counts must match");
                println!("Hit counts match: {}", hits_std.len());
            }
        }
    }

    Ok(())
}
