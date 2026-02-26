//!
//! This binary will provide a CLI for processing pixel detector data.

use clap::{Parser, Subcommand, ValueEnum};

use rustpix_algorithms::{cluster_and_extract_batch, AlgorithmParams, ClusteringAlgorithm};
use rustpix_algorithms::{
    AbsClustering, AbsState, DbscanClustering, DbscanState, GridClustering, GridState,
};
use rustpix_core::clustering::ClusteringConfig;
use rustpix_core::extraction::ExtractionConfig;
use rustpix_core::neutron::NeutronBatch;
use rustpix_core::soa::HitBatch;
use rustpix_io::hdf5::{Hdf5NeutronSink, NeutronEventBatch, NeutronWriteOptions};
use rustpix_io::hdf5_sns::{SnsEventSink, SnsRunMetadata, SnsWriteOptions};
use rustpix_io::{out_of_core_neutron_stream, OutOfCoreConfig, Tpx3FileReader};
use std::fs::File as StdFile;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;
use thiserror::Error;
use tiff::encoder::colortype::{Gray16, Gray32};
use tiff::encoder::TiffEncoder as TiffFileEncoder;
use tiff::tags::Tag;

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

    #[error("HDF5 error: {0}")]
    Hdf5(#[from] hdf5::Error),

    #[error("TIFF error: {0}")]
    Tiff(#[from] tiff::TiffError),

    #[error("{0}")]
    Other(String),
}

/// Resolved output format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OutputFormat {
    Csv,
    Binary,
    Hdf5,
    SnsHdf5,
    Tiff,
}

/// TIFF bit depth for histogram export.
#[derive(Debug, Clone, Copy, ValueEnum)]
enum BitDepth {
    /// 16-bit unsigned integer
    #[value(name = "16")]
    Bit16,
    /// 32-bit unsigned integer
    #[value(name = "32")]
    Bit32,
}

/// Output format override (auto-detected from file extension when omitted).
#[derive(Debug, Clone, Copy, ValueEnum)]
enum Format {
    /// Comma-separated values
    Csv,
    /// Compact binary
    Binary,
    /// Generic `NeXus` HDF5
    Hdf5,
    /// ORNL SNS `NXsnsevent` HDF5
    #[value(name = "sns-hdf5")]
    SnsHdf5,
    /// TIFF image stack
    Tiff,
}

impl From<Format> for OutputFormat {
    fn from(f: Format) -> Self {
        match f {
            Format::Csv => OutputFormat::Csv,
            Format::Binary => OutputFormat::Binary,
            Format::Hdf5 => OutputFormat::Hdf5,
            Format::SnsHdf5 => OutputFormat::SnsHdf5,
            Format::Tiff => OutputFormat::Tiff,
        }
    }
}

/// Instrument preset for SNS HDF5 export.
#[derive(Debug, Clone, Copy, ValueEnum)]
enum Instrument {
    /// VENUS (BL10) imaging beamline
    Venus,
}

