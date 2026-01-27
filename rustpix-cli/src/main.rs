//!
//! This binary will provide a CLI for processing pixel detector data.

use clap::{Parser, Subcommand, ValueEnum};

use rustpix_algorithms::{cluster_and_extract_batch, AlgorithmParams, ClusteringAlgorithm};
use rustpix_algorithms::{
    AbsClustering, AbsState, DbscanClustering, DbscanState, GridClustering, GridState,
};
use rustpix_core::clustering::ClusteringConfig;
use rustpix_core::extraction::ExtractionConfig;
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

    /// Ordering benchmark (deprecated; no-op)
    OrderingBenchmark,
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
        } => run_process(
            &input,
            &output,
            algorithm,
            radius,
            temporal_window_ns,
            min_cluster_size,
            verbose,
        ),

        Commands::Info { input } => run_info(&input),

        Commands::Benchmark { input, iterations } => run_benchmark(&input, iterations),

        Commands::OrderingBenchmark => run_ordering_benchmark(),
    }
}

fn run_process(
    input: &[PathBuf],
    output: &PathBuf,
    algorithm: Algorithm,
    radius: f64,
    temporal_window_ns: f64,
    min_cluster_size: u16,
    verbose: bool,
) -> Result<()> {
    if verbose {
        eprintln!("Processing {} file(s)...", input.len());
        eprintln!("Algorithm: {algorithm:?}");
        eprintln!("Radius: {radius} pixels");
        eprintln!("Temporal window: {temporal_window_ns} ns");
        eprintln!("Min cluster size: {min_cluster_size}");
    }

    let start = Instant::now();
    let algo = resolve_algorithm(algorithm);
    let clustering = ClusteringConfig {
        radius,
        temporal_window_ns,
        min_cluster_size,
        max_cluster_size: None,
    };
    let extraction = ExtractionConfig::default();
    let params = AlgorithmParams::default();

    let mut writer = rustpix_io::DataFileWriter::create(output)?;
    if verbose {
        eprintln!("Writing output to: {}", output.display());
    }
    let output_format = output
        .extension()
        .and_then(|ext| ext.to_str())
        .map_or_else(|| "bin".to_string(), str::to_lowercase);
    let mut wrote_header = false;
    let mut warned_unknown = false;

    let mut total_hits = 0usize;
    let mut total_neutrons = 0usize;
    for path in input {
        if verbose {
            eprintln!("Reading: {}", path.display());
        }

        let (file_hits, file_neutrons) = process_input_file(
            path,
            algo,
            &clustering,
            &extraction,
            &params,
            &mut writer,
            &output_format,
            &mut wrote_header,
            &mut warned_unknown,
            verbose,
        )?;

        total_hits = total_hits.saturating_add(file_hits);
        total_neutrons = total_neutrons.saturating_add(file_neutrons);

        if verbose {
            eprintln!("  {file_hits} hits processed");
            eprintln!("  {file_neutrons} neutrons extracted");
        }
    }

    let elapsed = start.elapsed();
    println!(
        "Processed {} files in {:.2}s",
        input.len(),
        elapsed.as_secs_f64()
    );
    println!("Total hits: {total_hits}");
    println!("Total neutrons: {total_neutrons}");
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn process_input_file(
    path: &PathBuf,
    algo: ClusteringAlgorithm,
    clustering: &ClusteringConfig,
    extraction: &ExtractionConfig,
    params: &AlgorithmParams,
    writer: &mut rustpix_io::DataFileWriter,
    output_format: &str,
    wrote_header: &mut bool,
    warned_unknown: &mut bool,
    verbose: bool,
) -> Result<(usize, usize)> {
    let reader = Tpx3FileReader::open(path)?;
    let stream = reader.stream_time_ordered()?;
    let mut file_hits = 0usize;
    let mut file_neutrons = 0usize;

    for mut batch in stream {
        file_hits = file_hits.saturating_add(batch.len());
        let neutrons = cluster_and_extract_batch(&mut batch, algo, clustering, extraction, params)?;
        file_neutrons = file_neutrons.saturating_add(neutrons.len());

        match output_format {
            "csv" => {
                writer.write_neutron_batch_csv(&neutrons, !*wrote_header)?;
                *wrote_header = true;
            }
            "bin" | "dat" => {
                writer.write_neutron_batch_binary(&neutrons)?;
            }
            _ => {
                if verbose && !*warned_unknown {
                    eprintln!("Unknown extension '{output_format}', defaulting to binary");
                }
                *warned_unknown = true;
                writer.write_neutron_batch_binary(&neutrons)?;
            }
        }
    }

    Ok((file_hits, file_neutrons))
}

fn resolve_algorithm(algorithm: Algorithm) -> ClusteringAlgorithm {
    match algorithm {
        Algorithm::Abs => ClusteringAlgorithm::Abs,
        Algorithm::Dbscan => ClusteringAlgorithm::Dbscan,
        Algorithm::Grid => ClusteringAlgorithm::Grid,
    }
}

fn run_info(input: &PathBuf) -> Result<()> {
    let reader = Tpx3FileReader::open(input)?;
    let file_size = reader.file_size();
    let packet_count = reader.packet_count();

    println!("File: {}", input.display());
    println!(
        "Size: {} bytes ({:.2} MB)",
        file_size,
        usize_to_f64(file_size) / 1_000_000.0
    );
    println!("Packets: {packet_count}");

    let batch = reader.read_batch()?;
    println!("Hits: {}", batch.len());

    if !batch.is_empty() {
        let min_tof = batch.tof.iter().copied().min().unwrap();
        let max_tof = batch.tof.iter().copied().max().unwrap();
        println!("TOF range: {min_tof} - {max_tof}");

        let min_x = batch.x.iter().copied().min().unwrap();
        let max_x = batch.x.iter().copied().max().unwrap();
        let min_y = batch.y.iter().copied().min().unwrap();
        let max_y = batch.y.iter().copied().max().unwrap();
        println!("X range: {min_x} - {max_x}");
        println!("Y range: {min_y} - {max_y}");
    }

    Ok(())
}

fn run_benchmark(input: &PathBuf, iterations: usize) -> Result<()> {
    let reader = Tpx3FileReader::open(input)?;
    let base_batch = reader.read_batch()?;

    println!(
        "Benchmarking with {} hits, {} iterations",
        base_batch.len(),
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
        warmup_algorithm(algo_enum, &base_batch);
        let times = benchmark_algorithm(algo_enum, &base_batch, iterations)?;

        let min_time = times.iter().fold(f64::INFINITY, |a, &b| a.min(b));
        let max_time = times.iter().fold(f64::NEG_INFINITY, |a, &b| a.max(b));
        let mean_time = times.iter().sum::<f64>() / usize_to_f64(times.len());

        println!("{name:<10} | {mean_time:<15.2} | {min_time:<15.2} | {max_time:<15.2}");
    }

    Ok(())
}

fn warmup_algorithm(algo_enum: Algorithm, base_batch: &HitBatch) {
    let mut batch = base_batch.clone();
    let _ = run_cluster_once(algo_enum, &mut batch);
}

fn benchmark_algorithm(
    algo_enum: Algorithm,
    base_batch: &HitBatch,
    iterations: usize,
) -> Result<Vec<f64>> {
    let mut times = Vec::with_capacity(iterations);

    for _ in 0..iterations {
        let start = Instant::now();
        let mut batch = base_batch.clone();
        run_cluster_once(algo_enum, &mut batch)?;
        times.push(start.elapsed().as_secs_f64() * 1000.0);
    }

    Ok(times)
}

fn run_cluster_once(algo_enum: Algorithm, batch: &mut HitBatch) -> Result<()> {
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
            let _ = algo.cluster(batch, &mut state)?;
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
            let _ = algo.cluster(batch, &mut state)?;
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
            let _ = algo.cluster(batch, &mut state)?;
        }
    }
    Ok(())
}

#[allow(clippy::unnecessary_wraps)]
fn run_ordering_benchmark() -> Result<()> {
    println!("Ordering benchmark removed: read_batch now uses the time-ordered stream.");
    Ok(())
}

fn usize_to_f64(value: usize) -> f64 {
    #[allow(clippy::cast_precision_loss)]
    {
        value as f64
    }
}
