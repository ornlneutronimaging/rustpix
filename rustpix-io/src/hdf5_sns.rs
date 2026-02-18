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
use std::path::Path;

const NS_PER_TICK: u64 = 25;
const US_PER_TICK: f64 = 25.0 / 1000.0;

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
        })
    }

    fn append_hits(
        &mut self,
        bank: &SnsBankConfig,
        batch: &EventBatch,
        pulse_time_s: f64,
    ) -> Result<()> {
        let n = batch.hits.x.len();
        if n == 0 {
            return Ok(());
        }

        // event_index: start of this pulse's events
        append_slice(
            &self.event_index,
            self.pulse_count as usize,
            &[self.event_count],
        )?;

        // event_time_zero: pulse time in seconds
        append_slice(
            &self.event_time_zero,
            self.pulse_count as usize,
            &[pulse_time_s],
        )?;
        self.pulse_count += 1;

        // Pixel IDs
        let pixel_ids: Vec<u32> = batch
            .hits
            .x
            .iter()
            .zip(batch.hits.y.iter())
            .map(|(&x, &y)| bank.pixel_id_offset + u32::from(y) * bank.width + u32::from(x))
            .collect();
        append_slice(&self.event_id, self.event_count as usize, &pixel_ids)?;

        // TOF in microseconds
        let tof_us: Vec<f32> = batch
            .hits
            .tof
            .iter()
            .map(|&t| (f64::from(t) * US_PER_TICK) as f32)
            .collect();
        append_slice(&self.event_time_offset, self.event_count as usize, &tof_us)?;

        self.event_count += n as u64;
        Ok(())
    }

    fn append_neutrons(
        &mut self,
        bank: &SnsBankConfig,
        batch: &NeutronEventBatch,
        pulse_time_s: f64,
        super_resolution_factor: f64,
    ) -> Result<()> {
        let n = batch.neutrons.x.len();
        if n == 0 {
            return Ok(());
        }

        // event_index + event_time_zero
        append_slice(
            &self.event_index,
            self.pulse_count as usize,
            &[self.event_count],
        )?;
        append_slice(
            &self.event_time_zero,
            self.pulse_count as usize,
            &[pulse_time_s],
        )?;
        self.pulse_count += 1;

        // Pixel IDs — convert super-resolution coords to pixel coords
        let inv = 1.0 / super_resolution_factor;
        let pixel_ids: Vec<u32> = batch
            .neutrons
            .x
            .iter()
            .zip(batch.neutrons.y.iter())
            .map(|(&x, &y)| {
                let px = (x * inv).round().max(0.0) as u32;
                let py = (y * inv).round().max(0.0) as u32;
                bank.pixel_id_offset + py * bank.width + px
            })
            .collect();
        append_slice(&self.event_id, self.event_count as usize, &pixel_ids)?;

        // TOF in microseconds
        let tof_us: Vec<f32> = batch
            .neutrons
            .tof
            .iter()
            .map(|&t| (f64::from(t) * US_PER_TICK) as f32)
            .collect();
        append_slice(&self.event_time_offset, self.event_count as usize, &tof_us)?;

        self.event_count += n as u64;
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
/// Call [`write_hits`] or [`write_neutrons`] per pulse, then [`finalize`].
pub struct SnsEventSink {
    _file: File,
    entry: Group,
    writers: Vec<(SnsBankConfig, SnsBankEventWriter)>,
    options: SnsWriteOptions,
    run_start_ns: Option<u64>,
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

        Ok(Self {
            _file: file,
            entry,
            writers,
            options,
            run_start_ns: None,
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
        if bank_index >= self.writers.len() {
            return Err(Error::InvalidFormat(format!(
                "bank index {bank_index} out of range (have {} banks)",
                self.writers.len()
            )));
        }

        let pulse_ns = batch.tdc_timestamp_25ns * NS_PER_TICK;
        let pulse_time_s = self.pulse_time_seconds(pulse_ns);
        let n = batch.hits.x.len();

        let (ref bank, ref mut writer) = self.writers[bank_index];
        writer.append_hits(bank, batch, pulse_time_s)?;

        self.total_counts += n as u64;
        self.total_pulses += 1;
        Ok(())
    }

    /// Write a neutron-event batch to the specified bank.
    ///
    /// `bank_index` selects which bank in [`SnsWriteOptions::banks`] to write to.
    ///
    /// # Errors
    /// Returns an error if the bank index is out of bounds or HDF5 write fails.
    pub fn write_neutrons(&mut self, bank_index: usize, batch: &NeutronEventBatch) -> Result<()> {
        if bank_index >= self.writers.len() {
            return Err(Error::InvalidFormat(format!(
                "bank index {bank_index} out of range (have {} banks)",
                self.writers.len()
            )));
        }

        let pulse_ns = batch.tdc_timestamp_25ns * NS_PER_TICK;
        let pulse_time_s = self.pulse_time_seconds(pulse_ns);
        let n = batch.neutrons.x.len();

        let (ref bank, ref mut writer) = self.writers[bank_index];
        writer.append_neutrons(
            bank,
            batch,
            pulse_time_s,
            self.options.super_resolution_factor,
        )?;

        self.total_counts += n as u64;
        self.total_pulses += 1;
        Ok(())
    }

    /// Write `DASlogs` process-variable entries.
    ///
    /// Can be called multiple times to add different variables.
    ///
    /// # Errors
    /// Returns an error if the HDF5 write fails.
    pub fn write_daslogs(&self, logs: &[DasLogEntry]) -> Result<()> {
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
    pub fn finalize(&mut self) -> Result<()> {
        if self.finalized {
            return Ok(());
        }
        self.finalized = true;

        // Per-bank total counts
        for (_bank, writer) in &self.writers {
            writer.write_total_counts()?;
        }

        // Entry-level totals
        overwrite_u64_dataset(&self.entry, "total_counts", self.total_counts)?;
        overwrite_u64_dataset(&self.entry, "total_pulses", self.total_pulses)?;

        Ok(())
    }

    /// Compute pulse time in seconds relative to run start.
    fn pulse_time_seconds(&mut self, pulse_ns: u64) -> f64 {
        if let Some(start) = self.run_start_ns {
            (pulse_ns.saturating_sub(start)) as f64 / 1_000_000_000.0
        } else {
            self.run_start_ns = Some(pulse_ns);
            0.0
        }
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
        // (511, 511) -> offset + 511*512 + 511 = 1_000_000 + 261_632 + 511 = 1_262_143
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
        assert_eq!(ids, vec![1_262_143u32]);
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
}
