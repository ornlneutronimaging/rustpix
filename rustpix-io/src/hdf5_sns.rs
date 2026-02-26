//! ORNL SNS/HFIR `NXsnsevent` HDF5 export.
//!
//! Produces `NeXus` event files compatible with the ORNL SNS & HFIR data
//! pipeline (Mantid, etc.). Data is stored as 1D pixel-ID vectors with
//! bank-specific offsets, matching the `NXsnsevent` definition.

// Intentional truncation: u64→usize (safe on 64-bit), f64→f32 (SNS format uses f32 TOF),
// f64→u32 (pixel coords clamped to non-negative), u64→f64 (sub-ns precision loss is fine).
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]

use crate::hdf5::{
    append_slice, create_extendable_dataset, set_attr_str_group, set_dataset_units,
    to_var_len_unicode, NeutronEventBatch,
};
use crate::reader::EventBatch;
use crate::{Error, Result};
use hdf5::types::VarLenUnicode;
use hdf5::{Dataset, File, Group};
use std::collections::HashSet;
use std::path::Path;

const NS_PER_TICK: u64 = 25;
const US_PER_TICK: f64 = 25.0 / 1000.0;

/// Remap a coordinate from the source grid (which may contain chip-gap pixels)
/// to the target grid (which does not).
///
/// Gap positions are given as a sorted slice.  A coordinate that falls on a gap
/// returns `None`.  Coordinates beyond the gap are shifted down by the number
/// of gap positions below them.
fn remap_gap(coord: u32, gaps: &[u32]) -> Option<u32> {
    let shift = gaps.partition_point(|&g| g < coord);
    if gaps.get(shift).copied() == Some(coord) {
        return None; // coord is a gap pixel
    }
    Some(coord - shift as u32)
}

// ---------------------------------------------------------------------------
// Configuration types
// ---------------------------------------------------------------------------

/// Configuration for a single detector bank in SNS format.
#[derive(Clone, Debug)]
pub struct SnsBankConfig {
    /// Bank name (e.g., `"bank100"`). Used for HDF5 group naming.
    pub name: String,
    /// Pixel-ID offset for this bank (e.g., 1\_000\_000 for bank100).
    pub pixel_id_offset: u32,
    /// Number of columns (pixels per row).
    pub width: u32,
    /// Number of rows.
    pub height: u32,
    /// Column indices that are chip-gap pixels in the source coordinate space.
    /// Events at these columns are silently dropped; columns beyond the gap
    /// are shifted down by the gap width.  For VENUS this is `[256, 257]`.
    pub gap_columns: Vec<u32>,
    /// Row indices that are chip-gap pixels in the source coordinate space.
    /// Same semantics as `gap_columns`.  For VENUS this is `[256, 257]`.
    pub gap_rows: Vec<u32>,
}

/// Run-level metadata for an SNS `NXsnsevent` file.
#[derive(Clone, Debug)]
pub struct SnsRunMetadata {
    /// Run number (e.g., 15162).
    pub run_number: u32,
    /// IPTS experiment identifier (e.g., `"IPTS-35004"`).
    pub experiment_identifier: String,
    /// ISO 8601 start time of the run.
    pub start_time: String,
    /// ISO 8601 end time (filled at finalisation if `None`).
    pub end_time: Option<String>,
    /// Run duration in seconds.
    pub duration: Option<f64>,
    /// Total integrated proton charge in picoCoulombs.
    pub proton_charge: Option<f64>,
    /// Run title / description.
    pub title: Option<String>,
}

/// Instrument metadata for the `NXinstrument` group.
#[derive(Clone, Debug)]
pub struct SnsInstrumentConfig {
    /// Instrument short name (e.g., `"VENUS"`).
    pub name: String,
    /// Beamline identifier (e.g., `"BL10"`).
    pub beamline: String,
    /// Optional Instrument Definition File XML string.
    pub instrument_xml: Option<String>,
}

/// Top-level options for an SNS `NXsnsevent` export.
#[derive(Clone, Debug)]
pub struct SnsWriteOptions {
    /// Detector bank configurations (one per bank).
    pub banks: Vec<SnsBankConfig>,
    /// Run metadata.
    pub run: SnsRunMetadata,
    /// Instrument configuration.
    pub instrument: SnsInstrumentConfig,
    /// Super-resolution factor for neutron coordinates (default 1.0).
    ///
    /// When writing neutron events whose x/y are in super-resolution space,
    /// set this to the factor used during extraction (e.g., 8.0) so that
    /// pixel IDs are computed correctly.
    pub super_resolution_factor: f64,
    /// Chunk size along the event dimension.
    pub chunk_events: usize,
    /// Optional gzip compression level (0–9). `None` disables compression.
    pub compression: Option<u8>,
    /// Enable byte-shuffle filter before compression.
    pub shuffle: bool,
}

impl SnsWriteOptions {
    /// Create options with VENUS defaults.
    ///
    /// Bank100 with pixel-ID offset 1\_000\_000 and a 512×512 grid,
    /// instrument name "VENUS", beamline "BL10".
    #[must_use]
    pub fn venus_defaults(run: SnsRunMetadata) -> Self {
        Self {
            banks: vec![SnsBankConfig {
                name: "bank100".to_string(),
                pixel_id_offset: 1_000_000,
                width: 512,
                height: 512,
                gap_columns: vec![256, 257],
                gap_rows: vec![256, 257],
            }],
            run,
            instrument: SnsInstrumentConfig {
                name: "VENUS".to_string(),
                beamline: "BL10".to_string(),
                instrument_xml: None,
            },
            super_resolution_factor: 1.0,
            chunk_events: 100_000,
            compression: Some(1),
            shuffle: true,
        }
    }
}

/// A single `DASlogs` entry (process variable time series).
#[derive(Clone, Debug)]
pub struct DasLogEntry {
    /// Process variable name (e.g., `"BL10:Mot:S1:X"`).
    pub name: String,
    /// Timestamps in seconds from run start.
    pub time: Vec<f64>,
    /// Values at each timestamp.
    pub value: Vec<f64>,
    /// Units string (e.g., `"mm"`, `"K"`, `"deg"`).
    pub units: Option<String>,
}

// ---------------------------------------------------------------------------
// Internal bank writer
// ---------------------------------------------------------------------------

struct SnsBankEventWriter {
    event_id: Dataset,
    event_time_offset: Dataset,
    event_time_zero: Dataset,
    event_index: Dataset,
    total_counts_ds: Dataset,
    event_count: u64,
    pulse_count: u64,
    /// Buffered absolute pulse timestamps (in nanoseconds).  Written to
    /// `event_time_zero` as relative seconds during [`SnsEventSink::finalize`],
    /// once the global minimum timestamp across all banks is known.
    pulse_timestamps_ns: Vec<u64>,
}

impl SnsBankEventWriter {
    fn new(group: &Group, options: &SnsWriteOptions) -> Result<Self> {
        let event_id = create_extendable_dataset::<u32>(
            group,
            "event_id",
            options.chunk_events,
            options.compression,
            options.shuffle,
        )?;

        let event_time_offset = create_extendable_dataset::<f32>(
            group,
            "event_time_offset",
            options.chunk_events,
            options.compression,
            options.shuffle,
        )?;
        set_dataset_units(&event_time_offset, "microsecond")?;

        let event_time_zero = create_extendable_dataset::<f64>(
            group,
            "event_time_zero",
            options.chunk_events,
            options.compression,
            options.shuffle,
        )?;
        set_dataset_units(&event_time_zero, "second")?;

        let event_index = create_extendable_dataset::<u64>(
            group,
            "event_index",
            options.chunk_events,
            options.compression,
            options.shuffle,
        )?;

        let total_counts_ds = group
            .new_dataset::<u64>()
            .shape((1,))
            .create("total_counts")?;
        total_counts_ds.write_raw(&[0u64])?;

        Ok(Self {
            event_id,
            event_time_offset,
            event_time_zero,
            event_index,
            total_counts_ds,
            event_count: 0,
            pulse_count: 0,
            pulse_timestamps_ns: Vec::new(),
        })
    }