fn detect_output_format(path: &Path) -> OutputFormat {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    // Check for .nxs.h5 (SNS convention) before .h5
    if name.ends_with(".nxs.h5") {
        return OutputFormat::SnsHdf5;
    }
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase);
    match ext.as_deref() {
        Some("h5" | "hdf5" | "nxs") => OutputFormat::Hdf5,
        Some("tif" | "tiff") => OutputFormat::Tiff,
        Some("csv") => OutputFormat::Csv,
        _ => OutputFormat::Binary,
    }
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

        /// Output format (auto-detected from file extension if omitted)
        #[arg(short = 'f', long, value_enum)]
        format: Option<Format>,

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

        /// Enable out-of-core processing (pulse-bounded)
        #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
        out_of_core: bool,

        /// Fraction of available memory to target for out-of-core processing
        #[arg(long, default_value = "0.5")]
        memory_fraction: f64,

        /// Explicit memory budget in bytes (overrides `memory_fraction`)
        #[arg(long)]
        memory_budget_bytes: Option<usize>,

        /// Worker threads for out-of-core slice processing
        #[arg(long)]
        parallelism: Option<usize>,

        /// Bounded queue depth for out-of-core pipeline stages
        #[arg(long, default_value = "2")]
        queue_depth: usize,

        /// Enable async reader/worker pipeline
        #[arg(long, default_value_t = false, action = clap::ArgAction::Set)]
        async_io: bool,

        /// Run number for SNS HDF5 export
        #[arg(long)]
        run_number: Option<u32>,

        /// Experiment identifier for SNS HDF5 (e.g., "IPTS-35004")
        #[arg(long)]
        ipts: Option<String>,

        /// Instrument preset for SNS HDF5 export
        #[arg(long, value_enum, default_value = "venus")]
        instrument: Instrument,

        /// Number of TOF bins for TIFF histogram output
        #[arg(long, default_value = "200")]
        tof_bins: usize,

        /// Maximum TOF in 25ns ticks for TIFF histogram (auto-detect if omitted).
        /// Providing this value avoids a full extra clustering pass over the data.
        #[arg(long)]
        tof_max: Option<u32>,

        /// Bit depth for TIFF output (16 or 32)
        #[arg(long, value_enum, default_value = "16")]
        bit_depth: BitDepth,

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

    /// Benchmark out-of-core single vs multi-threaded processing
    OutOfCoreBenchmark {
        /// Input TPX3 file
        input: PathBuf,

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

        /// Number of benchmark iterations
        #[arg(short, long, default_value = "3")]
        iterations: usize,

        /// Fraction of available memory to target for out-of-core processing
        #[arg(long, default_value = "0.5")]
        memory_fraction: f64,

        /// Explicit memory budget in bytes (overrides `memory_fraction`)
        #[arg(long)]
        memory_budget_bytes: Option<usize>,

        /// Worker threads for out-of-core slice processing
        #[arg(long)]
        parallelism: Option<usize>,

        /// Bounded queue depth for out-of-core pipeline stages
        #[arg(long, default_value = "2")]
        queue_depth: usize,

        /// Enable async reader/worker pipeline
        #[arg(long, default_value_t = false, action = clap::ArgAction::Set)]
        async_io: bool,
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
            format,
            algorithm,
            radius,
            temporal_window_ns,
            min_cluster_size,
            out_of_core,
            memory_fraction,
            memory_budget_bytes,
            parallelism,
            queue_depth,
            async_io,
            run_number,
            ipts,
            instrument,
            tof_bins,
            tof_max,
            bit_depth,
            verbose,
        } => run_process(
            &input,
            &output,
            format,
            algorithm,
            radius,
            temporal_window_ns,
            min_cluster_size,
            out_of_core,
            memory_fraction,
            memory_budget_bytes,
            parallelism,
            queue_depth,
            async_io,
            run_number,
            ipts.as_deref(),
            instrument,
            tof_bins,
            tof_max,
            bit_depth,
            verbose,
        ),

        Commands::Info { input } => run_info(&input),

        Commands::Benchmark { input, iterations } => run_benchmark(&input, iterations),

        Commands::OutOfCoreBenchmark {
            input,
            algorithm,
            radius,
            temporal_window_ns,
            min_cluster_size,
            iterations,
            memory_fraction,
            memory_budget_bytes,
            parallelism,
            queue_depth,
            async_io,
        } => run_out_of_core_benchmark(
            &input,
            algorithm,
            radius,
            temporal_window_ns,
            min_cluster_size,
            iterations,
            memory_fraction,
            memory_budget_bytes,
            parallelism,
            queue_depth,
            async_io,
        ),

        Commands::OrderingBenchmark => run_ordering_benchmark(),
    }
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
fn run_process(
    input: &[PathBuf],
    output: &PathBuf,
    format_override: Option<Format>,
    algorithm: Algorithm,
    radius: f64,
    temporal_window_ns: f64,
    min_cluster_size: u16,
    out_of_core: bool,
    memory_fraction: f64,
    memory_budget_bytes: Option<usize>,
    parallelism: Option<usize>,
    queue_depth: usize,
    async_io: bool,
    run_number: Option<u32>,
    ipts: Option<&str>,
    instrument: Instrument,
    tof_bins: usize,
    tof_max: Option<u32>,
    bit_depth: BitDepth,
    verbose: bool,
) -> Result<()> {
    if verbose {
        eprintln!("Processing {} file(s)...", input.len());
        eprintln!("Algorithm: {algorithm:?}");
        eprintln!("Radius: {radius} pixels");
        eprintln!("Temporal window: {temporal_window_ns} ns");
        eprintln!("Min cluster size: {min_cluster_size}");
        eprintln!("Out-of-core: {out_of_core}");
        if out_of_core {
            eprintln!("Memory fraction: {memory_fraction}");
            if let Some(bytes) = memory_budget_bytes {
                eprintln!("Memory budget override: {bytes} bytes");
            }
            if let Some(threads) = parallelism {
                eprintln!("Parallelism: {threads} threads");
            }
            eprintln!("Queue depth: {queue_depth}");
            eprintln!("Async IO: {async_io}");
        }
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
    let format = format_override.map_or_else(|| detect_output_format(output), OutputFormat::from);

    if verbose {
        eprintln!("Writing output to: {}", output.display());
        eprintln!("Output format: {format:?}");
    }

    let ooc_config = build_ooc_config(
        out_of_core,
        memory_fraction,
        memory_budget_bytes,
        parallelism,
        queue_depth,
        async_io,
    );

    let (total_hits, total_neutrons) = match format {
        OutputFormat::Csv | OutputFormat::Binary => run_process_flat(
            input,
            output,
            format,
            algo,
            &clustering,
            &extraction,
            &params,
            out_of_core,
            &ooc_config,
            verbose,
        )?,
        OutputFormat::Hdf5 => run_process_hdf5(
            input,
            output,
            algo,
            &clustering,
            &extraction,
            &params,
            out_of_core,
            &ooc_config,
            verbose,
        )?,
        OutputFormat::SnsHdf5 => run_process_sns_hdf5(
            input,
            output,
            algo,
            &clustering,
            &extraction,
            &params,
            out_of_core,
            &ooc_config,
            run_number,
            ipts,
            instrument,
            verbose,
        )?,
        OutputFormat::Tiff => run_process_tiff(
            input,
            output,
            algo,
            &clustering,
            &extraction,
            &params,
            out_of_core,
            &ooc_config,
            tof_bins,
            tof_max,
            bit_depth,
            verbose,
        )?,
    };

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

fn build_ooc_config(
    out_of_core: bool,
    memory_fraction: f64,
    memory_budget_bytes: Option<usize>,
    parallelism: Option<usize>,
    queue_depth: usize,
    async_io: bool,
) -> OutOfCoreConfig {
    if !out_of_core {
        return OutOfCoreConfig::default();
    }
    let mut config = OutOfCoreConfig::default().with_memory_fraction(memory_fraction);
    if let Some(bytes) = memory_budget_bytes {
        config = config.with_memory_budget_bytes(bytes);
    }
    if let Some(threads) = parallelism {
        config = config.with_parallelism(threads);
    }
    config.with_queue_depth(queue_depth).with_async_io(async_io)
}

/// Process files and write output as CSV or binary.
#[allow(clippy::too_many_arguments)]
fn run_process_flat(
    input: &[PathBuf],
    output: &PathBuf,
    format: OutputFormat,
    algo: ClusteringAlgorithm,
    clustering: &ClusteringConfig,
    extraction: &ExtractionConfig,
    params: &AlgorithmParams,
    out_of_core: bool,
    ooc_config: &OutOfCoreConfig,
    verbose: bool,
) -> Result<(usize, usize)> {
    let mut writer = rustpix_io::DataFileWriter::create(output)?;
    let mut wrote_header = false;
    let mut total_hits = 0usize;
    let mut total_neutrons = 0usize;

    for path in input {
        if verbose {
            eprintln!("Reading: {}", path.display());
        }
        let reader = Tpx3FileReader::open(path)?;

        if out_of_core {
            let stream = out_of_core_neutron_stream(
                &reader, algo, clustering, extraction, params, ooc_config,
            )?;
            for batch in stream {
                let batch = batch?;
                total_hits = total_hits.saturating_add(batch.hits_processed);
                total_neutrons = total_neutrons.saturating_add(batch.neutrons.len());
                write_neutrons_flat(&mut writer, format, &batch.neutrons, &mut wrote_header)?;
            }
        } else {
            let stream = reader.stream_time_ordered()?;
            for mut batch in stream {
                total_hits = total_hits.saturating_add(batch.len());
                let neutrons =
                    cluster_and_extract_batch(&mut batch, algo, clustering, extraction, params)?;
                total_neutrons = total_neutrons.saturating_add(neutrons.len());
                write_neutrons_flat(&mut writer, format, &neutrons, &mut wrote_header)?;
            }
        }

        if verbose {
            eprintln!("  Cumulative: {total_hits} hits, {total_neutrons} neutrons");
        }
    }

    Ok((total_hits, total_neutrons))
}

fn write_neutrons_flat(
    writer: &mut rustpix_io::DataFileWriter,
    format: OutputFormat,
    neutrons: &rustpix_core::neutron::NeutronBatch,
    wrote_header: &mut bool,
) -> Result<()> {
    match format {
        OutputFormat::Csv => {
            writer.write_neutron_batch_csv(neutrons, !*wrote_header)?;
            *wrote_header = true;
        }
        _ => {
            writer.write_neutron_batch_binary(neutrons)?;
        }
    }
    Ok(())
}

/// Process files and write output as generic `NeXus` HDF5.
#[allow(clippy::too_many_arguments)]
fn run_process_hdf5(
    input: &[PathBuf],
    output: &PathBuf,
    algo: ClusteringAlgorithm,
    clustering: &ClusteringConfig,
    extraction: &ExtractionConfig,
    params: &AlgorithmParams,
    out_of_core: bool,
    ooc_config: &OutOfCoreConfig,
    verbose: bool,
) -> Result<(usize, usize)> {
    let options = NeutronWriteOptions {
        x_size: 514,
        y_size: 514,
        super_resolution_factor: extraction.super_resolution_factor,
        chunk_events: 100_000,
        compression: Some(1),
        shuffle: true,
        flight_path_m: None,
        tof_offset_ns: None,
        energy_axis_kind: Some("tof".to_string()),
        include_xy: true,
        include_tot: true,
        include_chip_id: true,
        include_n_hits: true,
    };
    let mut sink = Hdf5NeutronSink::create(output, options)?;
    let mut total_hits = 0usize;
    let mut total_neutrons = 0usize;

    for path in input {
        if verbose {
            eprintln!("Reading: {}", path.display());
        }
        let reader = Tpx3FileReader::open(path)?;

        if out_of_core {
            let stream = out_of_core_neutron_stream(
                &reader, algo, clustering, extraction, params, ooc_config,
            )?;
            for batch in stream {
                let batch = batch?;
                total_hits = total_hits.saturating_add(batch.hits_processed);
                total_neutrons = total_neutrons.saturating_add(batch.neutrons.len());
                let event_batch = NeutronEventBatch {
                    tdc_timestamp_25ns: batch.tdc_timestamp_25ns,
                    neutrons: batch.neutrons,
                };
                sink.write_neutrons(&event_batch)?;
            }
        } else {
            let stream = reader.stream_time_ordered_events()?;
            for event in stream {
                total_hits = total_hits.saturating_add(event.hits.len());
                let mut hits = event.hits;
                let neutrons =
                    cluster_and_extract_batch(&mut hits, algo, clustering, extraction, params)?;
                total_neutrons = total_neutrons.saturating_add(neutrons.len());
                let event_batch = NeutronEventBatch {
                    tdc_timestamp_25ns: event.tdc_timestamp_25ns,
                    neutrons,
                };
                sink.write_neutrons(&event_batch)?;
            }
        }

        if verbose {
            eprintln!("  Cumulative: {total_hits} hits, {total_neutrons} neutrons");
        }
    }

    drop(sink);
    Ok((total_hits, total_neutrons))
}

/// Process files and write output as ORNL SNS `NXsnsevent` HDF5.
#[allow(clippy::too_many_arguments)]
fn run_process_sns_hdf5(
    input: &[PathBuf],
    output: &PathBuf,
    algo: ClusteringAlgorithm,
    clustering: &ClusteringConfig,
    extraction: &ExtractionConfig,
    params: &AlgorithmParams,
    out_of_core: bool,
    ooc_config: &OutOfCoreConfig,
    run_number: Option<u32>,
    ipts: Option<&str>,
    instrument: Instrument,
    verbose: bool,
) -> Result<(usize, usize)> {
    let run_meta = SnsRunMetadata {
        run_number: run_number.unwrap_or(0),
        experiment_identifier: ipts.unwrap_or("").to_string(),
        start_time: iso8601_now(),
        end_time: None,
        duration: None,
        proton_charge: None,
        title: None,
    };

    let mut write_options = match instrument {
        Instrument::Venus => SnsWriteOptions::venus_defaults(run_meta),
    };
    write_options.super_resolution_factor = extraction.super_resolution_factor;
    let mut sink = SnsEventSink::create(output, write_options)
        .map_err(|e| CliError::Other(format!("Failed to create SNS HDF5: {e}")))?;

    let mut total_hits = 0usize;
    let mut total_neutrons = 0usize;

    // TDC rebase state: each TPX3 file has an independent TDC clock, so when
    // concatenating multiple files the timestamps must be normalised to each
    // file's first-pulse baseline and then offset to stay monotonically
    // increasing across file boundaries.
    //
    //   rebased = (raw_tdc − file_base) + tdc_offset
    //
    // This avoids inflating event_time_zero/duration when a file's device
    // clock starts from a large non-zero value.
    let mut tdc_offset: u64 = 0;
    let mut last_tdc_seen: u64 = 0;
    let mut is_first_file = true;

    for path in input {
        if verbose {
            eprintln!("Reading: {}", path.display());
        }

        if !is_first_file {
            tdc_offset = last_tdc_seen + 1;
        }
        is_first_file = false;

        // Will be set to the first raw TDC timestamp of this file so that
        // all timestamps in the file are relative to its own start.
        let mut file_base_tdc: Option<u64> = None;

        let reader = Tpx3FileReader::open(path)?;

        if out_of_core {
            let stream = out_of_core_neutron_stream(
                &reader, algo, clustering, extraction, params, ooc_config,
            )?;
            for batch in stream {
                let batch = batch?;
                total_hits = total_hits.saturating_add(batch.hits_processed);
                total_neutrons = total_neutrons.saturating_add(batch.neutrons.len());
                if batch.neutrons.is_empty() {
                    continue;
                }
                let base = *file_base_tdc.get_or_insert(batch.tdc_timestamp_25ns);
                let rebased_tdc = (batch.tdc_timestamp_25ns - base).saturating_add(tdc_offset);
                last_tdc_seen = last_tdc_seen.max(rebased_tdc);
                let event_batch = NeutronEventBatch {
                    tdc_timestamp_25ns: rebased_tdc,
                    neutrons: batch.neutrons,
                };
                sink.write_neutrons(0, &event_batch)
                    .map_err(|e| CliError::Other(format!("Failed writing SNS neutrons: {e}")))?;
            }
        } else {
            let stream = reader.stream_time_ordered_events()?;
            for event in stream {
                total_hits = total_hits.saturating_add(event.hits.len());
                let mut hits = event.hits;
                let neutrons =
                    cluster_and_extract_batch(&mut hits, algo, clustering, extraction, params)?;
                total_neutrons = total_neutrons.saturating_add(neutrons.len());
                if neutrons.is_empty() {
                    continue;
                }
                let base = *file_base_tdc.get_or_insert(event.tdc_timestamp_25ns);
                let rebased_tdc = (event.tdc_timestamp_25ns - base).saturating_add(tdc_offset);
                last_tdc_seen = last_tdc_seen.max(rebased_tdc);
                let event_batch = NeutronEventBatch {
                    tdc_timestamp_25ns: rebased_tdc,
                    neutrons,
                };
                sink.write_neutrons(0, &event_batch)
                    .map_err(|e| CliError::Other(format!("Failed writing SNS neutrons: {e}")))?;
            }
        }

        if verbose {
            eprintln!("  Cumulative: {total_hits} hits, {total_neutrons} neutrons");
        }
    }

    sink.finalize()
        .map_err(|e| CliError::Other(format!("Failed to finalize SNS HDF5: {e}")))?;
    Ok((total_hits, total_neutrons))
}

/// Process files and write output as a TIFF stack with spectrum CSV.
///
/// When `--tof-max` is provided, accumulation happens in a single streaming
/// pass (constant memory). When auto-detecting, a lightweight first pass
/// scans for the maximum TOF value and then a second pass accumulates.
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
fn run_process_tiff(
    input: &[PathBuf],
    output: &Path,
    algo: ClusteringAlgorithm,
    clustering: &ClusteringConfig,
    extraction: &ExtractionConfig,
    params: &AlgorithmParams,
    out_of_core: bool,
    ooc_config: &OutOfCoreConfig,
    tof_bins: usize,
    tof_max_override: Option<u32>,
    bit_depth: BitDepth,
    verbose: bool,
) -> Result<(usize, usize)> {
    if tof_bins == 0 {
        return Err(CliError::Other("--tof-bins must be >= 1".to_string()));
    }
    if tof_max_override == Some(0) {
        return Err(CliError::Other("--tof-max must be >= 1".to_string()));
    }

    let width: usize = 514;
    let height: usize = 514;

    // Determine tof_max.  If not provided, scan once to find it.
    let tof_max = if let Some(v) = tof_max_override {
        v
    } else {
        if verbose {
            eprintln!("Scanning for TOF range ...");
        }
        let mut observed: u32 = 0;
        for path in input {
            let reader = Tpx3FileReader::open(path)?;
            if out_of_core {
                let stream = out_of_core_neutron_stream(
                    &reader, algo, clustering, extraction, params, ooc_config,
                )?;
                for batch in stream {
                    let batch = batch?;
                    for &t in &batch.neutrons.tof {
                        observed = observed.max(t);
                    }
                }
            } else {
                let stream = reader.stream_time_ordered()?;
                for mut batch in stream {
                    let neutrons = cluster_and_extract_batch(
                        &mut batch, algo, clustering, extraction, params,
                    )?;
                    for &t in &neutrons.tof {
                        observed = observed.max(t);
                    }
                }
            }
        }
        observed.max(1)
    };

    if verbose {
        eprintln!("TOF max: {tof_max} (25ns ticks)");
        eprintln!("TOF bins: {tof_bins}");
    }

    // Streaming accumulation pass — histogram only, no batch storage.
    #[allow(clippy::cast_precision_loss)]
    let bin_width = f64::from(tof_max) / tof_bins as f64;
    let mut data = vec![0u64; tof_bins * height * width];
    let mut total_hits = 0usize;
    let mut total_neutrons = 0usize;

    for path in input {
        if verbose {
            eprintln!("Reading: {}", path.display());
        }
        let reader = Tpx3FileReader::open(path)?;

        if out_of_core {
            let stream = out_of_core_neutron_stream(
                &reader, algo, clustering, extraction, params, ooc_config,
            )?;
            for batch in stream {
                let batch = batch?;
                total_hits = total_hits.saturating_add(batch.hits_processed);
                total_neutrons = total_neutrons.saturating_add(batch.neutrons.len());
                accumulate_neutrons_into_histogram(
                    &mut data,
                    &batch.neutrons,
                    tof_bins,
                    width,
                    height,
                    bin_width,
                    extraction.super_resolution_factor,
                );
            }
        } else {
            let stream = reader.stream_time_ordered()?;
            for mut batch in stream {
                total_hits = total_hits.saturating_add(batch.len());
                let neutrons =
                    cluster_and_extract_batch(&mut batch, algo, clustering, extraction, params)?;
                total_neutrons = total_neutrons.saturating_add(neutrons.len());
                accumulate_neutrons_into_histogram(
                    &mut data,
                    &neutrons,
                    tof_bins,
                    width,
                    height,
                    bin_width,
                    extraction.super_resolution_factor,
                );
            }
        }

        if verbose {
            eprintln!("  Cumulative: {total_hits} hits, {total_neutrons} neutrons");
        }
    }

    if verbose {
        eprintln!("Writing TIFF stack to: {}", output.display());
    }

    // Write TIFF stack.
    write_tiff_stack_file(output, &data, tof_bins, width, height, bit_depth)?;

    // Write spectrum CSV alongside the TIFF, unless the CSV path collides
    // with the TIFF output (e.g. user passed `-o foo.csv -f tiff`).
    let spectrum_path = output.with_extension("csv");
    if spectrum_path == output {
        eprintln!("Warning: skipping spectrum CSV — output path already has .csv extension");
    } else {
        write_spectrum_csv(&spectrum_path, &data, tof_bins, width, height, bin_width)?;
    }
    if verbose {
        eprintln!("Wrote spectrum to: {}", spectrum_path.display());
    }

    Ok((total_hits, total_neutrons))
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn accumulate_neutrons_into_histogram(
    data: &mut [u64],
    batch: &NeutronBatch,
    n_tof_bins: usize,
    width: usize,
    height: usize,
    bin_width: f64,
    super_resolution_factor: f64,
) {
    let inv = 1.0 / super_resolution_factor;
    for i in 0..batch.len() {
        let x = (batch.x[i] * inv).round();
        let y = (batch.y[i] * inv).round();
        let tof = batch.tof[i];

        if x < 0.0 || y < 0.0 {
            continue;
        }
        let x = x as usize;
        let y = y as usize;

        let tof_bin = if bin_width > 0.0 {
            let bin = (f64::from(tof) / bin_width) as usize;
            bin.min(n_tof_bins.saturating_sub(1))
        } else {
            0
        };

        if x < width && y < height && tof_bin < n_tof_bins {
            let idx = tof_bin * height * width + y * width + x;
            data[idx] += 1;
        }
    }
}

fn write_tiff_stack_file(
    path: &Path,
    data: &[u64],
    n_bins: usize,
    width: usize,
    height: usize,
    bit_depth: BitDepth,
) -> Result<()> {
    let w =
        u32::try_from(width).map_err(|_| CliError::Other("Image width exceeds u32".to_string()))?;
    let h = u32::try_from(height)
        .map_err(|_| CliError::Other("Image height exceeds u32".to_string()))?;

    let file = StdFile::create(path)?;
    let mut encoder = TiffFileEncoder::new_big(file)?;

    let description =
        format!("ImageJ=1.53\nimages={n_bins}\nslices={n_bins}\nhyperstack=true\nmode=grayscale\n");

    let xy_size = height * width;
    let mut clamped = 0usize;
    for tof in 0..n_bins {
        let start = tof * xy_size;
        let end = start + xy_size;
        let slice = &data[start..end];

        match bit_depth {
            BitDepth::Bit16 => {
                let pixels = convert_slice_u16(slice, &mut clamped);
                let mut image = encoder.new_image::<Gray16>(w, h)?;
                if tof == 0 {
                    image
                        .encoder()
                        .write_tag(Tag::ImageDescription, description.as_str())?;
                }
                image.write_data(&pixels)?;
            }
            BitDepth::Bit32 => {
                let pixels = convert_slice_u32(slice, &mut clamped);
                let mut image = encoder.new_image::<Gray32>(w, h)?;
                if tof == 0 {
                    image
                        .encoder()
                        .write_tag(Tag::ImageDescription, description.as_str())?;
                }
                image.write_data(&pixels)?;
            }
        }
    }

    if clamped > 0 {
        eprintln!(
            "Warning: {clamped} pixel value(s) clamped to {bit_depth:?} max during TIFF export"
        );
    }

    Ok(())
}

fn convert_slice_u16(counts: &[u64], clamped: &mut usize) -> Vec<u16> {
    counts
        .iter()
        .map(|&v| {
            u16::try_from(v).unwrap_or_else(|_| {
                *clamped += 1;
                u16::MAX
            })
        })
        .collect()
}

fn convert_slice_u32(counts: &[u64], clamped: &mut usize) -> Vec<u32> {
    counts
        .iter()
        .map(|&v| {
            u32::try_from(v).unwrap_or_else(|_| {
                *clamped += 1;
                u32::MAX
            })
        })
        .collect()
}

fn write_spectrum_csv(
    path: &Path,
    data: &[u64],
    n_bins: usize,
    width: usize,
    height: usize,
    bin_width_25ns: f64,
) -> Result<()> {
    let xy_size = width * height;
    let mut file = BufWriter::new(StdFile::create(path)?);
    writeln!(file, "shutter_time,counts")?;
    for tof_bin in 0..n_bins {
        let start = tof_bin * xy_size;
        let end = start + xy_size;
        let count: u64 = data[start..end].iter().sum();
        // Time at bin center in seconds.
        #[allow(clippy::cast_precision_loss)]
        let time_ns = (tof_bin as f64 + 0.5) * bin_width_25ns * 25.0;
        let time_seconds = time_ns * 1.0e-9;
        writeln!(file, "{time_seconds:.6e},{count}")?;
    }
    Ok(())
}

/// Generate an ISO 8601 UTC timestamp for the current time.
fn iso8601_now() -> String {
    let dur = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let total_secs = dur.as_secs();
    let days = total_secs / 86400;
    let rem = total_secs % 86400;
    let hours = rem / 3600;
    let minutes = (rem % 3600) / 60;
    let seconds = rem % 60;
    let mut y = 1970i64;
    let mut d = i64::try_from(days).unwrap_or(0);
    loop {
        let year_days = if y % 4 == 0 && (y % 100 != 0 || y % 400 == 0) {
            366
        } else {
            365
        };
        if d < year_days {
            break;
        }
        d -= year_days;
        y += 1;
    }
    let leap = y % 4 == 0 && (y % 100 != 0 || y % 400 == 0);
    let month_days: [i64; 12] = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let mut m = 0usize;
    for (i, &md) in month_days.iter().enumerate() {
        if d < md {
            m = i;
            break;
        }
        d -= md;
    }
    format!(
        "{y:04}-{:02}-{:02}T{hours:02}:{minutes:02}:{seconds:02}Z",
        m + 1,
        d + 1,
    )
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

#[allow(clippy::too_many_arguments)]
fn run_out_of_core_benchmark(
    input: &PathBuf,
    algorithm: Algorithm,
    radius: f64,
    temporal_window_ns: f64,
    min_cluster_size: u16,
    iterations: usize,
    memory_fraction: f64,
    memory_budget_bytes: Option<usize>,
    parallelism: Option<usize>,
    queue_depth: usize,
    async_io: bool,
) -> Result<()> {
    let algo = resolve_algorithm(algorithm);
    let clustering = ClusteringConfig {
        radius,
        temporal_window_ns,
        min_cluster_size,
        max_cluster_size: None,
    };
    let extraction = ExtractionConfig::default();
    let params = AlgorithmParams::default();

    let mut single_config = OutOfCoreConfig::default().with_memory_fraction(memory_fraction);
    if let Some(bytes) = memory_budget_bytes {
        single_config = single_config.with_memory_budget_bytes(bytes);
    }

    let mut multi_config = single_config.clone().with_queue_depth(queue_depth);
    let threads = parallelism.unwrap_or_else(|| {
        std::thread::available_parallelism()
            .map(std::num::NonZeroUsize::get)
            .unwrap_or(1)
    });
    multi_config = multi_config.with_parallelism(threads);
    multi_config = multi_config.with_async_io(async_io);

    let mut single_total = std::time::Duration::ZERO;
    let mut multi_total = std::time::Duration::ZERO;
    for _ in 0..iterations {
        let (hits, neutrons, duration) = bench_out_of_core(
            input,
            algo,
            &clustering,
            &extraction,
            &params,
            &single_config,
        )?;
        single_total += duration;

        let (hits_mt, neutrons_mt, duration) = bench_out_of_core(
            input,
            algo,
            &clustering,
            &extraction,
            &params,
            &multi_config,
        )?;
        multi_total += duration;

        if hits != hits_mt || neutrons != neutrons_mt {
            eprintln!(
                "Warning: counts differ between single ({hits}, {neutrons}) and multi-thread ({hits_mt}, {neutrons_mt})"
            );
        }
    }

    let iterations_f64 = usize_to_f64(iterations);
    let single_avg = single_total.as_secs_f64() / iterations_f64;
    let multi_avg = multi_total.as_secs_f64() / iterations_f64;
    let speedup = single_avg / multi_avg.max(f64::EPSILON);

    println!("Out-of-core benchmark ({iterations} iterations)");
    println!("Single-thread avg: {single_avg:.3}s");
    println!(
        "Multi-thread avg: {:.3}s (threads: {}, async: {})",
        multi_avg,
        multi_config.effective_parallelism(),
        async_io
    );
    println!("Speedup: {speedup:.2}x");
    Ok(())
}

fn bench_out_of_core(
    input: &PathBuf,
    algo: ClusteringAlgorithm,
    clustering: &ClusteringConfig,
    extraction: &ExtractionConfig,
    params: &AlgorithmParams,
    config: &OutOfCoreConfig,
) -> Result<(usize, usize, std::time::Duration)> {
    let reader = Tpx3FileReader::open(input)?;
    let start = Instant::now();
    let stream = out_of_core_neutron_stream(&reader, algo, clustering, extraction, params, config)?;
    let mut total_hits = 0usize;
    let mut total_neutrons = 0usize;
    for batch in stream {
        let batch = batch?;
        total_hits = total_hits.saturating_add(batch.hits_processed);
        total_neutrons = total_neutrons.saturating_add(batch.neutrons.len());
    }
    Ok((total_hits, total_neutrons, start.elapsed()))
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
