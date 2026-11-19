//!
//! This binary will provide a CLI for processing pixel detector data.

mod output;

use clap::{Parser, Subcommand, ValueEnum};

use output::{has_extension, same_file, PendingOutput};
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
use std::io::{BufRead, BufReader, BufWriter, Read, Write};
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

        /// Fraction of available memory to target for out-of-core processing [default: 0.5]
        #[arg(long)]
        memory_fraction: Option<f64>,

        /// Explicit memory budget in bytes (overrides `memory_fraction`)
        #[arg(long)]
        memory_budget_bytes: Option<usize>,

        /// Worker threads for out-of-core slice processing
        #[arg(long)]
        parallelism: Option<usize>,

        /// Bounded queue depth for out-of-core pipeline stages [default: 2]
        #[arg(long)]
        queue_depth: Option<usize>,

        /// Enable async reader/worker pipeline [default: false]
        #[arg(long, action = clap::ArgAction::Set)]
        async_io: Option<bool>,

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
    output: &Path,
    format_override: Option<Format>,
    algorithm: Algorithm,
    radius: f64,
    temporal_window_ns: f64,
    min_cluster_size: u16,
    out_of_core: bool,
    memory_fraction: Option<f64>,
    memory_budget_bytes: Option<usize>,
    parallelism: Option<usize>,
    queue_depth: Option<usize>,
    async_io: Option<bool>,
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
    )?;
    if verbose && out_of_core {
        eprintln!("Memory fraction: {}", ooc_config.memory_fraction);
        if let Some(bytes) = ooc_config.memory_budget_bytes {
            eprintln!("Memory budget override: {bytes} bytes");
        }
        if let Some(threads) = ooc_config.parallelism {
            eprintln!("Parallelism: {threads} threads");
        }
        eprintln!("Queue depth: {}", ooc_config.queue_depth);
        eprintln!("Async IO: {}", ooc_config.async_io);
    }

    check_paths(input, output, format)?;

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
    memory_fraction: Option<f64>,
    memory_budget_bytes: Option<usize>,
    parallelism: Option<usize>,
    queue_depth: Option<usize>,
    async_io: Option<bool>,
) -> Result<OutOfCoreConfig> {
    let mut config = OutOfCoreConfig::default();
    if !out_of_core {
        let given: Vec<&str> = [
            ("--memory-fraction", memory_fraction.is_some()),
            ("--memory-budget-bytes", memory_budget_bytes.is_some()),
            ("--parallelism", parallelism.is_some()),
            ("--queue-depth", queue_depth.is_some()),
            ("--async-io", async_io.is_some()),
        ]
        .into_iter()
        .filter_map(|(name, set)| set.then_some(name))
        .collect();
        if !given.is_empty() {
            return Err(CliError::Other(format!(
                "{} only apply with --out-of-core true",
                given.join(", ")
            )));
        }
        return Ok(config);
    }
    if let Some(fraction) = memory_fraction {
        config = config.with_memory_fraction(fraction);
    }
    if let Some(bytes) = memory_budget_bytes {
        config = config.with_memory_budget_bytes(bytes);
    }
    if let Some(threads) = parallelism {
        config = config.with_parallelism(threads);
    }
    if let Some(depth) = queue_depth {
        config = config.with_queue_depth(depth);
    }
    if let Some(enabled) = async_io {
        config = config.with_async_io(enabled);
    }
    Ok(config)
}

/// Opens a TPX3 input, rejecting files that hold no TPX3 section header.
fn open_input(path: &Path) -> Result<Tpx3FileReader> {
    let reader = Tpx3FileReader::open(path)
        .map_err(|e| CliError::Other(format!("cannot open {}: {e}", path.display())))?;
    if !reader.iter_packets().any(|packet| packet.is_header()) {
        return Err(CliError::Other(format!(
            "{}: no TPX3 section header found; not a TPX3 file",
            path.display()
        )));
    }
    Ok(reader)
}