    /// Append hit events, returning the number of events actually written
    /// (may be fewer than the batch size when gap pixels are dropped).
    ///
    /// `pulse_ns` is the absolute pulse timestamp in nanoseconds.  It is
    /// buffered and converted to relative seconds in
    /// [`Self::write_pulse_times`].
    fn append_hits(
        &mut self,
        bank: &SnsBankConfig,
        batch: &EventBatch,
        pulse_ns: u64,
    ) -> Result<usize> {
        let n = batch.hits.x.len();
        if n == 0 {
            return Ok(0);
        }

        // 1. Filter events first — remap through chip-gap positions, drop
        //    gap pixels and out-of-bounds coordinates.
        let mut pixel_ids = Vec::with_capacity(n);
        let mut tof_us = Vec::with_capacity(n);
        for i in 0..n {
            let raw_x = u32::from(batch.hits.x[i]);
            let raw_y = u32::from(batch.hits.y[i]);
            let Some(px) = remap_gap(raw_x, &bank.gap_columns) else {
                continue; // gap pixel — skip
            };
            let Some(py) = remap_gap(raw_y, &bank.gap_rows) else {
                continue;
            };
            if px >= bank.width || py >= bank.height {
                continue; // out-of-bounds — skip
            }
            pixel_ids.push(bank.pixel_id_offset + py * bank.width + px);
            tof_us.push((f64::from(batch.hits.tof[i]) * US_PER_TICK) as f32);
        }

        if pixel_ids.is_empty() {
            return Ok(0); // no surviving events — no pulse entry
        }

        // 2. Write event_index and buffer pulse timestamp (event_time_zero
        //    is written in finalize once the global minimum is known).
        append_slice(
            &self.event_index,
            self.pulse_count as usize,
            &[self.event_count],
        )?;
        self.pulse_timestamps_ns.push(pulse_ns);
        self.pulse_count += 1;

        // 3. Write events
        append_slice(&self.event_id, self.event_count as usize, &pixel_ids)?;
        append_slice(&self.event_time_offset, self.event_count as usize, &tof_us)?;

        let written = pixel_ids.len();
        self.event_count += written as u64;
        Ok(written)
    }

    /// Append neutron events, returning the number of events actually written
    /// (may be fewer than the batch size when gap pixels are dropped).
    ///
    /// `pulse_ns` is the absolute pulse timestamp in nanoseconds.  It is
    /// buffered and converted to relative seconds in
    /// [`Self::write_pulse_times`].
    fn append_neutrons(
        &mut self,
        bank: &SnsBankConfig,
        batch: &NeutronEventBatch,
        pulse_ns: u64,
        super_resolution_factor: f64,
    ) -> Result<usize> {
        let n = batch.neutrons.x.len();
        if n == 0 {
            return Ok(0);
        }

        // 1. Filter events first — convert super-resolution coords to pixel
        //    coords, remap through chip-gap positions, and drop gap/OOB events.
        let inv = 1.0 / super_resolution_factor;
        let mut pixel_ids = Vec::with_capacity(n);
        let mut tof_us = Vec::with_capacity(n);
        for i in 0..n {
            let raw_x = (batch.neutrons.x[i] * inv).round().max(0.0) as u32;
            let raw_y = (batch.neutrons.y[i] * inv).round().max(0.0) as u32;
            let Some(px) = remap_gap(raw_x, &bank.gap_columns) else {
                continue;
            };
            let Some(py) = remap_gap(raw_y, &bank.gap_rows) else {
                continue;
            };
            if px >= bank.width || py >= bank.height {
                continue; // out-of-bounds — skip
            }
            pixel_ids.push(bank.pixel_id_offset + py * bank.width + px);
            tof_us.push((f64::from(batch.neutrons.tof[i]) * US_PER_TICK) as f32);
        }

        if pixel_ids.is_empty() {
            return Ok(0); // no surviving events — no pulse entry
        }

        // 2. Write event_index and buffer pulse timestamp (event_time_zero
        //    is written in finalize once the global minimum is known).
        append_slice(
            &self.event_index,
            self.pulse_count as usize,
            &[self.event_count],
        )?;
        self.pulse_timestamps_ns.push(pulse_ns);
        self.pulse_count += 1;

        // 3. Write events
        append_slice(&self.event_id, self.event_count as usize, &pixel_ids)?;
        append_slice(&self.event_time_offset, self.event_count as usize, &tof_us)?;

        let written = pixel_ids.len();
        self.event_count += written as u64;
        Ok(written)
    }

    /// Flush buffered pulse timestamps to `event_time_zero` as seconds
    /// relative to `min_start_ns` (the global minimum across all banks).
    fn write_pulse_times(&self, min_start_ns: u64) -> Result<()> {
        let times_s: Vec<f64> = self
            .pulse_timestamps_ns
            .iter()
            .map(|&ns| (ns - min_start_ns) as f64 / 1_000_000_000.0)
            .collect();
        // Write all pulse times at once (dataset was created empty/extendable).
        if !times_s.is_empty() {
            append_slice(&self.event_time_zero, 0, &times_s)?;
        }
        Ok(())
    }

