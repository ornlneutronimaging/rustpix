//! rustpix-cli: Command-line interface for rustpix.
//!
//! This binary provides a CLI for processing pixel detector data.

use clap::{Parser, Subcommand, ValueEnum};
use rayon::prelude::*;
use rustpix_algorithms::{AbsClustering, DbscanClustering, GraphClustering, GridClustering};
use rustpix_core::{
    CentroidExtractor, ClusteringAlgorithm, ClusteringConfig, ExtractionConfig,
    WeightedCentroidExtractor,
};
use rustpix_io::{Tpx3FileReader, Tpx3FileWriter};
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

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

/// Clustering algorithm selection.
#[derive(Debug, Clone, Copy, ValueEnum)]
enum Algorithm {
    /// Adjacency-Based Search clustering
    Abs,
    /// DBSCAN clustering
    Dbscan,
    /// Graph-based clustering
    Graph,
    /// Grid-based clustering with spatial indexing
    Grid,
}

/// Output format selection.
#[derive(Debug, Clone, Copy, ValueEnum)]
enum OutputFormat {
    /// CSV format
    Csv,
    /// Binary format
    Binary,
    /// JSON format
    Json,
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

        /// Output format
        #[arg(short, long, value_enum, default_value = "csv")]
        format: OutputFormat,

        /// Spatial epsilon for clustering (pixels)
        #[arg(long, default_value = "1.5")]
        spatial_epsilon: f64,

        /// Temporal epsilon for clustering (time units)
        #[arg(long, default_value = "1000")]
        temporal_epsilon: u64,

        /// Minimum cluster size
        #[arg(long, default_value = "1")]
        min_cluster_size: usize,

        /// Maximum cluster size (optional)
        #[arg(long)]
        max_cluster_size: Option<usize>,

        /// Use ToT weighting for centroid calculation
        #[arg(long, default_value = "true")]
        tot_weighted: bool,