/// Checks every input and refuses outputs that would overwrite raw data or an unrelated file.
fn check_paths(input: &[PathBuf], output: &Path, format: OutputFormat) -> Result<()> {
    if has_extension(output, "tpx3") {
        return Err(CliError::Other(format!(
            "refusing to write output to {}: .tpx3 is the raw-data extension",
            output.display()
        )));
    }
    let sidecar = if format == OutputFormat::Tiff {
        spectrum_sidecar_path(output)
    } else {
        None
    };
    for path in input {
        open_input(path)?;
        for target in std::iter::once(output).chain(sidecar.as_deref()) {
            if same_file(path, target) {
                return Err(CliError::Other(format!(
                    "output {} is the input file {}",
                    target.display(),
                    path.display()
                )));
            }
        }
    }
    if let Some(sidecar) = &sidecar {
        ensure_spectrum_replaceable(sidecar)?;
    }
    Ok(())
}

const SPECTRUM_HEADER: &str = "shutter_time,counts";

/// Spectrum CSV path beside a TIFF output, or `None` when it would be the TIFF itself.
fn spectrum_sidecar_path(output: &Path) -> Option<PathBuf> {
    (!has_extension(output, "csv")).then(|| output.with_extension("csv"))
}

/// Errors when `path` holds anything other than a spectrum this tool wrote.
fn ensure_spectrum_replaceable(path: &Path) -> Result<()> {
    let file = match StdFile::open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e.into()),
    };
    if file.metadata()?.len() == 0 {
        return Ok(());
    }
    let mut first_line = String::new();
    let readable = BufReader::new(file.take(256))
        .read_line(&mut first_line)
        .is_ok();
    if readable && first_line.trim_end() == SPECTRUM_HEADER {
        return Ok(());
    }
    Err(CliError::Other(format!(
        "refusing to overwrite {}: it is not a spectrum file written by rustpix; \
         move it or choose another output name",
        path.display()
    )))
}

/// Process files and write output as CSV or binary.
#[allow(clippy::too_many_arguments)]
fn run_process_flat(
    input: &[PathBuf],
    output: &Path,
    format: OutputFormat,
    algo: ClusteringAlgorithm,
    clustering: &ClusteringConfig,
    extraction: &ExtractionConfig,
    params: &AlgorithmParams,
    out_of_core: bool,
    ooc_config: &OutOfCoreConfig,
    verbose: bool,
) -> Result<(usize, usize)> {
    let pending = PendingOutput::new(output)?;
    let mut writer = rustpix_io::DataFileWriter::create(pending.path())?;
    let mut wrote_header = false;
    let mut total_hits = 0usize;
    let mut total_neutrons = 0usize;

    for path in input {
        if verbose {
            eprintln!("Reading: {}", path.display());
        }
        let reader = open_input(path)?;

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

    drop(writer);
    pending.commit()?;
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
    output: &Path,
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
    let pending = PendingOutput::new(output)?;
    let mut sink = Hdf5NeutronSink::create(pending.path(), options)?;
    let mut total_hits = 0usize;
    let mut total_neutrons = 0usize;

    // TDC rebase state: each TPX3 file has an independent TDC clock.
    // Normalise each file to its first-pulse baseline and then offset to
    // stay monotonically increasing across file boundaries.
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

        let mut file_base_tdc: Option<u64> = None;

        let reader = open_input(path)?;

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
                sink.write_neutrons(&event_batch)?;
            }
        }

        if verbose {
            eprintln!("  Cumulative: {total_hits} hits, {total_neutrons} neutrons");
        }
    }

    drop(sink);
    pending.commit()?;
    Ok((total_hits, total_neutrons))
}