    fn write_total_counts(&self) -> Result<()> {
        self.total_counts_ds.write_raw(&[self.event_count])?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Public streaming writer
// ---------------------------------------------------------------------------

/// Streaming writer for ORNL SNS `NXsnsevent` HDF5 files.
///
/// Writes event data in bank-based format with pulse-indexed structure.
/// Call [`Self::write_hits`] or [`Self::write_neutrons`] per pulse, then [`Self::finalize`].
pub struct SnsEventSink {
    _file: File,
    entry: Group,
    writers: Vec<(SnsBankConfig, SnsBankEventWriter)>,
    options: SnsWriteOptions,
    /// Global minimum pulse timestamp across all banks.  Computed from the
    /// minimum first-pulse across every bank and used as the time origin for
    /// `event_time_zero` (written during [`Self::finalize`]).
    min_pulse_ns: Option<u64>,
    /// Per-bank last pulse timestamp for monotonicity enforcement.
    /// Each bank's event stream must be independently monotonic, but banks
    /// may be written in any order (e.g., all pulses for bank 0, then bank 1).
    last_pulse_ns_per_bank: Vec<u64>,
    /// Global maximum pulse timestamp across all banks, used for computing
    /// run duration in [`Self::finalize`].
    max_pulse_ns: u64,
    /// Set of pulse timestamps already counted toward `total_pulses`, used to
    /// de-duplicate counts when the same physical pulse is written to multiple
    /// banks regardless of write ordering.
    counted_pulse_ns: HashSet<u64>,
    total_counts: u64,
    total_pulses: u64,
    finalized: bool,
}

impl SnsEventSink {
    /// Create a new SNS event sink.
    ///
    /// Builds the full HDF5 skeleton (`NXentry`, banks, instrument, `DASlogs`).
    ///
    /// # Errors
    /// Returns an error if the file or HDF5 structures cannot be created.
    pub fn create<P: AsRef<Path>>(path: P, options: SnsWriteOptions) -> Result<Self> {
        let srf = options.super_resolution_factor;
        if !srf.is_finite() || srf <= 0.0 {
            return Err(crate::Error::InvalidFormat(format!(
                "super_resolution_factor must be finite and positive, got {srf}"
            )));
        }

        let file = File::create(path)?;

        // Root-level entry
        let entry = file.create_group("entry")?;
        set_attr_str_group(&entry, "NX_class", "NXentry")?;
        set_attr_str_group(&entry, "definition", "NXsnsevent")?;

        // Run metadata as datasets (matching SNS convention: scalar datasets, not attributes)
        write_str_dataset(&entry, "run_number", &options.run.run_number.to_string())?;
        write_str_dataset(
            &entry,
            "experiment_identifier",
            &options.run.experiment_identifier,
        )?;
        write_str_dataset(&entry, "start_time", &options.run.start_time)?;
        write_str_dataset(
            &entry,
            "end_time",
            options.run.end_time.as_deref().unwrap_or(""),
        )?;
        if let Some(title) = &options.run.title {
            write_str_dataset(&entry, "title", title)?;
        }

        // Scalar numeric metadata
        write_f64_dataset(
            &entry,
            "duration",
            options.run.duration.unwrap_or(0.0),
            "second",
        )?;
        write_f64_dataset(
            &entry,
            "proton_charge",
            options.run.proton_charge.unwrap_or(0.0),
            "picoCoulomb",
        )?;
        write_u64_dataset(&entry, "total_counts", 0)?;
        write_u64_dataset(&entry, "total_pulses", 0)?;

        // Bank event groups
        let mut writers = Vec::with_capacity(options.banks.len());
        for bank in &options.banks {
            let group_name = format!("{}_events", bank.name);
            let group = entry.create_group(&group_name)?;
            set_attr_str_group(&group, "NX_class", "NXevent_data")?;

            let writer = SnsBankEventWriter::new(&group, &options)?;
            writers.push((bank.clone(), writer));
        }

        // Instrument group with hard links to banks
        let instrument = entry.create_group("instrument")?;
        set_attr_str_group(&instrument, "NX_class", "NXinstrument")?;
        write_str_dataset(&instrument, "name", &options.instrument.name)?;
        write_str_dataset(&instrument, "beamline", &options.instrument.beamline)?;

        for bank in &options.banks {
            let src = format!("{}_events", bank.name);
            let dst = format!("instrument/{}", bank.name);
            entry.link_hard(&src, &dst)?;
        }

        // Instrument definition XML
        if let Some(ref xml) = options.instrument.instrument_xml {
            let xml_group = instrument.create_group("instrument_xml")?;
            set_attr_str_group(&xml_group, "NX_class", "NXnote")?;
            write_str_dataset(&xml_group, "data", xml)?;
            write_str_dataset(&xml_group, "type", "text/xml")?;
        }

        // DASlogs placeholder
        let daslogs = entry.create_group("DASlogs")?;
        set_attr_str_group(&daslogs, "NX_class", "NXcollection")?;

        // Sample placeholder
        let sample = entry.create_group("sample")?;
        set_attr_str_group(&sample, "NX_class", "NXsample")?;
        write_str_dataset(&sample, "name", "")?;

        let num_banks = writers.len();
        Ok(Self {
            _file: file,
            entry,
            writers,
            options,
            min_pulse_ns: None,
            last_pulse_ns_per_bank: vec![0u64; num_banks],
            max_pulse_ns: 0,
            counted_pulse_ns: HashSet::new(),
            total_counts: 0,
            total_pulses: 0,
            finalized: false,
        })
    }

    /// Write a hit-event batch to the specified bank.
    ///
    /// `bank_index` selects which bank in [`SnsWriteOptions::banks`] to write to.
    ///
    /// # Errors
    /// Returns an error if the bank index is out of bounds or HDF5 write fails.
    pub fn write_hits(&mut self, bank_index: usize, batch: &EventBatch) -> Result<()> {
        if self.finalized {
            return Err(Error::InvalidFormat(
                "cannot write after finalization".into(),
            ));
        }
        if bank_index >= self.writers.len() {
            return Err(Error::InvalidFormat(format!(
                "bank index {bank_index} out of range (have {} banks)",
                self.writers.len()
            )));
        }

        let pulse_ns = batch.tdc_timestamp_25ns * NS_PER_TICK;
        self.validate_pulse_monotonicity(pulse_ns, bank_index)?;

        let (ref bank, ref mut writer) = self.writers[bank_index];
        let written = writer.append_hits(bank, batch, pulse_ns)?;

        if written > 0 {
            self.track_written_pulse(pulse_ns);
            self.total_counts += written as u64;
            if self.counted_pulse_ns.insert(pulse_ns) {
                self.total_pulses += 1;
            }
        }
        Ok(())
    }

    /// Write a neutron-event batch to the specified bank.
    ///
    /// `bank_index` selects which bank in [`SnsWriteOptions::banks`] to write to.
    ///
    /// # Errors
    /// Returns an error if the bank index is out of bounds or HDF5 write fails.
    pub fn write_neutrons(&mut self, bank_index: usize, batch: &NeutronEventBatch) -> Result<()> {
        if self.finalized {
            return Err(Error::InvalidFormat(
                "cannot write after finalization".into(),
            ));
        }
        if bank_index >= self.writers.len() {
            return Err(Error::InvalidFormat(format!(
                "bank index {bank_index} out of range (have {} banks)",
                self.writers.len()
            )));
        }

        let pulse_ns = batch.tdc_timestamp_25ns * NS_PER_TICK;
        self.validate_pulse_monotonicity(pulse_ns, bank_index)?;

        let (ref bank, ref mut writer) = self.writers[bank_index];
        let written =
            writer.append_neutrons(bank, batch, pulse_ns, self.options.super_resolution_factor)?;

        if written > 0 {
            self.track_written_pulse(pulse_ns);
            self.total_counts += written as u64;
            if self.counted_pulse_ns.insert(pulse_ns) {
                self.total_pulses += 1;
            }
        }
        Ok(())
    }

    /// Write `DASlogs` process-variable entries.
    ///
    /// Can be called multiple times to add different variables.
    ///
    /// # Errors
    /// Returns an error if the HDF5 write fails.
    pub fn write_daslogs(&self, logs: &[DasLogEntry]) -> Result<()> {
        if self.finalized {
            return Err(Error::InvalidFormat(
                "cannot write after finalization".into(),
            ));
        }
        let daslogs = self.entry.group("DASlogs")?;
        for log in logs {
            let group = daslogs.create_group(&log.name)?;
            set_attr_str_group(&group, "NX_class", "NXlog")?;

            let time_ds = group
                .new_dataset::<f64>()
                .shape((log.time.len(),))
                .create("time")?;
            time_ds.write_raw(&log.time)?;
            set_dataset_units(&time_ds, "second")?;

            let value_ds = group
                .new_dataset::<f64>()
                .shape((log.value.len(),))
                .create("value")?;
            value_ds.write_raw(&log.value)?;
            if let Some(ref units) = log.units {
                set_dataset_units(&value_ds, units)?;
            }
        }
        Ok(())
    }

    /// Finalise the file: update total counts/pulses, per-bank totals, and
    /// end time / duration.
    ///
    /// Called automatically on [`Drop`], but an explicit call allows error handling.
    ///
    /// # Errors
    /// Returns an error if the HDF5 write fails.
    #[allow(clippy::cast_precision_loss)]
    pub fn finalize(&mut self) -> Result<()> {
        if self.finalized {
            return Ok(());
        }
        self.finalized = true;

        // Compute the global minimum pulse timestamp across all banks.
        // This is the time origin for event_time_zero.
        let min_start_ns = self.min_pulse_ns.unwrap_or(0);

        // Per-bank: write buffered pulse times and total counts
        for (_bank, writer) in &self.writers {
            writer.write_pulse_times(min_start_ns)?;
            writer.write_total_counts()?;
        }

        // Entry-level totals
        overwrite_u64_dataset(&self.entry, "total_counts", self.total_counts)?;
        overwrite_u64_dataset(&self.entry, "total_pulses", self.total_pulses)?;

        // Compute and write end_time and duration from pulse timestamps.
        // `min_pulse_ns` and `max_pulse_ns` are *relative* detector tick counts,
        // not Unix-epoch nanoseconds — the delta gives the measurement duration.
        if self.min_pulse_ns.is_some() {
            let duration_s =
                (self.max_pulse_ns.saturating_sub(min_start_ns)) as f64 / 1_000_000_000.0;
            overwrite_f64_dataset(&self.entry, "duration", duration_s)?;

            // Derive end_time by adding duration to the human-supplied start_time.
            if let Some(start_epoch) = iso8601_to_epoch_secs(&self.options.run.start_time) {
                let end_epoch = start_epoch + duration_s.round() as u64;
                let end_time = epoch_secs_to_iso8601(end_epoch);
                overwrite_str_dataset(&self.entry, "end_time", &end_time)?;
            }
        }

        Ok(())
    }

    /// Validate per-bank monotonicity.
    ///
    /// Monotonicity is enforced **per bank**: each bank's pulse stream must be
    /// independently non-decreasing, but different banks may be written in any
    /// order (e.g., all pulses for bank 0, then all pulses for bank 1).
    ///
    /// This is called for every pulse *before* event filtering.  The global
    /// min/max tracking is deferred to [`Self::track_written_pulse`] so that
    /// empty pulses (all events filtered) do not affect `event_time_zero` or
    /// duration metadata.
    ///
    /// # Errors
    /// Returns an error if the pulse timestamp is earlier than the previous
    /// pulse *for the same bank* (non-monotonic).
    fn validate_pulse_monotonicity(&mut self, pulse_ns: u64, bank_index: usize) -> Result<()> {
        let bank_last = self.last_pulse_ns_per_bank[bank_index];
        if pulse_ns < bank_last {
            return Err(Error::InvalidFormat(format!(
                "non-monotonic pulse timestamp for bank {bank_index}: \
                 {pulse_ns} ns < previous {bank_last} ns",
            )));
        }
        self.last_pulse_ns_per_bank[bank_index] = pulse_ns;
        Ok(())
    }

    /// Update global min/max pulse timestamps for a pulse that produced at
    /// least one written event.  Called only when `written > 0`.
    fn track_written_pulse(&mut self, pulse_ns: u64) {
        self.min_pulse_ns = Some(self.min_pulse_ns.map_or(pulse_ns, |m| m.min(pulse_ns)));
        self.max_pulse_ns = self.max_pulse_ns.max(pulse_ns);
    }
}

impl Drop for SnsEventSink {
    fn drop(&mut self) {
        let _ = self.finalize();
    }
}

// ---------------------------------------------------------------------------
// One-shot convenience functions
// ---------------------------------------------------------------------------

/// Write hit events in SNS `NXsnsevent` format (one-shot).
///
/// All batches are written to bank index 0.
///
/// # Errors
/// Returns an error if the file cannot be created or data cannot be written.
pub fn write_hits_sns<P, I>(path: P, batches: I, options: &SnsWriteOptions) -> Result<()>
where
    P: AsRef<Path>,
    I: IntoIterator<Item = EventBatch>,
{
    let mut sink = SnsEventSink::create(path, options.clone())?;
    for batch in batches {
        sink.write_hits(0, &batch)?;
    }
    sink.finalize()?;
    Ok(())
}

/// Write neutron events in SNS `NXsnsevent` format (one-shot).
///
/// All batches are written to bank index 0.
///
/// # Errors
/// Returns an error if the file cannot be created or data cannot be written.
pub fn write_neutrons_sns<P, I>(path: P, batches: I, options: &SnsWriteOptions) -> Result<()>
where
    P: AsRef<Path>,
    I: IntoIterator<Item = NeutronEventBatch>,
{
    let mut sink = SnsEventSink::create(path, options.clone())?;
    for batch in batches {
        sink.write_neutrons(0, &batch)?;
    }
    sink.finalize()?;
    Ok(())
}

// ---------------------------------------------------------------------------
// HDF5 dataset helpers
// ---------------------------------------------------------------------------

fn write_str_dataset(group: &Group, name: &str, value: &str) -> Result<()> {
    let vlu = to_var_len_unicode(value)?;
    group
        .new_dataset::<VarLenUnicode>()
        .shape(())
        .create(name)?
        .write_scalar(&vlu)?;
    Ok(())
}

fn write_f64_dataset(group: &Group, name: &str, value: f64, units: &str) -> Result<()> {
    let ds = group.new_dataset::<f64>().shape(()).create(name)?;
    ds.write_scalar(&value)?;
    set_dataset_units(&ds, units)?;
    Ok(())
}

fn write_u64_dataset(group: &Group, name: &str, value: u64) -> Result<()> {
    let ds = group.new_dataset::<u64>().shape(()).create(name)?;
    ds.write_scalar(&value)?;
    Ok(())
}

fn overwrite_u64_dataset(group: &Group, name: &str, value: u64) -> Result<()> {
    let ds = group.dataset(name)?;
    ds.write_scalar(&value)?;
    Ok(())
}

fn overwrite_f64_dataset(group: &Group, name: &str, value: f64) -> Result<()> {
    let ds = group.dataset(name)?;
    ds.write_scalar(&value)?;
    Ok(())
}

fn overwrite_str_dataset(group: &Group, name: &str, value: &str) -> Result<()> {
    // HDF5 fixed-length string datasets cannot be resized; delete and recreate.
    if group.dataset(name).is_ok() {
        group.unlink(name)?;
    }
    write_str_dataset(group, name, value)?;
    Ok(())
}

/// Convert Unix epoch seconds to an ISO 8601 UTC string.
fn epoch_secs_to_iso8601(secs: u64) -> String {
    let days = secs / 86400;
    let rem = secs % 86400;
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

/// Parse a subset of ISO 8601 strings into Unix epoch seconds (UTC).
///
/// Accepted formats: `YYYY-MM-DDThh:mm:ssZ`, `YYYY-MM-DDThh:mm:ss±hh:mm`.
/// Returns `None` for unparseable input.
fn iso8601_to_epoch_secs(s: &str) -> Option<u64> {
    // Minimum length: "2025-01-01T00:00:00Z" = 20 chars
    if s.len() < 20 {
        return None;
    }
    let year: i64 = s.get(0..4)?.parse().ok()?;
    let month: u32 = s.get(5..7)?.parse().ok()?;
    let day: u32 = s.get(8..10)?.parse().ok()?;
    let hour: i64 = s.get(11..13)?.parse().ok()?;
    let min: i64 = s.get(14..16)?.parse().ok()?;
    let sec: i64 = s.get(17..19)?.parse().ok()?;

    // Timezone offset (after position 19): 'Z' or ±hh:mm
    let tz_offset_s: i64 = {
        let tz = s.get(19..)?;
        if tz == "Z" || tz.is_empty() {
            0
        } else {
            let sign: i64 = if tz.starts_with('+') { 1 } else { -1 };
            let tz = &tz[1..];
            let oh: i64 = tz.get(0..2).and_then(|v| v.parse().ok()).unwrap_or(0);
            let om: i64 = tz.get(3..5).and_then(|v| v.parse().ok()).unwrap_or(0);
            sign * (oh * 3600 + om * 60)
        }
    };

    // Days from epoch to start of `year`
    let mut days: i64 = 0;
    for y in 1970..year {
        days += if y % 4 == 0 && (y % 100 != 0 || y % 400 == 0) {
            366
        } else {
            365
        };
    }
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let month_days: [u32; 12] = [
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
    for &md in &month_days[..((month as usize).saturating_sub(1))] {
        days += i64::from(md);
    }
    days += i64::from(day.saturating_sub(1));

    let utc_secs = days * 86400 + hour * 3600 + min * 60 + sec - tz_offset_s;
    u64::try_from(utc_secs).ok()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use rustpix_core::neutron::NeutronBatch;
    use rustpix_core::soa::HitBatch;
    use tempfile::NamedTempFile;

    fn make_test_run() -> SnsRunMetadata {
        SnsRunMetadata {
            run_number: 99999,
            experiment_identifier: "IPTS-00001".to_string(),
            start_time: "2025-01-01T00:00:00-05:00".to_string(),
            end_time: None,
            duration: None,
            proton_charge: Some(100.0),
            title: Some("test run".to_string()),
        }
    }

    fn make_test_options() -> SnsWriteOptions {
        SnsWriteOptions::venus_defaults(make_test_run())
    }

    fn make_hit_batch(tdc: u64, xs: &[u16], ys: &[u16], tofs: &[u32]) -> EventBatch {
        let n = xs.len();
        let mut hits = HitBatch::with_capacity(n);
        hits.x.extend_from_slice(xs);
        hits.y.extend_from_slice(ys);
        hits.tof.extend_from_slice(tofs);
        hits.tot.extend_from_slice(&vec![10u16; n]);
        hits.timestamp.extend_from_slice(&vec![0u32; n]);
        hits.chip_id.extend_from_slice(&vec![0u8; n]);
        hits.cluster_id.extend_from_slice(&vec![-1i32; n]);
        EventBatch {
            tdc_timestamp_25ns: tdc,
            hits,
        }
    }

    fn make_neutron_batch(tdc: u64, xs: &[f64], ys: &[f64], tofs: &[u32]) -> NeutronEventBatch {
        let n = xs.len();
        NeutronEventBatch {
            tdc_timestamp_25ns: tdc,
            neutrons: NeutronBatch {
                x: xs.to_vec(),
                y: ys.to_vec(),
                tof: tofs.to_vec(),
                tot: vec![10u16; n],
                n_hits: vec![3u16; n],
                chip_id: vec![0u8; n],
            },
        }
    }

    // --- Pixel ID tests ---

    #[test]
    fn test_pixel_id_origin() {
        // (0, 0) -> offset + 0
        let opts = make_test_options();
        let batch = make_hit_batch(1000, &[0], &[0], &[100]);
        let file = NamedTempFile::new().unwrap();
        let path = file.path();

        let mut sink = SnsEventSink::create(path, opts).unwrap();
        sink.write_hits(0, &batch).unwrap();
        sink.finalize().unwrap();

        let f = File::open(path).unwrap();
        let ids: Vec<u32> = f
            .group("entry/bank100_events")
            .unwrap()
            .dataset("event_id")
            .unwrap()
            .read_raw()
            .unwrap();
        assert_eq!(ids, vec![1_000_000u32]);
    }

    #[test]
    fn test_pixel_id_corner() {
        // 514-space (511, 511) is beyond the gap at [256, 257], so it remaps
        // to 512-space (509, 509): 511 − 2 gap positions = 509.
        // event_id = 1_000_000 + 509*512 + 509 = 1_261_117
        let opts = make_test_options();
        let batch = make_hit_batch(1000, &[511], &[511], &[100]);
        let file = NamedTempFile::new().unwrap();
        let path = file.path();

        let mut sink = SnsEventSink::create(path, opts).unwrap();
        sink.write_hits(0, &batch).unwrap();
        sink.finalize().unwrap();

        let f = File::open(path).unwrap();
        let ids: Vec<u32> = f
            .group("entry/bank100_events")
            .unwrap()
            .dataset("event_id")
            .unwrap()
            .read_raw()
            .unwrap();
        assert_eq!(ids, vec![1_000_000 + 509 * 512 + 509]);
    }

    // --- Time conversion tests ---

    #[test]
    fn test_tof_conversion() {
        // 400 ticks * 25ns = 10_000 ns = 10.0 µs
        let opts = make_test_options();
        let batch = make_hit_batch(1000, &[0], &[0], &[400]);
        let file = NamedTempFile::new().unwrap();
        let path = file.path();

        let mut sink = SnsEventSink::create(path, opts).unwrap();
        sink.write_hits(0, &batch).unwrap();
        sink.finalize().unwrap();

        let f = File::open(path).unwrap();
        let tof: Vec<f32> = f
            .group("entry/bank100_events")
            .unwrap()
            .dataset("event_time_offset")
            .unwrap()
            .read_raw()
            .unwrap();
        assert!((tof[0] - 10.0f32).abs() < 1e-5);
    }

    #[test]
    fn test_pulse_time_relative() {
        // First pulse at tdc=40_000_000 (1 second in 25ns ticks)
        // Second pulse at tdc=40_000_000 + 666_667 (~16.67ms later)
        let opts = make_test_options();
        let batch1 = make_hit_batch(40_000_000, &[0], &[0], &[100]);
        let batch2 = make_hit_batch(40_666_667, &[1], &[1], &[200]);
        let file = NamedTempFile::new().unwrap();
        let path = file.path();

        let mut sink = SnsEventSink::create(path, opts).unwrap();
        sink.write_hits(0, &batch1).unwrap();
        sink.write_hits(0, &batch2).unwrap();
        sink.finalize().unwrap();

        let f = File::open(path).unwrap();
        let etz: Vec<f64> = f
            .group("entry/bank100_events")
            .unwrap()
            .dataset("event_time_zero")
            .unwrap()
            .read_raw()
            .unwrap();
        assert_eq!(etz.len(), 2);
        assert!((etz[0] - 0.0).abs() < 1e-12); // First pulse is t=0
        let expected = 666_667.0 * 25.0 / 1_000_000_000.0; // ~16.67ms
        assert!((etz[1] - expected).abs() < 1e-9);
    }

    // --- event_index tracking ---

    #[test]
    fn test_event_index_tracking() {
        let opts = make_test_options();
        let batch1 = make_hit_batch(1000, &[0, 1, 2], &[0, 0, 0], &[100, 200, 300]);
        let batch2 = make_hit_batch(2000, &[10, 11], &[10, 10], &[400, 500]);
        let file = NamedTempFile::new().unwrap();
        let path = file.path();

        let mut sink = SnsEventSink::create(path, opts).unwrap();
        sink.write_hits(0, &batch1).unwrap();
        sink.write_hits(0, &batch2).unwrap();
        sink.finalize().unwrap();

        let f = File::open(path).unwrap();
        let idx: Vec<u64> = f
            .group("entry/bank100_events")
            .unwrap()
            .dataset("event_index")
            .unwrap()
            .read_raw()
            .unwrap();
        assert_eq!(idx, vec![0u64, 3u64]); // First pulse starts at 0, second at 3
    }

    // --- HDF5 structure validation ---

    #[test]
    fn test_hdf5_structure() {
        let opts = make_test_options();
        let batch = make_hit_batch(1000, &[5], &[10], &[100]);
        let file = NamedTempFile::new().unwrap();
        let path = file.path();

        let mut sink = SnsEventSink::create(path, opts).unwrap();
        sink.write_hits(0, &batch).unwrap();
        sink.finalize().unwrap();

        let f = File::open(path).unwrap();

        // Entry attributes
        let entry = f.group("entry").unwrap();
        let nx_class: VarLenUnicode = entry.attr("NX_class").unwrap().read_scalar().unwrap();
        assert_eq!(nx_class.as_str(), "NXentry");
        let definition: VarLenUnicode = entry.attr("definition").unwrap().read_scalar().unwrap();
        assert_eq!(definition.as_str(), "NXsnsevent");

        // Bank event group exists
        let bank = f.group("entry/bank100_events").unwrap();
        let nx_class: VarLenUnicode = bank.attr("NX_class").unwrap().read_scalar().unwrap();
        assert_eq!(nx_class.as_str(), "NXevent_data");

        // Instrument group exists
        let inst = f.group("entry/instrument").unwrap();
        let nx_class: VarLenUnicode = inst.attr("NX_class").unwrap().read_scalar().unwrap();
        assert_eq!(nx_class.as_str(), "NXinstrument");

        // Hard link: instrument/bank100 should exist and contain same data
        let link = f.group("entry/instrument/bank100").unwrap();
        let link_ids: Vec<u32> = link.dataset("event_id").unwrap().read_raw().unwrap();
        let direct_ids: Vec<u32> = bank.dataset("event_id").unwrap().read_raw().unwrap();
        assert_eq!(link_ids, direct_ids);

        // DASlogs placeholder
        let daslogs = f.group("entry/DASlogs").unwrap();
        let nx_class: VarLenUnicode = daslogs.attr("NX_class").unwrap().read_scalar().unwrap();
        assert_eq!(nx_class.as_str(), "NXcollection");

        // Run metadata
        let run_num: VarLenUnicode = entry.dataset("run_number").unwrap().read_scalar().unwrap();
        assert_eq!(run_num.as_str(), "99999");
    }

    // --- Finalization ---

    #[test]
    fn test_finalization_totals() {
        let opts = make_test_options();
        let batch1 = make_hit_batch(1000, &[0, 1], &[0, 0], &[100, 200]);
        let batch2 = make_hit_batch(2000, &[2, 3, 4], &[0, 0, 0], &[300, 400, 500]);
        let file = NamedTempFile::new().unwrap();
        let path = file.path();

        let mut sink = SnsEventSink::create(path, opts).unwrap();
        sink.write_hits(0, &batch1).unwrap();
        sink.write_hits(0, &batch2).unwrap();
        sink.finalize().unwrap();

        let f = File::open(path).unwrap();
        let entry = f.group("entry").unwrap();

        // Entry-level totals
        let tc: u64 = entry
            .dataset("total_counts")
            .unwrap()
            .read_scalar()
            .unwrap();
        assert_eq!(tc, 5);
        let tp: u64 = entry
            .dataset("total_pulses")
            .unwrap()
            .read_scalar()
            .unwrap();
        assert_eq!(tp, 2);

        // Per-bank total
        let bank_tc: Vec<u64> = entry
            .group("bank100_events")
            .unwrap()
            .dataset("total_counts")
            .unwrap()
            .read_raw()
            .unwrap();
        assert_eq!(bank_tc, vec![5u64]);
    }

    #[test]
    fn test_pulse_count_deduplication_across_banks() {
        // Two banks, same pulse written to both — total_pulses should be 1, not 2.
        let mut opts = make_test_options();
        opts.banks.push(SnsBankConfig {
            name: "bank200".to_string(),
            pixel_id_offset: 2_000_000,
            width: 512,
            height: 512,
            gap_columns: vec![256, 257],
            gap_rows: vec![256, 257],
        });
        let batch = make_hit_batch(1000, &[0], &[0], &[100]);
        let file = NamedTempFile::new().unwrap();
        let path = file.path();

        let mut sink = SnsEventSink::create(path, opts).unwrap();
        sink.write_hits(0, &batch).unwrap();
        sink.write_hits(1, &batch).unwrap();
        sink.finalize().unwrap();

        let f = File::open(path).unwrap();
        let entry = f.group("entry").unwrap();

        let tc: u64 = entry
            .dataset("total_counts")
            .unwrap()
            .read_scalar()
            .unwrap();
        assert_eq!(tc, 2); // 1 event per bank = 2 total events

        let tp: u64 = entry
            .dataset("total_pulses")
            .unwrap()
            .read_scalar()
            .unwrap();
        assert_eq!(tp, 1); // Same pulse, counted once
    }

    // --- Empty batch ---

    #[test]
    fn test_empty_batch() {
        let opts = make_test_options();
        let batch = make_hit_batch(1000, &[], &[], &[]);
        let file = NamedTempFile::new().unwrap();
        let path = file.path();

        let mut sink = SnsEventSink::create(path, opts).unwrap();
        sink.write_hits(0, &batch).unwrap();
        sink.finalize().unwrap();

        let f = File::open(path).unwrap();
        let ids: Vec<u32> = f
            .group("entry/bank100_events")
            .unwrap()
            .dataset("event_id")
            .unwrap()
            .read_raw()
            .unwrap();
        assert!(ids.is_empty());
    }

    // --- Neutron roundtrip ---

    #[test]
    fn test_neutron_pixel_ids() {
        let mut opts = make_test_options();
        opts.super_resolution_factor = 8.0;

        // Neutron at super-res (80.0, 160.0) -> pixel (10, 20)
        // pixel_id = 1_000_000 + 20*512 + 10 = 1_010_250
        let batch = make_neutron_batch(1000, &[80.0], &[160.0], &[100]);
        let file = NamedTempFile::new().unwrap();
        let path = file.path();

        let mut sink = SnsEventSink::create(path, opts).unwrap();
        sink.write_neutrons(0, &batch).unwrap();
        sink.finalize().unwrap();

        let f = File::open(path).unwrap();
        let ids: Vec<u32> = f
            .group("entry/bank100_events")
            .unwrap()
            .dataset("event_id")
            .unwrap()
            .read_raw()
            .unwrap();
        assert_eq!(ids, vec![1_000_000 + 20 * 512 + 10]);
    }

    // --- DASlogs ---

    #[test]
    fn test_daslogs_write() {
        let opts = make_test_options();
        let file = NamedTempFile::new().unwrap();
        let path = file.path();

        let mut sink = SnsEventSink::create(path, opts).unwrap();
        sink.write_daslogs(&[DasLogEntry {
            name: "BL10:Mot:S1:X".to_string(),
            time: vec![0.0, 1.0, 2.0],
            value: vec![-212.0, -212.0, -212.0],
            units: Some("mm".to_string()),
        }])
        .unwrap();
        sink.finalize().unwrap();

        let f = File::open(path).unwrap();
        let log = f.group("entry/DASlogs/BL10:Mot:S1:X").unwrap();
        let nx_class: VarLenUnicode = log.attr("NX_class").unwrap().read_scalar().unwrap();
        assert_eq!(nx_class.as_str(), "NXlog");

        let times: Vec<f64> = log.dataset("time").unwrap().read_raw().unwrap();
        assert_eq!(times, vec![0.0, 1.0, 2.0]);

        let values: Vec<f64> = log.dataset("value").unwrap().read_raw().unwrap();
        assert_eq!(values, vec![-212.0, -212.0, -212.0]);
    }

    // --- VENUS defaults ---

    #[test]
    fn test_venus_defaults() {
        let opts = SnsWriteOptions::venus_defaults(make_test_run());
        assert_eq!(opts.banks.len(), 1);
        assert_eq!(opts.banks[0].name, "bank100");
        assert_eq!(opts.banks[0].pixel_id_offset, 1_000_000);
        assert_eq!(opts.banks[0].width, 512);
        assert_eq!(opts.banks[0].height, 512);
        assert_eq!(opts.instrument.name, "VENUS");
        assert_eq!(opts.instrument.beamline, "BL10");
    }

    // --- Dataset dtype validation ---

    #[test]
    fn test_dataset_dtypes() {
        let opts = make_test_options();
        let batch = make_hit_batch(1000, &[0], &[0], &[100]);
        let file = NamedTempFile::new().unwrap();
        let path = file.path();

        let mut sink = SnsEventSink::create(path, opts).unwrap();
        sink.write_hits(0, &batch).unwrap();
        sink.finalize().unwrap();

        let f = File::open(path).unwrap();
        let bank = f.group("entry/bank100_events").unwrap();

        // event_id should be u32
        let eid = bank.dataset("event_id").unwrap();
        assert!(eid.read_raw::<u32>().is_ok());

        // event_time_offset should be f32
        let eto = bank.dataset("event_time_offset").unwrap();
        assert!(eto.read_raw::<f32>().is_ok());

        // event_time_zero should be f64
        let etz = bank.dataset("event_time_zero").unwrap();
        assert!(etz.read_raw::<f64>().is_ok());

        // event_index should be u64
        let idx = bank.dataset("event_index").unwrap();
        assert!(idx.read_raw::<u64>().is_ok());
    }

    // --- Coordinate clamping ---

    #[test]
    fn test_hit_pixel_id_remapped_through_gap() {
        // Hit at (513, 513) is beyond the gap (256, 257) so it is remapped
        // to (513 − 2, 513 − 2) = (511, 511) for a 512×512 bank.
        // Expected: offset + 511*512 + 511 = 1_262_143
        let opts = make_test_options();
        let batch = make_hit_batch(1000, &[513], &[513], &[100]);
        let file = NamedTempFile::new().unwrap();
        let path = file.path();

        let mut sink = SnsEventSink::create(path, opts).unwrap();
        sink.write_hits(0, &batch).unwrap();
        sink.finalize().unwrap();

        let f = File::open(path).unwrap();
        let ids: Vec<u32> = f
            .group("entry/bank100_events")
            .unwrap()
            .dataset("event_id")
            .unwrap()
            .read_raw()
            .unwrap();
        assert_eq!(ids, vec![1_000_000 + 511 * 512 + 511]);
    }

    #[test]
    fn test_neutron_pixel_id_remapped_through_gap() {
        // Neutron at super-res (4104.0, 4104.0) with factor 8.0 -> pixel (513, 513)
        // Remapped through gap to (511, 511) for a 512×512 bank.
        let mut opts = make_test_options();
        opts.super_resolution_factor = 8.0;
        let batch = make_neutron_batch(1000, &[4104.0], &[4104.0], &[100]);
        let file = NamedTempFile::new().unwrap();
        let path = file.path();

        let mut sink = SnsEventSink::create(path, opts).unwrap();
        sink.write_neutrons(0, &batch).unwrap();
        sink.finalize().unwrap();

        let f = File::open(path).unwrap();
        let ids: Vec<u32> = f
            .group("entry/bank100_events")
            .unwrap()
            .dataset("event_id")
            .unwrap()
            .read_raw()
            .unwrap();
        assert_eq!(ids, vec![1_000_000 + 511 * 512 + 511]);
    }

    #[test]
    fn test_hit_gap_pixel_dropped() {
        // Hit at (256, 100) — column 256 is a gap pixel, should be dropped.
        let opts = make_test_options();
        let batch = make_hit_batch(1000, &[256], &[100], &[100]);
        let file = NamedTempFile::new().unwrap();
        let path = file.path();

        let mut sink = SnsEventSink::create(path, opts).unwrap();
        sink.write_hits(0, &batch).unwrap();
        sink.finalize().unwrap();

        let f = File::open(path).unwrap();
        let ids: Vec<u32> = f
            .group("entry/bank100_events")
            .unwrap()
            .dataset("event_id")
            .unwrap()
            .read_raw()
            .unwrap();
        assert!(ids.is_empty(), "Gap pixel should have been dropped");
    }

    #[test]
    fn test_hit_edge_column_preserved() {
        // Hit at (258, 0) — first real column after the gap — should remap
        // to column 256 (258 − 2 gap positions).
        let opts = make_test_options();
        let batch = make_hit_batch(1000, &[258], &[0], &[100]);
        let file = NamedTempFile::new().unwrap();
        let path = file.path();

        let mut sink = SnsEventSink::create(path, opts).unwrap();
        sink.write_hits(0, &batch).unwrap();
        sink.finalize().unwrap();

        let f = File::open(path).unwrap();
        let ids: Vec<u32> = f
            .group("entry/bank100_events")
            .unwrap()
            .dataset("event_id")
            .unwrap()
            .read_raw()
            .unwrap();
        // row 0, col 256 → offset + 256
        assert_eq!(ids, vec![1_000_256]);
    }

    #[test]
    fn test_hit_no_gap_bank_passes_through() {
        // A bank with no gap columns/rows should pass coordinates unchanged.
        let mut opts = make_test_options();
        opts.banks[0].gap_columns = vec![];
        opts.banks[0].gap_rows = vec![];
        let batch = make_hit_batch(1000, &[256], &[257], &[100]);
        let file = NamedTempFile::new().unwrap();
        let path = file.path();

        let mut sink = SnsEventSink::create(path, opts).unwrap();
        sink.write_hits(0, &batch).unwrap();
        sink.finalize().unwrap();

        let f = File::open(path).unwrap();
        let ids: Vec<u32> = f
            .group("entry/bank100_events")
            .unwrap()
            .dataset("event_id")
            .unwrap()
            .read_raw()
            .unwrap();
        assert_eq!(ids, vec![1_000_000 + 257 * 512 + 256]);
    }

    // --- Bank index bounds ---

    #[test]
    fn test_invalid_bank_index() {
        let opts = make_test_options();
        let batch = make_hit_batch(1000, &[0], &[0], &[100]);
        let file = NamedTempFile::new().unwrap();
        let path = file.path();

        let mut sink = SnsEventSink::create(path, opts).unwrap();
        let result = sink.write_hits(99, &batch);
        assert!(result.is_err());
    }

    // --- Non-monotonic pulse timestamps ---

    #[test]
    fn test_non_monotonic_pulse_timestamp_rejected() {
        let opts = make_test_options();
        let file = NamedTempFile::new().unwrap();
        let path = file.path();

        let mut sink = SnsEventSink::create(path, opts).unwrap();

        // First pulse at timestamp 2000 (25ns ticks).
        let batch1 = make_hit_batch(2000, &[0], &[0], &[100]);
        sink.write_hits(0, &batch1).unwrap();

        // Second pulse at earlier timestamp 1000 — should fail.
        let batch2 = make_hit_batch(1000, &[1], &[1], &[200]);
        let result = sink.write_hits(0, &batch2);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("non-monotonic"),
            "Expected non-monotonic error, got: {err_msg}"
        );
    }

    #[test]
    fn test_non_monotonic_regression_intermediate_decrease() {
        // Regression: sequence 1000→2000→1500 must be rejected even though
        // 1500 > run_start (1000). The check must compare against the
        // *previous* pulse, not just run start.
        let opts = make_test_options();
        let file = NamedTempFile::new().unwrap();
        let path = file.path();

        let mut sink = SnsEventSink::create(path, opts).unwrap();

        sink.write_hits(0, &make_hit_batch(1000, &[0], &[0], &[100]))
            .unwrap();
        sink.write_hits(0, &make_hit_batch(2000, &[1], &[1], &[200]))
            .unwrap();
        // 1500 < 2000 (previous pulse) → must error.
        let result = sink.write_hits(0, &make_hit_batch(1500, &[2], &[2], &[300]));
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("non-monotonic"),);
    }

    // --- Out-of-bounds coordinate filtering (Bug 2) ---

    #[test]
    fn test_hit_out_of_bounds_dropped() {
        // Bank is 512×512. A hit at remapped (5, 3) is valid, but a hit at
        // raw coordinate 520 (no gaps) remaps to 520 which is >= 512 and
        // must be dropped.
        let mut opts = make_test_options();
        opts.banks[0].gap_columns = vec![];
        opts.banks[0].gap_rows = vec![];
        // Two hits: (5, 3) valid, (520, 3) out-of-bounds
        let batch = make_hit_batch(1000, &[5, 520], &[3, 3], &[100, 200]);
        let file = NamedTempFile::new().unwrap();
        let path = file.path();

        let mut sink = SnsEventSink::create(path, opts).unwrap();
        sink.write_hits(0, &batch).unwrap();
        sink.finalize().unwrap();

        let f = File::open(path).unwrap();
        let ids: Vec<u32> = f
            .group("entry/bank100_events")
            .unwrap()
            .dataset("event_id")
            .unwrap()
            .read_raw()
            .unwrap();
        // Only the in-bounds hit survives: offset + 3*512 + 5
        assert_eq!(ids, vec![1_000_000 + 3 * 512 + 5]);
    }

    #[test]
    fn test_neutron_out_of_bounds_dropped() {
        // Same as above but for neutrons. Super-resolution factor 1.0,
        // no gaps, one valid neutron and one OOB.
        let mut opts = make_test_options();
        opts.banks[0].gap_columns = vec![];
        opts.banks[0].gap_rows = vec![];
        opts.super_resolution_factor = 1.0;
        // Two neutrons: (5.0, 3.0) valid, (520.0, 3.0) out-of-bounds
        let batch = make_neutron_batch(1000, &[5.0, 520.0], &[3.0, 3.0], &[100, 200]);
        let file = NamedTempFile::new().unwrap();
        let path = file.path();

        let mut sink = SnsEventSink::create(path, opts).unwrap();
        sink.write_neutrons(0, &batch).unwrap();
        sink.finalize().unwrap();

        let f = File::open(path).unwrap();
        let ids: Vec<u32> = f
            .group("entry/bank100_events")
            .unwrap()
            .dataset("event_id")
            .unwrap()
            .read_raw()
            .unwrap();
        assert_eq!(ids, vec![1_000_000 + 3 * 512 + 5]);
    }

    // --- Pulse metadata filtering (Bug 3) ---

    #[test]
    fn test_all_gap_pulse_no_pulse_entry() {
        // A pulse where all hits land on gap pixels should produce no pulse
        // array entries and total_pulses = 0.
        let opts = make_test_options();
        // Both hits at gap columns (256, 257)
        let batch = make_hit_batch(1000, &[256, 257], &[0, 0], &[100, 200]);
        let file = NamedTempFile::new().unwrap();
        let path = file.path();

        let mut sink = SnsEventSink::create(path, opts).unwrap();
        sink.write_hits(0, &batch).unwrap();
        sink.finalize().unwrap();

        let f = File::open(path).unwrap();
        let entry = f.group("entry").unwrap();

        let tp: u64 = entry
            .dataset("total_pulses")
            .unwrap()
            .read_scalar()
            .unwrap();
        assert_eq!(tp, 0, "All-gap pulse should not increment total_pulses");

        let tc: u64 = entry
            .dataset("total_counts")
            .unwrap()
            .read_scalar()
            .unwrap();
        assert_eq!(tc, 0);

        // No pulse entries should have been written
        let etz: Vec<f64> = f
            .group("entry/bank100_events")
            .unwrap()
            .dataset("event_time_zero")
            .unwrap()
            .read_raw()
            .unwrap();
        assert!(
            etz.is_empty(),
            "No event_time_zero entries for all-gap pulse"
        );

        let idx: Vec<u64> = f
            .group("entry/bank100_events")
            .unwrap()
            .dataset("event_index")
            .unwrap()
            .read_raw()
            .unwrap();
        assert!(idx.is_empty(), "No event_index entries for all-gap pulse");
    }

    #[test]
    fn test_mixed_gap_and_valid_pulses() {
        // First pulse: all hits on gaps → no pulse entry.
        // Second pulse: one valid hit → exactly 1 pulse entry.
        let opts = make_test_options();
        let file = NamedTempFile::new().unwrap();
        let path = file.path();

        let mut sink = SnsEventSink::create(path, opts).unwrap();

        // Pulse 1: all gap hits
        let gap_batch = make_hit_batch(1000, &[256, 257], &[0, 0], &[100, 200]);
        sink.write_hits(0, &gap_batch).unwrap();

        // Pulse 2: one valid hit at (5, 3)
        let valid_batch = make_hit_batch(2000, &[5], &[3], &[300]);
        sink.write_hits(0, &valid_batch).unwrap();

        sink.finalize().unwrap();

        let f = File::open(path).unwrap();
        let entry = f.group("entry").unwrap();

        let tp: u64 = entry
            .dataset("total_pulses")
            .unwrap()
            .read_scalar()
            .unwrap();
        assert_eq!(tp, 1, "Only the valid pulse should be counted");

        let tc: u64 = entry
            .dataset("total_counts")
            .unwrap()
            .read_scalar()
            .unwrap();
        assert_eq!(tc, 1);

        let etz: Vec<f64> = f
            .group("entry/bank100_events")
            .unwrap()
            .dataset("event_time_zero")
            .unwrap()
            .read_raw()
            .unwrap();
        assert_eq!(etz.len(), 1, "Only one pulse entry");

        let idx: Vec<u64> = f
            .group("entry/bank100_events")
            .unwrap()
            .dataset("event_index")
            .unwrap()
            .read_raw()
            .unwrap();
        assert_eq!(idx, vec![0u64], "Single pulse starts at event 0");
    }

    // --- Empty pulse time-origin exclusion ---

    #[test]
    fn test_empty_pulse_does_not_shift_time_origin() {
        // Empty pulse at tdc=1000, valid pulse at tdc=2000.
        // min_pulse_ns must be based on tdc=2000 (the first *written* pulse),
        // so event_time_zero[0] must be 0.0, NOT offset by the empty pulse.
        let opts = make_test_options();
        let file = NamedTempFile::new().unwrap();
        let path = file.path();

        let mut sink = SnsEventSink::create(path, opts).unwrap();

        // Pulse 1 (tdc=1000): all gap-column hits → 0 events written
        let gap_batch = make_hit_batch(1000, &[256, 257], &[0, 0], &[100, 200]);
        sink.write_hits(0, &gap_batch).unwrap();

        // Pulse 2 (tdc=2000): one valid hit
        let valid_batch = make_hit_batch(2000, &[5], &[3], &[300]);
        sink.write_hits(0, &valid_batch).unwrap();

        sink.finalize().unwrap();

        let f = File::open(path).unwrap();

        let etz: Vec<f64> = f
            .group("entry/bank100_events")
            .unwrap()
            .dataset("event_time_zero")
            .unwrap()
            .read_raw()
            .unwrap();
        assert_eq!(etz.len(), 1, "Only the non-empty pulse gets an entry");
        assert!(
            (etz[0] - 0.0).abs() < 1e-12,
            "First written pulse must be time-origin zero, got {}",
            etz[0]
        );
    }

    #[test]
    fn test_empty_pulse_does_not_shift_duration() {
        // Empty pulse at tdc=5000, two valid pulses at tdc=1000 and tdc=2000.
        // max_pulse_ns should be based on tdc=2000 (not tdc=5000), so
        // duration should reflect only the written pulses.
        let opts = make_test_options();
        let file = NamedTempFile::new().unwrap();
        let path = file.path();

        let mut sink = SnsEventSink::create(path, opts).unwrap();

        // Pulse 1 (tdc=1000): valid hit
        sink.write_hits(0, &make_hit_batch(1000, &[5], &[3], &[100]))
            .unwrap();

        // Pulse 2 (tdc=2000): valid hit
        sink.write_hits(0, &make_hit_batch(2000, &[5], &[3], &[200]))
            .unwrap();

        // Pulse 3 (tdc=5000): all gap hits → 0 written events
        sink.write_hits(0, &make_hit_batch(5000, &[256], &[0], &[300]))
            .unwrap();

        sink.finalize().unwrap();

        let f = File::open(path).unwrap();
        let entry = f.group("entry").unwrap();

        let duration: f64 = entry.dataset("duration").unwrap().read_scalar().unwrap();
        // Duration should be (2000 - 1000) * 25 ns = 25 µs = 25e-6 s
        let expected = f64::from(2000 - 1000) * 25e-9;
        assert!(
            (duration - expected).abs() < 1e-12,
            "Duration must reflect only written pulses: expected {expected}, got {duration}"
        );
    }

    // --- TDC rebase monotonicity (Bug 1) ---

    #[test]
    fn test_monotonic_rebased_timestamps_accepted() {
        // Simulates what the CLI rebase does: File 1 has timestamps
        // 1000→2000, File 2 has timestamps 500→1500 rebased to 2001→2501.
        // The sink should accept all four pulses without error.
        let opts = make_test_options();
        let file = NamedTempFile::new().unwrap();
        let path = file.path();

        let mut sink = SnsEventSink::create(path, opts).unwrap();

        // File 1 timestamps (no offset)
        sink.write_hits(0, &make_hit_batch(1000, &[0], &[0], &[100]))
            .unwrap();
        sink.write_hits(0, &make_hit_batch(2000, &[1], &[1], &[200]))
            .unwrap();

        // File 2 timestamps rebased: 500 + 2001 = 2501, 1500 + 2001 = 3501
        let tdc_offset: u64 = 2001; // last_tdc_seen(2000) + 1
        sink.write_hits(0, &make_hit_batch(500 + tdc_offset, &[2], &[2], &[300]))
            .unwrap();
        sink.write_hits(0, &make_hit_batch(1500 + tdc_offset, &[3], &[3], &[400]))
            .unwrap();

        sink.finalize().unwrap();

        let f = File::open(path).unwrap();
        let entry = f.group("entry").unwrap();

        let tp: u64 = entry
            .dataset("total_pulses")
            .unwrap()
            .read_scalar()
            .unwrap();
        assert_eq!(tp, 4);

        let tc: u64 = entry
            .dataset("total_counts")
            .unwrap()
            .read_scalar()
            .unwrap();
        assert_eq!(tc, 4);

        // Verify pulse times are monotonically increasing
        let etz: Vec<f64> = f
            .group("entry/bank100_events")
            .unwrap()
            .dataset("event_time_zero")
            .unwrap()
            .read_raw()
            .unwrap();
        assert_eq!(etz.len(), 4);
        for i in 1..etz.len() {
            assert!(
                etz[i] > etz[i - 1],
                "Pulse times must be monotonically increasing: etz[{}]={} <= etz[{}]={}",
                i,
                etz[i],
                i - 1,
                etz[i - 1]
            );
        }
    }

    // --- Per-bank monotonicity ---

    #[test]
    fn test_sequential_multi_bank_writes_accepted() {
        // Write all pulses for bank 0, then all pulses for bank 1.
        // Bank 1's first pulse (tdc=1000) is less than bank 0's last (tdc=3000),
        // but per-bank ordering is valid.
        let mut opts = make_test_options();
        opts.banks.push(SnsBankConfig {
            name: "bank200".to_string(),
            pixel_id_offset: 2_000_000,
            width: 512,
            height: 512,
            gap_columns: vec![256, 257],
            gap_rows: vec![256, 257],
        });
        let file = NamedTempFile::new().unwrap();
        let path = file.path();

        let mut sink = SnsEventSink::create(path, opts).unwrap();

        // Bank 0: timestamps 1000, 2000, 3000
        sink.write_hits(0, &make_hit_batch(1000, &[0], &[0], &[100]))
            .unwrap();
        sink.write_hits(0, &make_hit_batch(2000, &[1], &[1], &[200]))
            .unwrap();
        sink.write_hits(0, &make_hit_batch(3000, &[2], &[2], &[300]))
            .unwrap();

        // Bank 1: timestamps 1000, 2000, 3000 (restart from 1000 is fine)
        sink.write_hits(1, &make_hit_batch(1000, &[0], &[0], &[100]))
            .unwrap();
        sink.write_hits(1, &make_hit_batch(2000, &[1], &[1], &[200]))
            .unwrap();
        sink.write_hits(1, &make_hit_batch(3000, &[2], &[2], &[300]))
            .unwrap();

        sink.finalize().unwrap();

        let f = File::open(path).unwrap();
        let entry = f.group("entry").unwrap();

        let tc: u64 = entry
            .dataset("total_counts")
            .unwrap()
            .read_scalar()
            .unwrap();
        assert_eq!(tc, 6); // 3 per bank

        let tp: u64 = entry
            .dataset("total_pulses")
            .unwrap()
            .read_scalar()
            .unwrap();
        assert_eq!(tp, 3); // 3 unique pulse timestamps, deduplicated

        // Each bank has 3 events and 3 pulse entries
        for bank_name in &["bank100_events", "bank200_events"] {
            let bank = f.group(&format!("entry/{bank_name}")).unwrap();
            let ids: Vec<u32> = bank.dataset("event_id").unwrap().read_raw().unwrap();
            assert_eq!(ids.len(), 3);
            let etz: Vec<f64> = bank.dataset("event_time_zero").unwrap().read_raw().unwrap();
            assert_eq!(etz.len(), 3);
        }
    }

    #[test]
    fn test_per_bank_non_monotonic_rejected() {
        // Within a single bank, non-monotonic timestamps must still be rejected.
        let mut opts = make_test_options();
        opts.banks.push(SnsBankConfig {
            name: "bank200".to_string(),
            pixel_id_offset: 2_000_000,
            width: 512,
            height: 512,
            gap_columns: vec![256, 257],
            gap_rows: vec![256, 257],
        });
        let file = NamedTempFile::new().unwrap();
        let path = file.path();

        let mut sink = SnsEventSink::create(path, opts).unwrap();

        // Bank 1: timestamps 2000 then 1000 — non-monotonic within same bank
        sink.write_hits(1, &make_hit_batch(2000, &[0], &[0], &[100]))
            .unwrap();
        let result = sink.write_hits(1, &make_hit_batch(1000, &[1], &[1], &[200]));
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("non-monotonic"));
    }

    #[test]
    fn test_later_bank_with_earlier_timestamps() {
        // Bank 0 written first with later timestamps (5000, 6000),
        // then bank 1 written with earlier timestamps (1000, 2000).
        // event_time_zero must use global min (1000*25=25000 ns) as origin,
        // so bank 0's first pulse is at (5000*25 - 1000*25)/1e9 seconds
        // and bank 1's first pulse is at 0.0.
        let mut opts = make_test_options();
        opts.banks.push(SnsBankConfig {
            name: "bank200".to_string(),
            pixel_id_offset: 2_000_000,
            width: 512,
            height: 512,
            gap_columns: vec![256, 257],
            gap_rows: vec![256, 257],
        });
        let file = NamedTempFile::new().unwrap();
        let path = file.path();

        let mut sink = SnsEventSink::create(path, opts).unwrap();

        // Bank 0: later timestamps
        sink.write_hits(0, &make_hit_batch(5000, &[0], &[0], &[100]))
            .unwrap();
        sink.write_hits(0, &make_hit_batch(6000, &[1], &[1], &[200]))
            .unwrap();

        // Bank 1: earlier timestamps (this used to underflow/fail)
        sink.write_hits(1, &make_hit_batch(1000, &[0], &[0], &[100]))
            .unwrap();
        sink.write_hits(1, &make_hit_batch(2000, &[1], &[1], &[200]))
            .unwrap();

        sink.finalize().unwrap();

        let f = File::open(path).unwrap();

        // Bank 1 has the global minimum (1000*25ns = 25000ns),
        // so its first pulse should be at t=0.0
        let etz1: Vec<f64> = f
            .group("entry/bank200_events")
            .unwrap()
            .dataset("event_time_zero")
            .unwrap()
            .read_raw()
            .unwrap();
        assert_eq!(etz1.len(), 2);
        assert!(
            etz1[0].abs() < 1e-12,
            "Bank 1's first pulse should be at t=0, got {}",
            etz1[0]
        );
        // Bank 1's second pulse: (2000-1000)*25ns = 25000ns = 25µs
        let expected_1 = (2000.0 - 1000.0) * 25.0 / 1_000_000_000.0;
        assert!(
            (etz1[1] - expected_1).abs() < 1e-12,
            "Expected {expected_1}, got {}",
            etz1[1]
        );

        // Bank 0's first pulse: (5000-1000)*25ns = 100000ns = 100µs
        let etz0: Vec<f64> = f
            .group("entry/bank100_events")
            .unwrap()
            .dataset("event_time_zero")
            .unwrap()
            .read_raw()
            .unwrap();
        assert_eq!(etz0.len(), 2);
        let expected_0_first = (5000.0 - 1000.0) * 25.0 / 1_000_000_000.0;
        assert!(
            (etz0[0] - expected_0_first).abs() < 1e-12,
            "Expected {expected_0_first}, got {}",
            etz0[0]
        );

        // Duration should be from min(1000) to max(6000): 5000*25ns
        let entry = f.group("entry").unwrap();
        let duration: f64 = entry.dataset("duration").unwrap().read_scalar().unwrap();
        let expected_dur = 5000.0 * 25.0 / 1_000_000_000.0;
        assert!(
            (duration - expected_dur).abs() < 1e-12,
            "Expected duration {expected_dur}, got {duration}"
        );
    }

    // --- Post-finalization guard ---

    #[test]
    fn test_write_hits_after_finalize_rejected() {
        let opts = make_test_options();
        let file = NamedTempFile::new().unwrap();
        let path = file.path();

        let mut sink = SnsEventSink::create(path, opts).unwrap();
        sink.write_hits(0, &make_hit_batch(1000, &[0], &[0], &[100]))
            .unwrap();
        sink.finalize().unwrap();

        let err = sink
            .write_hits(0, &make_hit_batch(2000, &[1], &[1], &[200]))
            .unwrap_err();
        assert!(
            err.to_string().contains("finalization"),
            "Expected finalization error, got: {err}"
        );
    }

    #[test]
    fn test_write_neutrons_after_finalize_rejected() {
        let opts = make_test_options();
        let file = NamedTempFile::new().unwrap();
        let path = file.path();

        let mut sink = SnsEventSink::create(path, opts).unwrap();
        sink.write_neutrons(0, &make_neutron_batch(1000, &[0.5], &[0.5], &[100]))
            .unwrap();
        sink.finalize().unwrap();

        let err = sink
            .write_neutrons(0, &make_neutron_batch(2000, &[1.5], &[1.5], &[200]))
            .unwrap_err();
        assert!(
            err.to_string().contains("finalization"),
            "Expected finalization error, got: {err}"
        );
    }
}