        /// Number of threads (0 for auto)
        #[arg(short = 'j', long, default_value = "0")]
        threads: usize,

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
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Process {
            input,
            output,
            algorithm,
            format,
            spatial_epsilon,
            temporal_epsilon,
            min_cluster_size,
            max_cluster_size,
            tot_weighted,
            threads,
            verbose,
        } => {
            // Configure thread pool
            if threads > 0 {
                rayon::ThreadPoolBuilder::new()
                    .num_threads(threads)
                    .build_global()
                    .ok();
            }

            let clustering_config = ClusteringConfig {
                spatial_epsilon,
                temporal_epsilon,
                min_cluster_size,
                max_cluster_size,
            };

            let extraction_config = ExtractionConfig::new().with_tot_weighted(tot_weighted);

            if verbose {
                eprintln!("Processing {} file(s)...", input.len());
                eprintln!("Algorithm: {:?}", algorithm);
                eprintln!("Spatial epsilon: {}", spatial_epsilon);
                eprintln!("Temporal epsilon: {}", temporal_epsilon);
            }

            let start = Instant::now();
            let mut all_centroids = Vec::new();

            for path in &input {
                if verbose {
                    eprintln!("Reading: {}", path.display());
                }

                let reader = Tpx3FileReader::open(path)?;
                let hits = reader.read_hits()?;

                if verbose {
                    eprintln!("  {} hits read", hits.len());
                }

                // Convert to HitData
                let hit_data: Vec<rustpix_core::HitData> =
                    hits.into_iter().map(|h| h.into()).collect();

                // Cluster
                let clusters = match algorithm {
                    Algorithm::Abs => {
                        AbsClustering::new().cluster(&hit_data, &clustering_config)?
                    }
                    Algorithm::Dbscan => {
                        DbscanClustering::new().cluster(&hit_data, &clustering_config)?
                    }
                    Algorithm::Graph => {
                        GraphClustering::new().cluster(&hit_data, &clustering_config)?
                    }
                    Algorithm::Grid => {
                        GridClustering::new().cluster(&hit_data, &clustering_config)?
                    }
                };

                if verbose {
                    eprintln!("  {} clusters found", clusters.len());
                }

                // Extract centroids
                let extractor = WeightedCentroidExtractor::new();
                let centroids: Vec<_> = clusters
                    .par_iter()
                    .filter_map(|c| extractor.extract(c, &extraction_config).ok())
                    .collect();

                all_centroids.extend(centroids);
            }

            let elapsed = start.elapsed();

            if verbose {
                eprintln!(
                    "Total: {} centroids extracted in {:.2}s",
                    all_centroids.len(),
                    elapsed.as_secs_f64()
                );
            }

            // Write output
            match format {
                OutputFormat::Csv => {
                    let mut writer = Tpx3FileWriter::create(&output)?;
                    writer.write_centroids_csv(&all_centroids)?;
                }
                OutputFormat::Binary => {
                    let mut writer = Tpx3FileWriter::create(&output)?;
                    writer.write_centroids_binary(&all_centroids)?;
                }
                OutputFormat::Json => {
                    let json = serde_json::to_string_pretty(
                        &all_centroids
                            .iter()
                            .map(|c| {
                                serde_json::json!({
                                    "x": c.x,
                                    "y": c.y,
                                    "toa": c.toa.as_u64(),
                                    "tot_sum": c.tot_sum,
                                    "cluster_size": c.cluster_size
                                })
                            })
                            .collect::<Vec<_>>(),
                    )?;
                    std::fs::write(&output, json)?;
                }
            }

            if verbose {
                eprintln!("Output written to: {}", output.display());
            }

            println!(
                "Processed {} centroids in {:.2}s",
                all_centroids.len(),
                elapsed.as_secs_f64()
            );
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
                let min_toa = hits.iter().map(|h| h.toa).min().unwrap();
                let max_toa = hits.iter().map(|h| h.toa).max().unwrap();
                println!("ToA range: {} - {}", min_toa, max_toa);

                let min_x = hits.iter().map(|h| h.x).min().unwrap();
                let max_x = hits.iter().map(|h| h.x).max().unwrap();
                let min_y = hits.iter().map(|h| h.y).min().unwrap();
                let max_y = hits.iter().map(|h| h.y).max().unwrap();
                println!("X range: {} - {}", min_x, max_x);
                println!("Y range: {} - {}", min_y, max_y);
            }
        }

        Commands::Benchmark { input, iterations } => {
            let reader = Tpx3FileReader::open(&input)?;
            let hits = reader.read_hits()?;
            let hit_data: Vec<rustpix_core::HitData> = hits.into_iter().map(|h| h.into()).collect();

            println!(
                "Benchmarking with {} hits, {} iterations",
                hit_data.len(),
                iterations
            );

            let config = ClusteringConfig::default();

            for (name, algorithm) in [
                (
                    "ABS",
                    Box::new(AbsClustering::new())
                        as Box<dyn ClusteringAlgorithm<rustpix_core::HitData>>,
                ),
                ("DBSCAN", Box::new(DbscanClustering::new())),
                ("Graph", Box::new(GraphClustering::new())),
                ("Grid", Box::new(GridClustering::new())),
            ] {
                let start = Instant::now();
                let mut cluster_count = 0;

                for _ in 0..iterations {
                    let clusters = algorithm.cluster(&hit_data, &config)?;
                    cluster_count = clusters.len();
                }

                let elapsed = start.elapsed();
                let per_iter = elapsed.as_secs_f64() / iterations as f64;
                let hits_per_sec = hit_data.len() as f64 / per_iter;

                println!(
                    "{:8}: {:.3}s/iter, {:.1}M hits/sec, {} clusters",
                    name,
                    per_iter,
                    hits_per_sec / 1_000_000.0,
                    cluster_count
                );
            }
        }
    }

    Ok(())
}