/// Process files and write output as ORNL SNS `NXsnsevent` HDF5.
#[allow(clippy::too_many_arguments)]
fn run_process_sns_hdf5(
    input: &[PathBuf],
    output: &Path,
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
    let pending = PendingOutput::new(output)?;
    let mut sink = SnsEventSink::create(pending.path(), write_options)
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

        let reader = open_input(path)?;

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
    drop(sink);
    pending.commit()?;
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

    let mut stack = TofStack::new(tof_bins, 514, 514)?;

    // Determine tof_max.  If not provided, scan once to find it.
    stack.tof_max = if let Some(v) = tof_max_override {
        v
    } else {
        if verbose {
            eprintln!("Scanning for TOF range ...");
        }
        let mut observed: u32 = 0;
        for path in input {
            let reader = open_input(path)?;
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
        eprintln!("TOF max: {} (25ns ticks)", stack.tof_max);
        eprintln!("TOF bins: {tof_bins}");
    }

    let mut total_hits = 0usize;
    let mut total_neutrons = 0usize;

    for path in input {
        if verbose {
            eprintln!("Reading: {}", path.display());
        }
        let reader = open_input(path)?;

        if out_of_core {
            let stream = out_of_core_neutron_stream(
                &reader, algo, clustering, extraction, params, ooc_config,
            )?;
            for batch in stream {
                let batch = batch?;
                total_hits = total_hits.saturating_add(batch.hits_processed);
                total_neutrons = total_neutrons.saturating_add(batch.neutrons.len());
                stack.add(&batch.neutrons, extraction.super_resolution_factor);
            }
        } else {
            let stream = reader.stream_time_ordered()?;
            for mut batch in stream {
                total_hits = total_hits.saturating_add(batch.len());
                let neutrons =
                    cluster_and_extract_batch(&mut batch, algo, clustering, extraction, params)?;
                total_neutrons = total_neutrons.saturating_add(neutrons.len());
                stack.add(&neutrons, extraction.super_resolution_factor);
            }
        }

        if verbose {
            eprintln!("  Cumulative: {total_hits} hits, {total_neutrons} neutrons");
        }
    }

    if stack.above_range > 0 {
        eprintln!(
            "Warning: {} neutron(s) with TOF above {} ticks were left out of the histogram",
            stack.above_range, stack.tof_max
        );
    }

    if verbose {
        eprintln!("Writing TIFF stack to: {}", output.display());
    }

    let tiff = PendingOutput::new(output)?;
    write_tiff_stack_file(
        tiff.path(),
        &stack.data,
        stack.bins,
        stack.width,
        stack.height,
        bit_depth,
    )?;

    if let Some(spectrum_path) = spectrum_sidecar_path(output) {
        let spectrum = PendingOutput::new(&spectrum_path)?;
        write_spectrum_csv(
            spectrum.path(),
            &stack.data,
            stack.bins,
            stack.width,
            stack.height,
            stack.bin_width(),
        )?;
        // Sidecar first, so a failed rename never costs the earlier TIFF.
        spectrum.commit()?;
        tiff.commit()?;
        if verbose {
            eprintln!("Wrote spectrum to: {}", spectrum_path.display());
        }
    } else {
        eprintln!("Warning: skipping spectrum CSV — output path already has .csv extension");
        tiff.commit()?;
    }

    Ok((total_hits, total_neutrons))
}

/// TOF-binned image stack accumulated from neutron batches.
struct TofStack {
    /// Counts indexed `[tof_bin][y][x]`.
    data: Vec<u64>,
    bins: usize,
    width: usize,
    height: usize,
    /// Inclusive upper edge of the last bin (25 ns ticks).
    tof_max: u32,
    /// Neutrons left out because their TOF exceeds `tof_max`.
    above_range: usize,
}

impl TofStack {
    fn new(bins: usize, width: usize, height: usize) -> Result<Self> {
        let too_large = || {
            CliError::Other(format!(
                "cannot allocate a {bins} x {height} x {width} histogram; reduce --tof-bins"
            ))
        };
        let cells = bins.checked_mul(height * width).ok_or_else(too_large)?;
        let mut data = Vec::new();
        data.try_reserve_exact(cells).map_err(|_| too_large())?;
        data.resize(cells, 0);
        Ok(Self {
            data,
            bins,
            width,
            height,
            tof_max: 0,
            above_range: 0,
        })
    }

    /// Bin width in 25 ns ticks.
    #[allow(clippy::cast_precision_loss)]
    fn bin_width(&self) -> f64 {
        f64::from(self.tof_max) / self.bins as f64
    }

    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn add(&mut self, batch: &NeutronBatch, super_resolution_factor: f64) {
        let inv = 1.0 / super_resolution_factor;
        let bin_width = self.bin_width();
        for i in 0..batch.len() {
            let x = (batch.x[i] * inv).round();
            let y = (batch.y[i] * inv).round();
            let tof = batch.tof[i];

            if !x.is_finite() || !y.is_finite() || x < 0.0 || y < 0.0 {
                continue;
            }
            if tof > self.tof_max {
                self.above_range += 1;
                continue;
            }
            let x = x as usize;
            let y = y as usize;

            let tof_bin = if bin_width > 0.0 {
                // A neutron exactly at `tof_max` belongs to the last bin.
                ((f64::from(tof) / bin_width) as usize).min(self.bins - 1)
            } else {
                0
            };

            if x < self.width && y < self.height {
                self.data[tof_bin * self.height * self.width + y * self.width + x] += 1;
            }
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
    writeln!(file, "{SPECTRUM_HEADER}")?;
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
    file.flush()?;
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

fn run_info(input: &Path) -> Result<()> {
    let reader = open_input(input)?;
    let file_size = reader.file_size();
    let packet_count = reader.packet_count();

    println!("File: {}", input.display());
    println!(
        "Size: {} bytes ({:.2} MB)",
        file_size,
        usize_to_f64(file_size) / 1_000_000.0
    );
    println!("Packets: {packet_count}");

    let mut hits = 0usize;
    let mut tof = (u32::MAX, 0u32);
    let mut x = (u16::MAX, 0u16);
    let mut y = (u16::MAX, 0u16);
    for batch in reader.stream_time_ordered()? {
        hits += batch.len();
        for &v in &batch.tof {
            tof = (tof.0.min(v), tof.1.max(v));
        }
        for &v in &batch.x {
            x = (x.0.min(v), x.1.max(v));
        }
        for &v in &batch.y {
            y = (y.0.min(v), y.1.max(v));
        }
    }
    println!("Hits: {hits}");

    if hits > 0 {
        println!("TOF range: {} - {}", tof.0, tof.1);
        println!("X range: {} - {}", x.0, x.1);
        println!("Y range: {} - {}", y.0, y.1);
    }

    Ok(())
}

fn require_iterations(iterations: usize) -> Result<()> {
    if iterations == 0 {
        return Err(CliError::Other("--iterations must be >= 1".to_string()));
    }
    Ok(())
}

fn run_benchmark(input: &Path, iterations: usize) -> Result<()> {
    require_iterations(iterations)?;
    let reader = open_input(input)?;
    let pulses: Vec<HitBatch> = reader.stream_time_ordered()?.collect();
    let hits: usize = pulses.iter().map(HitBatch::len).sum();

    println!(
        "Benchmarking with {hits} hits in {} pulses, {iterations} iterations",
        pulses.len()
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
        warmup_algorithm(algo_enum, &pulses);
        let times = benchmark_algorithm(algo_enum, &pulses, iterations)?;

        let min_time = times.iter().fold(f64::INFINITY, |a, &b| a.min(b));
        let max_time = times.iter().fold(f64::NEG_INFINITY, |a, &b| a.max(b));
        let mean_time = times.iter().sum::<f64>() / usize_to_f64(times.len());

        println!("{name:<10} | {mean_time:<15.2} | {min_time:<15.2} | {max_time:<15.2}");
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn run_out_of_core_benchmark(
    input: &Path,
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
    require_iterations(iterations)?;
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
        std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get)
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
    input: &Path,
    algo: ClusteringAlgorithm,
    clustering: &ClusteringConfig,
    extraction: &ExtractionConfig,
    params: &AlgorithmParams,
    config: &OutOfCoreConfig,
) -> Result<(usize, usize, std::time::Duration)> {
    let reader = open_input(input)?;
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

fn warmup_algorithm(algo_enum: Algorithm, pulses: &[HitBatch]) {
    for pulse in pulses {
        let _ = run_cluster_once(algo_enum, &mut pulse.clone());
    }
}

/// Times clustering of every pulse in turn; the copy of the input is not timed.
fn benchmark_algorithm(
    algo_enum: Algorithm,
    pulses: &[HitBatch],
    iterations: usize,
) -> Result<Vec<f64>> {
    let mut times = Vec::with_capacity(iterations);

    for _ in 0..iterations {
        let mut work = pulses.to_vec();
        let start = Instant::now();
        for pulse in &mut work {
            run_cluster_once(algo_enum, pulse)?;
        }
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
