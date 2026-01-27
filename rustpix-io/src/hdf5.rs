//! HDF5/NeXus event I/O (`NXevent_data`).

use crate::reader::EventBatch;
use crate::{Error, Result};
use hdf5::types::{H5Type, VarLenUnicode};
use hdf5::{Dataset, File, Group};
use ndarray::{s, ArrayView, ArrayView1};
use rustpix_core::neutron::NeutronBatch;
use rustpix_tpx::DetectorConfig;
use std::path::Path;
use std::str::FromStr;

const NS_PER_TICK: u64 = 25;
const HISTOGRAM_AXES: [&str; 4] = ["rot_angle", "y", "x", "time_of_flight"];

/// Streaming writer for hit events in `NXevent_data`.
pub struct Hdf5HitSink {
    _file: File,
    writer: HitEventWriter,
    options: HitWriteOptions,
}

impl Hdf5HitSink {
    /// Create a new streaming hit sink.
    ///
    /// # Errors
    /// Returns an error if the HDF5 file or datasets cannot be created.
    pub fn create<P: AsRef<Path>>(path: P, options: HitWriteOptions) -> Result<Self> {
        let file = File::create(path)?;
        set_attr_str_file(&file, "rustpix_format_version", "0.1")?;

        let entry = file.create_group("entry")?;
        set_attr_str_group(&entry, "NX_class", "NXentry")?;
        set_conversion_attrs(
            &entry,
            options.flight_path_m,
            options.tof_offset_ns,
            options.energy_axis_kind.as_deref(),
        )?;

        let hits = entry.create_group("hits")?;
        set_attr_str_group(&hits, "NX_class", "NXevent_data")?;
        hits.new_attr::<u32>()
            .create("x_size")?
            .write_scalar(&options.x_size)?;
        hits.new_attr::<u32>()
            .create("y_size")?
            .write_scalar(&options.y_size)?;
        set_conversion_attrs(
            &hits,
            options.flight_path_m,
            options.tof_offset_ns,
            options.energy_axis_kind.as_deref(),
        )?;

        let writer = HitEventWriter::new(&hits, &options)?;
        Ok(Self {
            _file: file,
            writer,
            options,
        })
    }

    /// Append a hit batch.
    ///
    /// # Errors
    /// Returns an error if HDF5 I/O fails.
    pub fn write_hits(&mut self, batch: &EventBatch) -> Result<()> {
        self.writer.append_batch(batch, &self.options)
    }
}

/// Streaming writer for neutron events in `NXevent_data`.
pub struct Hdf5NeutronSink {
    _file: File,
    writer: NeutronEventWriter,
    options: NeutronWriteOptions,
}

impl Hdf5NeutronSink {
    /// Create a new streaming neutron sink.
    ///
    /// # Errors
    /// Returns an error if the HDF5 file or datasets cannot be created.
    pub fn create<P: AsRef<Path>>(path: P, options: NeutronWriteOptions) -> Result<Self> {
        let file = File::create(path)?;
        set_attr_str_file(&file, "rustpix_format_version", "0.1")?;

        let entry = file.create_group("entry")?;
        set_attr_str_group(&entry, "NX_class", "NXentry")?;
        set_conversion_attrs(
            &entry,
            options.flight_path_m,
            options.tof_offset_ns,
            options.energy_axis_kind.as_deref(),
        )?;

        let neutrons = entry.create_group("neutrons")?;
        set_attr_str_group(&neutrons, "NX_class", "NXevent_data")?;
        neutrons
            .new_attr::<u32>()
            .create("x_size")?
            .write_scalar(&options.x_size)?;
        neutrons
            .new_attr::<u32>()
            .create("y_size")?
            .write_scalar(&options.y_size)?;
        set_conversion_attrs(
            &neutrons,
            options.flight_path_m,
            options.tof_offset_ns,
            options.energy_axis_kind.as_deref(),
        )?;

        let writer = NeutronEventWriter::new(&neutrons, &options)?;
        Ok(Self {
            _file: file,
            writer,
            options,
        })
    }

    /// Append a neutron batch.
    ///
    /// # Errors
    /// Returns an error if HDF5 I/O fails.
    pub fn write_neutrons(&mut self, batch: &NeutronEventBatch) -> Result<()> {
        self.writer.append_batch(batch, &self.options)
    }
}

/// Event write configuration for hits.
#[derive(Clone, Debug)]
#[allow(clippy::struct_excessive_bools)]
pub struct HitWriteOptions {
    pub x_size: u32,
    pub y_size: u32,
    pub chunk_events: usize,
    pub compression: Option<u8>,
    pub shuffle: bool,
    pub flight_path_m: Option<f64>,
    pub tof_offset_ns: Option<f64>,
    pub energy_axis_kind: Option<String>,
    pub include_xy: bool,
    pub include_tot: bool,
    pub include_chip_id: bool,
    pub include_cluster_id: bool,
}

impl HitWriteOptions {
    /// Build write options from detector config and defaults.
    #[must_use]
    pub fn from_detector_config(config: &DetectorConfig) -> Self {
        let (x_size, y_size) = detector_size(config);
        Self {
            x_size,
            y_size,
            chunk_events: 100_000,
            compression: Some(1),
            shuffle: true,
            flight_path_m: None,
            tof_offset_ns: None,
            energy_axis_kind: Some("tof".to_string()),
            include_xy: true,
            include_tot: true,
            include_chip_id: true,
            include_cluster_id: true,
        }
    }
}

/// Event write configuration for neutrons.
#[derive(Clone, Debug)]
#[allow(clippy::struct_excessive_bools)]
pub struct NeutronWriteOptions {
    pub x_size: u32,
    pub y_size: u32,
    pub chunk_events: usize,
    pub compression: Option<u8>,
    pub shuffle: bool,
    pub flight_path_m: Option<f64>,
    pub tof_offset_ns: Option<f64>,
    pub energy_axis_kind: Option<String>,
    pub include_xy: bool,
    pub include_tot: bool,
    pub include_chip_id: bool,
    pub include_n_hits: bool,
}

impl NeutronWriteOptions {
    /// Build write options from detector config and defaults.
    #[must_use]
    pub fn from_detector_config(config: &DetectorConfig) -> Self {
        let (x_size, y_size) = detector_size(config);
        Self {
            x_size,
            y_size,
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
        }
    }
}

/// Event data loaded from an `NXevent_data` group (hits).
#[derive(Clone, Debug)]
pub struct HitEventData {
    pub event_id: Vec<i32>,
    pub event_time_offset_ns: Vec<u64>,
    pub event_time_zero_ns: Vec<u64>,
    pub event_index: Vec<i32>,
    pub time_over_threshold_ns: Option<Vec<u64>>,
    pub chip_id: Option<Vec<u8>>,
    pub cluster_id: Option<Vec<i32>>,
    pub x: Option<Vec<u16>>,
    pub y: Option<Vec<u16>>,
    pub attrs: EventAttributes,
}

/// Event data loaded from an `NXevent_data` group (neutrons).
#[derive(Clone, Debug)]
pub struct NeutronEventData {
    pub event_id: Vec<i32>,
    pub event_time_offset_ns: Vec<u64>,
    pub event_time_zero_ns: Vec<u64>,
    pub event_index: Vec<i32>,
    pub time_over_threshold_ns: Option<Vec<u64>>,
    pub chip_id: Option<Vec<u8>>,
    pub n_hits: Option<Vec<u16>>,
    pub x: Option<Vec<u16>>,
    pub y: Option<Vec<u16>>,
    pub attrs: EventAttributes,
}

/// Event group attributes.
#[derive(Clone, Debug, Default)]
pub struct EventAttributes {
    pub x_size: Option<u32>,
    pub y_size: Option<u32>,
    pub flight_path_m: Option<f64>,
    pub tof_offset_ns: Option<f64>,
    pub energy_axis_kind: Option<String>,
}

/// Neutron event batch with pulse timestamp.
#[derive(Clone, Debug)]
pub struct NeutronEventBatch {
    pub tdc_timestamp_25ns: u64,
    pub neutrons: NeutronBatch,
}

fn detector_size(config: &DetectorConfig) -> (u32, u32) {
    if config.chip_transforms.is_empty() {
        return (u32::from(config.chip_size_x), u32::from(config.chip_size_y));
    }

    let mut max_x: u32 = 0;
    let mut max_y: u32 = 0;
    let max_local_x = config.chip_size_x.saturating_sub(1);
    let max_local_y = config.chip_size_y.saturating_sub(1);

    for transform in &config.chip_transforms {
        let corners = [
            transform.apply(0, 0),
            transform.apply(max_local_x, 0),
            transform.apply(0, max_local_y),
            transform.apply(max_local_x, max_local_y),
        ];

        for (gx, gy) in corners {
            max_x = max_x.max(u32::from(gx));
            max_y = max_y.max(u32::from(gy));
        }
    }

    (max_x + 1, max_y + 1)
}

/// Writes hit events to an HDF5/NeXus file.
///
/// # Errors
/// Returns an error if HDF5 I/O fails or indices overflow i32.
pub fn write_hits_hdf5<P, I>(path: P, batches: I, options: &HitWriteOptions) -> Result<()>
where
    P: AsRef<Path>,
    I: IntoIterator<Item = EventBatch>,
{
    let file = File::create(path)?;
    set_attr_str_file(&file, "rustpix_format_version", "0.1")?;

    let entry = file.create_group("entry")?;
    set_attr_str_group(&entry, "NX_class", "NXentry")?;
    set_conversion_attrs(
        &entry,
        options.flight_path_m,
        options.tof_offset_ns,
        options.energy_axis_kind.as_deref(),
    )?;

    let hits = entry.create_group("hits")?;
    set_attr_str_group(&hits, "NX_class", "NXevent_data")?;
    hits.new_attr::<u32>()
        .create("x_size")?
        .write_scalar(&options.x_size)?;
    hits.new_attr::<u32>()
        .create("y_size")?
        .write_scalar(&options.y_size)?;
    set_conversion_attrs(
        &hits,
        options.flight_path_m,
        options.tof_offset_ns,
        options.energy_axis_kind.as_deref(),
    )?;

    let mut writer = HitEventWriter::new(&hits, options)?;
    for batch in batches {
        writer.append_batch(&batch, options)?;
    }
    Ok(())
}

/// Writes neutron events to an HDF5/NeXus file.
///
/// # Errors
/// Returns an error if HDF5 I/O fails or indices overflow i32.
pub fn write_neutrons_hdf5<P, I>(path: P, batches: I, options: &NeutronWriteOptions) -> Result<()>
where
    P: AsRef<Path>,
    I: IntoIterator<Item = NeutronEventBatch>,
{
    let file = File::create(path)?;
    set_attr_str_file(&file, "rustpix_format_version", "0.1")?;

    let entry = file.create_group("entry")?;
    set_attr_str_group(&entry, "NX_class", "NXentry")?;
    set_conversion_attrs(
        &entry,
        options.flight_path_m,
        options.tof_offset_ns,
        options.energy_axis_kind.as_deref(),
    )?;

    let neutrons = entry.create_group("neutrons")?;
    set_attr_str_group(&neutrons, "NX_class", "NXevent_data")?;
    neutrons
        .new_attr::<u32>()
        .create("x_size")?
        .write_scalar(&options.x_size)?;
    neutrons
        .new_attr::<u32>()
        .create("y_size")?
        .write_scalar(&options.y_size)?;
    set_conversion_attrs(
        &neutrons,
        options.flight_path_m,
        options.tof_offset_ns,
        options.energy_axis_kind.as_deref(),
    )?;

    let mut writer = NeutronEventWriter::new(&neutrons, options)?;
    for batch in batches {
        writer.append_batch(&batch, options)?;
    }
    Ok(())
}

/// Reads hit events from an HDF5/NeXus file.
///
/// # Errors
/// Returns an error if HDF5 I/O fails.
pub fn read_hits_hdf5<P: AsRef<Path>>(path: P) -> Result<HitEventData> {
    let file = File::open(path)?;
    let entry = file.group("entry")?;
    let hits = entry.group("hits")?;
    read_hit_event_group(&entry, &hits)
}

/// Reads neutron events from an HDF5/NeXus file.
///
/// # Errors
/// Returns an error if HDF5 I/O fails.
pub fn read_neutrons_hdf5<P: AsRef<Path>>(path: P) -> Result<NeutronEventData> {
    let file = File::open(path)?;
    let entry = file.group("entry")?;
    let neutrons = entry.group("neutrons")?;
    read_neutron_event_group(&entry, &neutrons)
}

fn read_hit_event_group(entry: &Group, group: &Group) -> Result<HitEventData> {
    let event_id = read_dataset_vec::<i32>(group, "event_id")?;
    let event_time_offset_ns = read_dataset_vec::<u64>(group, "event_time_offset")?;
    let event_time_zero_ns = read_dataset_vec::<u64>(group, "event_time_zero")?;
    let event_index = read_dataset_vec::<i32>(group, "event_index")?;

    let time_over_threshold_ns = read_dataset_vec_opt::<u64>(group, "time_over_threshold")?;
    let chip_id = read_dataset_vec_opt::<u8>(group, "chip_id")?;
    let cluster_id = read_dataset_vec_opt::<i32>(group, "cluster_id")?;
    let x = read_dataset_vec_opt::<u16>(group, "x")?;
    let y = read_dataset_vec_opt::<u16>(group, "y")?;

    let attrs = read_event_attrs(entry, group)?;

    Ok(HitEventData {
        event_id,
        event_time_offset_ns,
        event_time_zero_ns,
        event_index,
        time_over_threshold_ns,
        chip_id,
        cluster_id,
        x,
        y,
        attrs,
    })
}

fn read_neutron_event_group(entry: &Group, group: &Group) -> Result<NeutronEventData> {
    let event_id = read_dataset_vec::<i32>(group, "event_id")?;
    let event_time_offset_ns = read_dataset_vec::<u64>(group, "event_time_offset")?;
    let event_time_zero_ns = read_dataset_vec::<u64>(group, "event_time_zero")?;
    let event_index = read_dataset_vec::<i32>(group, "event_index")?;

    let time_over_threshold_ns = read_dataset_vec_opt::<u64>(group, "time_over_threshold")?;
    let chip_id = read_dataset_vec_opt::<u8>(group, "chip_id")?;
    let n_hits = read_dataset_vec_opt::<u16>(group, "n_hits")?;
    let x = read_dataset_vec_opt::<u16>(group, "x")?;
    let y = read_dataset_vec_opt::<u16>(group, "y")?;

    let attrs = read_event_attrs(entry, group)?;

    Ok(NeutronEventData {
        event_id,
        event_time_offset_ns,
        event_time_zero_ns,
        event_index,
        time_over_threshold_ns,
        chip_id,
        n_hits,
        x,
        y,
        attrs,
    })
}

fn read_event_attrs(entry: &Group, group: &Group) -> Result<EventAttributes> {
    let mut attrs = EventAttributes {
        x_size: read_attr_opt::<u32>(group, "x_size")?,
        y_size: read_attr_opt::<u32>(group, "y_size")?,
        flight_path_m: read_attr_opt::<f64>(entry, "flight_path_m")?,
        tof_offset_ns: read_attr_opt::<f64>(entry, "tof_offset_ns")?,
        energy_axis_kind: read_attr_opt_string(entry, "energy_axis_kind")?,
    };

    if let Some(value) = read_attr_opt::<f64>(group, "flight_path_m")? {
        attrs.flight_path_m = Some(value);
    }
    if let Some(value) = read_attr_opt::<f64>(group, "tof_offset_ns")? {
        attrs.tof_offset_ns = Some(value);
    }
    if let Some(value) = read_attr_opt_string(group, "energy_axis_kind")? {
        attrs.energy_axis_kind = Some(value);
    }

    Ok(attrs)
}

struct HitEventWriter {
    event_id: Dataset,
    event_time_offset: Dataset,
    event_time_zero: Dataset,
    event_index: Dataset,
    time_over_threshold: Option<Dataset>,
    chip_id: Option<Dataset>,
    cluster_id: Option<Dataset>,
    x: Option<Dataset>,
    y: Option<Dataset>,
    event_count: usize,
    pulse_count: usize,
}

impl HitEventWriter {
    #[allow(clippy::too_many_lines)]
    fn new(group: &Group, options: &HitWriteOptions) -> Result<Self> {
        let event_id = create_extendable_dataset::<i32>(
            group,
            "event_id",
            options.chunk_events,
            options.compression,
            options.shuffle,
        )?;
        let event_time_offset = create_extendable_dataset::<u64>(
            group,
            "event_time_offset",
            options.chunk_events,
            options.compression,
            options.shuffle,
        )?;
        let event_time_zero = create_extendable_dataset::<u64>(
            group,
            "event_time_zero",
            options.chunk_events,
            options.compression,
            options.shuffle,
        )?;
        let event_index = create_extendable_dataset::<i32>(
            group,
            "event_index",
            options.chunk_events,
            options.compression,
            options.shuffle,
        )?;

        let time_over_threshold = if options.include_tot {
            Some(create_extendable_dataset::<u64>(
                group,
                "time_over_threshold",
                options.chunk_events,
                options.compression,
                options.shuffle,
            )?)
        } else {
            None
        };

        let chip_id = if options.include_chip_id {
            Some(create_extendable_dataset::<u8>(
                group,
                "chip_id",
                options.chunk_events,
                options.compression,
                options.shuffle,
            )?)
        } else {
            None
        };

        let cluster_id = if options.include_cluster_id {
            Some(create_extendable_dataset::<i32>(
                group,
                "cluster_id",
                options.chunk_events,
                options.compression,
                options.shuffle,
            )?)
        } else {
            None
        };

        let x = if options.include_xy {
            Some(create_extendable_dataset::<u16>(
                group,
                "x",
                options.chunk_events,
                options.compression,
                options.shuffle,
            )?)
        } else {
            None
        };

        let y = if options.include_xy {
            Some(create_extendable_dataset::<u16>(
                group,
                "y",
                options.chunk_events,
                options.compression,
                options.shuffle,
            )?)
        } else {
            None
        };

        set_dataset_units(&event_id, "id")?;
        set_dataset_units(&event_time_offset, "ns")?;
        set_dataset_units(&event_time_zero, "ns")?;
        set_dataset_units(&event_index, "id")?;
        if let Some(ds) = &time_over_threshold {
            set_dataset_units(ds, "ns")?;
        }
        if let Some(ds) = &chip_id {
            set_dataset_units(ds, "id")?;
        }
        if let Some(ds) = &cluster_id {
            set_dataset_units(ds, "id")?;
        }
        if let Some(ds) = &x {
            set_dataset_units(ds, "pixel")?;
        }
        if let Some(ds) = &y {
            set_dataset_units(ds, "pixel")?;
        }

        Ok(Self {
            event_id,
            event_time_offset,
            event_time_zero,
            event_index,
            time_over_threshold,
            chip_id,
            cluster_id,
            x,
            y,
            event_count: 0,
            pulse_count: 0,
        })
    }

    fn append_batch(&mut self, batch: &EventBatch, options: &HitWriteOptions) -> Result<()> {
        let count = batch.hits.len();
        if count == 0 {
            return Ok(());
        }

        let event_start = self.event_count;
        let event_end = event_start + count;

        let event_index = i32::try_from(event_start).map_err(|_| {
            Error::InvalidFormat(
                "event_index exceeds i32 range; split file or reduce events".to_string(),
            )
        })?;

        let tdc_ns = batch.tdc_timestamp_25ns.saturating_mul(NS_PER_TICK);

        let mut event_id = Vec::with_capacity(count);
        for (&x, &y) in batch.hits.x.iter().zip(batch.hits.y.iter()) {
            let id = u32::from(y) * options.x_size + u32::from(x);
            let id = i32::try_from(id).map_err(|_| {
                Error::InvalidFormat("event_id exceeds i32 range; adjust x_size/y_size".to_string())
            })?;
            event_id.push(id);
        }

        let mut event_time_offset_ns = Vec::with_capacity(count);
        for &tof in &batch.hits.tof {
            event_time_offset_ns.push(u64::from(tof) * NS_PER_TICK);
        }

        append_slice(&self.event_id, event_start, &event_id)?;
        append_slice(&self.event_time_offset, event_start, &event_time_offset_ns)?;

        if let Some(ds) = &self.time_over_threshold {
            let mut values = Vec::with_capacity(count);
            for &tot in &batch.hits.tot {
                values.push(u64::from(tot) * NS_PER_TICK);
            }
            append_slice(ds, event_start, &values)?;
        }

        if let Some(ds) = &self.chip_id {
            append_slice(ds, event_start, &batch.hits.chip_id)?;
        }

        if let Some(ds) = &self.cluster_id {
            append_slice(ds, event_start, &batch.hits.cluster_id)?;
        }

        if let Some(ds) = &self.x {
            append_slice(ds, event_start, &batch.hits.x)?;
        }

        if let Some(ds) = &self.y {
            append_slice(ds, event_start, &batch.hits.y)?;
        }

        append_slice(&self.event_time_zero, self.pulse_count, &[tdc_ns])?;
        append_slice(&self.event_index, self.pulse_count, &[event_index])?;

        self.event_count = event_end;
        self.pulse_count += 1;
        Ok(())
    }
}

struct NeutronEventWriter {
    event_id: Dataset,
    event_time_offset: Dataset,
    event_time_zero: Dataset,
    event_index: Dataset,
    time_over_threshold: Option<Dataset>,
    chip_id: Option<Dataset>,
    n_hits: Option<Dataset>,
    x: Option<Dataset>,
    y: Option<Dataset>,
    event_count: usize,
    pulse_count: usize,
}

impl NeutronEventWriter {
    #[allow(clippy::too_many_lines)]
    fn new(group: &Group, options: &NeutronWriteOptions) -> Result<Self> {
        let event_id = create_extendable_dataset::<i32>(
            group,
            "event_id",
            options.chunk_events,
            options.compression,
            options.shuffle,
        )?;
        let event_time_offset = create_extendable_dataset::<u64>(
            group,
            "event_time_offset",
            options.chunk_events,
            options.compression,
            options.shuffle,
        )?;
        let event_time_zero = create_extendable_dataset::<u64>(
            group,
            "event_time_zero",
            options.chunk_events,
            options.compression,
            options.shuffle,
        )?;
        let event_index = create_extendable_dataset::<i32>(
            group,
            "event_index",
            options.chunk_events,
            options.compression,
            options.shuffle,
        )?;

        let time_over_threshold = if options.include_tot {
            Some(create_extendable_dataset::<u64>(
                group,
                "time_over_threshold",
                options.chunk_events,
                options.compression,
                options.shuffle,
            )?)
        } else {
            None
        };

        let chip_id = if options.include_chip_id {
            Some(create_extendable_dataset::<u8>(
                group,
                "chip_id",
                options.chunk_events,
                options.compression,
                options.shuffle,
            )?)
        } else {
            None
        };

        let n_hits = if options.include_n_hits {
            Some(create_extendable_dataset::<u16>(
                group,
                "n_hits",
                options.chunk_events,
                options.compression,
                options.shuffle,
            )?)
        } else {
            None
        };

        let x = if options.include_xy {
            Some(create_extendable_dataset::<u16>(
                group,
                "x",
                options.chunk_events,
                options.compression,
                options.shuffle,
            )?)
        } else {
            None
        };

        let y = if options.include_xy {
            Some(create_extendable_dataset::<u16>(
                group,
                "y",
                options.chunk_events,
                options.compression,
                options.shuffle,
            )?)
        } else {
            None
        };

        set_dataset_units(&event_id, "id")?;
        set_dataset_units(&event_time_offset, "ns")?;
        set_dataset_units(&event_time_zero, "ns")?;
        set_dataset_units(&event_index, "id")?;
        if let Some(ds) = &time_over_threshold {
            set_dataset_units(ds, "ns")?;
        }
        if let Some(ds) = &chip_id {
            set_dataset_units(ds, "id")?;
        }
        if let Some(ds) = &n_hits {
            set_dataset_units(ds, "count")?;
        }
        if let Some(ds) = &x {
            set_dataset_units(ds, "pixel")?;
        }
        if let Some(ds) = &y {
            set_dataset_units(ds, "pixel")?;
        }

        Ok(Self {
            event_id,
            event_time_offset,
            event_time_zero,
            event_index,
            time_over_threshold,
            chip_id,
            n_hits,
            x,
            y,
            event_count: 0,
            pulse_count: 0,
        })
    }

    fn append_batch(
        &mut self,
        batch: &NeutronEventBatch,
        options: &NeutronWriteOptions,
    ) -> Result<()> {
        let count = batch.neutrons.len();
        if count == 0 {
            return Ok(());
        }

        let event_start = self.event_count;
        let event_end = event_start + count;

        let event_index = i32::try_from(event_start).map_err(|_| {
            Error::InvalidFormat(
                "event_index exceeds i32 range; split file or reduce events".to_string(),
            )
        })?;

        let tdc_ns = batch.tdc_timestamp_25ns.saturating_mul(NS_PER_TICK);

        let mut x_values = Vec::with_capacity(count);
        let mut y_values = Vec::with_capacity(count);
        let mut event_id = Vec::with_capacity(count);

        for (&x, &y) in batch.neutrons.x.iter().zip(batch.neutrons.y.iter()) {
            let x_pixel = x.round();
            let y_pixel = y.round();

            if x_pixel < 0.0 || y_pixel < 0.0 {
                return Err(Error::InvalidFormat(
                    "neutron x/y must be non-negative for event_id mapping".to_string(),
                ));
            }

            let x_u32 = {
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                {
                    x_pixel as u32
                }
            };
            let y_u32 = {
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                {
                    y_pixel as u32
                }
            };

            if x_u32 >= options.x_size || y_u32 >= options.y_size {
                return Err(Error::InvalidFormat(
                    "neutron x/y out of detector bounds".to_string(),
                ));
            }

            let x_u16 = u16::try_from(x_u32)
                .map_err(|_| Error::InvalidFormat("neutron x exceeds u16 range".to_string()))?;
            let y_u16 = u16::try_from(y_u32)
                .map_err(|_| Error::InvalidFormat("neutron y exceeds u16 range".to_string()))?;

            let id = y_u32 * options.x_size + x_u32;
            let id = i32::try_from(id).map_err(|_| {
                Error::InvalidFormat("event_id exceeds i32 range; adjust x_size/y_size".to_string())
            })?;

            x_values.push(x_u16);
            y_values.push(y_u16);
            event_id.push(id);
        }

        let mut event_time_offset_ns = Vec::with_capacity(count);
        for &tof in &batch.neutrons.tof {
            event_time_offset_ns.push(u64::from(tof) * NS_PER_TICK);
        }

        append_slice(&self.event_id, event_start, &event_id)?;
        append_slice(&self.event_time_offset, event_start, &event_time_offset_ns)?;

        if let Some(ds) = &self.time_over_threshold {
            let mut values = Vec::with_capacity(count);
            for &tot in &batch.neutrons.tot {
                values.push(u64::from(tot) * NS_PER_TICK);
            }
            append_slice(ds, event_start, &values)?;
        }

        if let Some(ds) = &self.chip_id {
            append_slice(ds, event_start, &batch.neutrons.chip_id)?;
        }

        if let Some(ds) = &self.n_hits {
            append_slice(ds, event_start, &batch.neutrons.n_hits)?;
        }

        if let Some(ds) = &self.x {
            append_slice(ds, event_start, &x_values)?;
        }

        if let Some(ds) = &self.y {
            append_slice(ds, event_start, &y_values)?;
        }

        append_slice(&self.event_time_zero, self.pulse_count, &[tdc_ns])?;
        append_slice(&self.event_index, self.pulse_count, &[event_index])?;

        self.event_count = event_end;
        self.pulse_count += 1;
        Ok(())
    }
}

/// Histogram write configuration.
#[derive(Clone, Debug)]
pub struct HistogramWriteOptions {
    pub chunk_counts: Option<[usize; 4]>,
    pub compression: Option<u8>,
    pub shuffle: bool,
    pub flight_path_m: Option<f64>,
    pub tof_offset_ns: Option<f64>,
    pub energy_axis_kind: Option<String>,
}

impl Default for HistogramWriteOptions {
    fn default() -> Self {
        Self {
            chunk_counts: None,
            compression: Some(1),
            shuffle: true,
            flight_path_m: None,
            tof_offset_ns: None,
            energy_axis_kind: Some("tof".to_string()),
        }
    }
}

/// Histogram shape (`rot_angle`, y, x, `time_of_flight`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HistogramShape {
    pub rot_angle: usize,
    pub y: usize,
    pub x: usize,
    pub time_of_flight: usize,
}

impl HistogramShape {
    #[must_use]
    pub fn len(&self) -> usize {
        self.rot_angle * self.y * self.x * self.time_of_flight
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Histogram data for writing.
#[derive(Clone, Debug)]
pub struct HistogramWriteData {
    pub counts: Vec<u64>,
    pub shape: HistogramShape,
    pub rot_angle: Vec<f64>,
    pub y: Vec<f64>,
    pub x: Vec<f64>,
    pub time_of_flight_ns: Vec<f64>,
}

/// Histogram data loaded from `NXdata`.
#[derive(Clone, Debug)]
pub struct HistogramData {
    pub counts: Vec<u64>,
    pub shape: HistogramShape,
    pub rot_angle: Vec<f64>,
    pub y: Vec<f64>,
    pub x: Vec<f64>,
    pub time_of_flight_ns: Vec<f64>,
    pub energy_ev: Option<Vec<f64>>,
    pub attrs: HistogramAttributes,
}

/// Histogram attributes.
#[derive(Clone, Debug, Default)]
pub struct HistogramAttributes {
    pub flight_path_m: Option<f64>,
    pub tof_offset_ns: Option<f64>,
    pub energy_axis_kind: Option<String>,
}

/// Writes histogram/hyperspectra data to an HDF5/NeXus file.
///
/// # Errors
/// Returns an error if HDF5 I/O fails or shapes are inconsistent.
pub fn write_histogram_hdf5<P: AsRef<Path>>(
    path: P,
    data: &HistogramWriteData,
    options: &HistogramWriteOptions,
) -> Result<()> {
    validate_histogram_data(data)?;

    let file = File::create(path)?;
    set_attr_str_file(&file, "rustpix_format_version", "0.1")?;

    let entry = file.create_group("entry")?;
    set_attr_str_group(&entry, "NX_class", "NXentry")?;
    set_conversion_attrs(
        &entry,
        options.flight_path_m,
        options.tof_offset_ns,
        options.energy_axis_kind.as_deref(),
    )?;

    let histogram = entry.create_group("histogram")?;
    set_attr_str_group(&histogram, "NX_class", "NXdata")?;
    set_attr_str_group(&histogram, "signal", "counts")?;
    set_axes_attr(&histogram, &HISTOGRAM_AXES)?;
    set_axis_indices(&histogram, "rot_angle", 0)?;
    set_axis_indices(&histogram, "y", 1)?;
    set_axis_indices(&histogram, "x", 2)?;
    set_axis_indices(&histogram, "time_of_flight", 3)?;

    set_conversion_attrs(
        &histogram,
        options.flight_path_m,
        options.tof_offset_ns,
        options.energy_axis_kind.as_deref(),
    )?;

    write_histogram_datasets(&histogram, data, options)?;
    Ok(())
}

/// Reads histogram/hyperspectra data from an HDF5/NeXus file.
///
/// # Errors
/// Returns an error if HDF5 I/O fails or required datasets are missing.
pub fn read_histogram_hdf5<P: AsRef<Path>>(path: P) -> Result<HistogramData> {
    let file = File::open(path)?;
    let entry = file.group("entry")?;
    let histogram = entry.group("histogram")?;

    let counts_ds = histogram.dataset("counts")?;
    let shape = counts_ds.shape();
    if shape.len() != 4 {
        return Err(Error::InvalidFormat(
            "counts dataset must be 4-D (rot_angle, y, x, time_of_flight)".to_string(),
        ));
    }

    let shape = HistogramShape {
        rot_angle: shape[0],
        y: shape[1],
        x: shape[2],
        time_of_flight: shape[3],
    };

    let counts = counts_ds.read_raw::<u64>()?;
    if counts.len() != shape.len() {
        return Err(Error::InvalidFormat(
            "counts dataset size does not match shape".to_string(),
        ));
    }

    let rot_angle = read_dataset_vec::<f64>(&histogram, "rot_angle")?;
    let y = read_dataset_vec::<f64>(&histogram, "y")?;
    let x = read_dataset_vec::<f64>(&histogram, "x")?;
    let time_of_flight_ns = read_dataset_vec::<f64>(&histogram, "time_of_flight")?;
    let energy_ev = read_dataset_vec_opt::<f64>(&histogram, "energy_eV")?;

    let attrs = read_histogram_attrs(&entry, &histogram)?;

    Ok(HistogramData {
        counts,
        shape,
        rot_angle,
        y,
        x,
        time_of_flight_ns,
        energy_ev,
        attrs,
    })
}

fn write_histogram_datasets(
    group: &Group,
    data: &HistogramWriteData,
    options: &HistogramWriteOptions,
) -> Result<()> {
    let shape = data.shape;

    let counts_ds = create_fixed_dataset::<u64, _>(
        group,
        "counts",
        (shape.rot_angle, shape.y, shape.x, shape.time_of_flight),
        options.chunk_counts,
        options.compression,
        options.shuffle,
    )?;
    set_dataset_units(&counts_ds, "count")?;

    let counts_view = ArrayView::from_shape(
        (shape.rot_angle, shape.y, shape.x, shape.time_of_flight),
        data.counts.as_slice(),
    )
    .map_err(|e| Error::InvalidFormat(format!("counts shape mismatch: {e}")))?;
    counts_ds.write(counts_view)?;

    let rot_ds = create_fixed_dataset::<f64, _>(
        group,
        "rot_angle",
        (data.rot_angle.len(),),
        None,
        None,
        false,
    )?;
    set_dataset_units(&rot_ds, "deg")?;
    rot_ds.write(ArrayView1::from(data.rot_angle.as_slice()))?;

    let y_ds = create_fixed_dataset::<f64, _>(group, "y", (data.y.len(),), None, None, false)?;
    set_dataset_units(&y_ds, "pixel")?;
    y_ds.write(ArrayView1::from(data.y.as_slice()))?;

    let x_ds = create_fixed_dataset::<f64, _>(group, "x", (data.x.len(),), None, None, false)?;
    set_dataset_units(&x_ds, "pixel")?;
    x_ds.write(ArrayView1::from(data.x.as_slice()))?;

    let tof_ds = create_fixed_dataset::<f64, _>(
        group,
        "time_of_flight",
        (data.time_of_flight_ns.len(),),
        None,
        None,
        false,
    )?;
    set_dataset_units(&tof_ds, "ns")?;
    tof_ds.write(ArrayView1::from(data.time_of_flight_ns.as_slice()))?;

    if let (Some(flight_path_m), Some(tof_offset_ns)) =
        (options.flight_path_m, options.tof_offset_ns)
    {
        let energy = derive_energy_axis_ev(&data.time_of_flight_ns, flight_path_m, tof_offset_ns);
        let energy_ds =
            create_fixed_dataset::<f64, _>(group, "energy_eV", (energy.len(),), None, None, false)?;
        set_dataset_units(&energy_ds, "eV")?;
        energy_ds.write(ArrayView1::from(energy.as_slice()))?;
    }

    Ok(())
}

fn validate_histogram_data(data: &HistogramWriteData) -> Result<()> {
    let shape = data.shape;
    if data.counts.len() != shape.len() {
        return Err(Error::InvalidFormat(
            "counts length does not match shape".to_string(),
        ));
    }

    validate_axis_len("rot_angle", shape.rot_angle, data.rot_angle.len())?;
    validate_axis_len("y", shape.y, data.y.len())?;
    validate_axis_len("x", shape.x, data.x.len())?;
    validate_axis_len(
        "time_of_flight",
        shape.time_of_flight,
        data.time_of_flight_ns.len(),
    )?;
    Ok(())
}

fn validate_axis_len(name: &str, dim: usize, axis_len: usize) -> Result<()> {
    if axis_len != dim && axis_len != dim + 1 {
        return Err(Error::InvalidFormat(format!(
            "axis {name} length {axis_len} must be {dim} or {}",
            dim + 1
        )));
    }
    Ok(())
}

fn create_fixed_dataset<T: H5Type, S>(
    group: &Group,
    name: &str,
    shape: S,
    chunk: Option<[usize; 4]>,
    compression: Option<u8>,
    shuffle: bool,
) -> Result<Dataset>
where
    S: Into<hdf5::Extents>,
{
    let mut builder = group.new_dataset::<T>().shape(shape);

    if let Some(chunk_shape) = chunk {
        builder = builder.chunk(chunk_shape);
    }

    if let Some(level) = compression {
        builder = builder.deflate(level);
    }

    if shuffle {
        builder = builder.shuffle();
    }

    Ok(builder.create(name)?)
}

fn set_axes_attr(group: &Group, axes: &[&str]) -> Result<()> {
    let values: Vec<VarLenUnicode> = axes
        .iter()
        .map(|axis| to_var_len_unicode(axis))
        .collect::<Result<Vec<_>>>()?;
    let attr = group
        .new_attr::<VarLenUnicode>()
        .shape((values.len(),))
        .create("axes")?;
    attr.write(ArrayView1::from(values.as_slice()))?;
    Ok(())
}

fn set_axis_indices(group: &Group, name: &str, index: i32) -> Result<()> {
    let attr_name = format!("{name}_indices");
    group
        .new_attr::<i32>()
        .create(attr_name.as_str())?
        .write_scalar(&index)?;
    Ok(())
}

fn read_histogram_attrs(entry: &Group, group: &Group) -> Result<HistogramAttributes> {
    let mut attrs = HistogramAttributes {
        flight_path_m: read_attr_opt::<f64>(entry, "flight_path_m")?,
        tof_offset_ns: read_attr_opt::<f64>(entry, "tof_offset_ns")?,
        energy_axis_kind: read_attr_opt_string(entry, "energy_axis_kind")?,
    };

    if let Some(value) = read_attr_opt::<f64>(group, "flight_path_m")? {
        attrs.flight_path_m = Some(value);
    }
    if let Some(value) = read_attr_opt::<f64>(group, "tof_offset_ns")? {
        attrs.tof_offset_ns = Some(value);
    }
    if let Some(value) = read_attr_opt_string(group, "energy_axis_kind")? {
        attrs.energy_axis_kind = Some(value);
    }

    Ok(attrs)
}

fn derive_energy_axis_ev(tof_ns: &[f64], flight_path_m: f64, tof_offset_ns: f64) -> Vec<f64> {
    const NEUTRON_MASS_KG: f64 = 1.674_927_498_04e-27;
    const ELEMENTARY_CHARGE: f64 = 1.602_176_634e-19;

    tof_ns
        .iter()
        .map(|&tof| {
            let t_s = (tof + tof_offset_ns) * 1.0e-9;
            if t_s <= 0.0 {
                0.0
            } else {
                0.5 * NEUTRON_MASS_KG * (flight_path_m / t_s).powi(2) / ELEMENTARY_CHARGE
            }
        })
        .collect()
}

fn create_extendable_dataset<T: H5Type>(
    group: &Group,
    name: &str,
    chunk_events: usize,
    compression: Option<u8>,
    shuffle: bool,
) -> Result<Dataset> {
    let mut builder = group
        .new_dataset::<T>()
        .shape((0..,))
        .chunk((chunk_events,));

    if let Some(level) = compression {
        builder = builder.deflate(level);
    }

    if shuffle {
        builder = builder.shuffle();
    }

    Ok(builder.create(name)?)
}

fn append_slice<T: H5Type>(dataset: &Dataset, offset: usize, data: &[T]) -> Result<()> {
    if data.is_empty() {
        return Ok(());
    }
    let new_len = offset + data.len();
    dataset.resize((new_len,))?;
    let view = ArrayView1::from(data);
    dataset.write_slice(view, s![offset..new_len])?;
    Ok(())
}

fn set_dataset_units(dataset: &Dataset, units: &str) -> Result<()> {
    let value = to_var_len_unicode(units)?;
    dataset
        .new_attr::<VarLenUnicode>()
        .create("units")?
        .write_scalar(&value)?;
    Ok(())
}

fn set_attr_str_file(file: &File, name: &str, value: &str) -> Result<()> {
    let value = to_var_len_unicode(value)?;
    file.new_attr::<VarLenUnicode>()
        .create(name)?
        .write_scalar(&value)?;
    Ok(())
}

fn set_attr_str_group(group: &Group, name: &str, value: &str) -> Result<()> {
    let value = to_var_len_unicode(value)?;
    group
        .new_attr::<VarLenUnicode>()
        .create(name)?
        .write_scalar(&value)?;
    Ok(())
}

fn set_conversion_attrs(
    group: &Group,
    flight_path_m: Option<f64>,
    tof_offset_ns: Option<f64>,
    energy_axis_kind: Option<&str>,
) -> Result<()> {
    if let Some(value) = flight_path_m {
        group
            .new_attr::<f64>()
            .create("flight_path_m")?
            .write_scalar(&value)?;
    }
    if let Some(value) = tof_offset_ns {
        group
            .new_attr::<f64>()
            .create("tof_offset_ns")?
            .write_scalar(&value)?;
    }
    if let Some(kind) = energy_axis_kind {
        let value = to_var_len_unicode(kind)?;
        group
            .new_attr::<VarLenUnicode>()
            .create("energy_axis_kind")?
            .write_scalar(&value)?;
    }
    Ok(())
}

fn read_dataset_vec<T: H5Type>(group: &Group, name: &str) -> Result<Vec<T>> {
    let dataset = group.dataset(name)?;
    Ok(dataset.read_raw::<T>()?)
}

fn read_dataset_vec_opt<T: H5Type>(group: &Group, name: &str) -> Result<Option<Vec<T>>> {
    match group.dataset(name) {
        Ok(dataset) => Ok(Some(dataset.read_raw::<T>()?)),
        Err(_) => Ok(None),
    }
}

fn read_attr_opt<T: H5Type + Clone>(group: &Group, name: &str) -> Result<Option<T>> {
    match group.attr(name) {
        Ok(attr) => Ok(Some(attr.read_scalar::<T>()?)),
        Err(_) => Ok(None),
    }
}

fn read_attr_opt_string(group: &Group, name: &str) -> Result<Option<String>> {
    match group.attr(name) {
        Ok(attr) => {
            let value: VarLenUnicode = attr.read_scalar()?;
            Ok(Some(value.to_string()))
        }
        Err(_) => Ok(None),
    }
}

fn to_var_len_unicode(value: &str) -> Result<VarLenUnicode> {
    VarLenUnicode::from_str(value)
        .map_err(|e| Error::InvalidFormat(format!("invalid utf-8 attribute: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustpix_core::neutron::NeutronBatch;
    use rustpix_core::soa::HitBatch;
    use tempfile::NamedTempFile;

    #[test]
    fn test_hdf5_hit_roundtrip() {
        let mut batch = HitBatch::with_capacity(2);
        batch.x.extend_from_slice(&[1, 2]);
        batch.y.extend_from_slice(&[3, 4]);
        batch.tof.extend_from_slice(&[10, 20]);
        batch.tot.extend_from_slice(&[5, 6]);
        batch.timestamp.extend_from_slice(&[100, 200]);
        batch.chip_id.extend_from_slice(&[0, 1]);
        batch.cluster_id.extend_from_slice(&[-1, 2]);

        let event_batch = EventBatch {
            tdc_timestamp_25ns: 7,
            hits: batch,
        };

        let file = NamedTempFile::new().unwrap();
        let options = HitWriteOptions {
            x_size: 512,
            y_size: 512,
            chunk_events: 10,
            compression: None,
            shuffle: false,
            flight_path_m: None,
            tof_offset_ns: None,
            energy_axis_kind: Some("tof".to_string()),
            include_xy: true,
            include_tot: true,
            include_chip_id: true,
            include_cluster_id: true,
        };

        write_hits_hdf5(file.path(), vec![event_batch], &options).unwrap();
        let data = read_hits_hdf5(file.path()).unwrap();

        assert_eq!(data.event_id.len(), 2);
        assert_eq!(data.event_time_zero_ns, vec![7 * NS_PER_TICK]);
        assert_eq!(data.event_index, vec![0]);
        assert_eq!(data.x.as_ref().unwrap(), &vec![1, 2]);
        assert_eq!(data.y.as_ref().unwrap(), &vec![3, 4]);
        assert_eq!(
            data.time_over_threshold_ns.as_ref().unwrap(),
            &vec![125, 150]
        );
        assert_eq!(data.chip_id.as_ref().unwrap(), &vec![0, 1]);
        assert_eq!(data.cluster_id.as_ref().unwrap(), &vec![-1, 2]);
    }

    #[test]
    fn test_hdf5_neutron_roundtrip() {
        let mut neutrons = NeutronBatch::with_capacity(2);
        neutrons.x.extend_from_slice(&[10.2, 11.8]);
        neutrons.y.extend_from_slice(&[20.4, 21.6]);
        neutrons.tof.extend_from_slice(&[30, 40]);
        neutrons.tot.extend_from_slice(&[7, 9]);
        neutrons.n_hits.extend_from_slice(&[2, 3]);
        neutrons.chip_id.extend_from_slice(&[1, 2]);

        let event_batch = NeutronEventBatch {
            tdc_timestamp_25ns: 12,
            neutrons,
        };

        let file = NamedTempFile::new().unwrap();
        let options = NeutronWriteOptions {
            x_size: 512,
            y_size: 512,
            chunk_events: 10,
            compression: None,
            shuffle: false,
            flight_path_m: None,
            tof_offset_ns: None,
            energy_axis_kind: Some("tof".to_string()),
            include_xy: true,
            include_tot: true,
            include_chip_id: true,
            include_n_hits: true,
        };

        write_neutrons_hdf5(file.path(), vec![event_batch], &options).unwrap();
        let data = read_neutrons_hdf5(file.path()).unwrap();

        assert_eq!(data.event_id.len(), 2);
        assert_eq!(data.event_time_zero_ns, vec![12 * NS_PER_TICK]);
        assert_eq!(data.event_index, vec![0]);
        assert_eq!(data.x.as_ref().unwrap(), &vec![10, 12]);
        assert_eq!(data.y.as_ref().unwrap(), &vec![20, 22]);
        assert_eq!(
            data.time_over_threshold_ns.as_ref().unwrap(),
            &vec![175, 225]
        );
        assert_eq!(data.n_hits.as_ref().unwrap(), &vec![2, 3]);
        assert_eq!(data.chip_id.as_ref().unwrap(), &vec![1, 2]);
    }

    #[test]
    fn test_hdf5_hit_sink_multi_batch() {
        let mut first = HitBatch::with_capacity(2);
        first.x.extend_from_slice(&[1, 2]);
        first.y.extend_from_slice(&[3, 4]);
        first.tof.extend_from_slice(&[10, 20]);
        first.tot.extend_from_slice(&[5, 6]);
        first.timestamp.extend_from_slice(&[100, 200]);
        first.chip_id.extend_from_slice(&[0, 1]);
        first.cluster_id.extend_from_slice(&[-1, 2]);

        let mut second = HitBatch::with_capacity(1);
        second.x.extend_from_slice(&[5]);
        second.y.extend_from_slice(&[6]);
        second.tof.extend_from_slice(&[30]);
        second.tot.extend_from_slice(&[7]);
        second.timestamp.extend_from_slice(&[300]);
        second.chip_id.extend_from_slice(&[2]);
        second.cluster_id.extend_from_slice(&[3]);

        let batches = [
            EventBatch {
                tdc_timestamp_25ns: 7,
                hits: first,
            },
            EventBatch {
                tdc_timestamp_25ns: 9,
                hits: second,
            },
        ];

        let file = NamedTempFile::new().unwrap();
        let options = HitWriteOptions {
            x_size: 512,
            y_size: 512,
            chunk_events: 2,
            compression: None,
            shuffle: false,
            flight_path_m: None,
            tof_offset_ns: None,
            energy_axis_kind: Some("tof".to_string()),
            include_xy: true,
            include_tot: true,
            include_chip_id: true,
            include_cluster_id: true,
        };

        let mut sink = Hdf5HitSink::create(file.path(), options).unwrap();
        sink.write_hits(&batches[0]).unwrap();
        sink.write_hits(&batches[1]).unwrap();
        drop(sink);

        let data = read_hits_hdf5(file.path()).unwrap();
        assert_eq!(data.event_id.len(), 3);
        assert_eq!(
            data.event_time_zero_ns,
            vec![7 * NS_PER_TICK, 9 * NS_PER_TICK]
        );
        assert_eq!(data.event_index, vec![0, 2]);
    }

    #[test]
    fn test_hdf5_neutron_sink_multi_batch() {
        let mut first = NeutronBatch::with_capacity(1);
        first.x.extend_from_slice(&[10.2]);
        first.y.extend_from_slice(&[20.4]);
        first.tof.extend_from_slice(&[30]);
        first.tot.extend_from_slice(&[7]);
        first.n_hits.extend_from_slice(&[2]);
        first.chip_id.extend_from_slice(&[1]);

        let mut second = NeutronBatch::with_capacity(2);
        second.x.extend_from_slice(&[11.8, 12.4]);
        second.y.extend_from_slice(&[21.6, 22.0]);
        second.tof.extend_from_slice(&[40, 50]);
        second.tot.extend_from_slice(&[9, 11]);
        second.n_hits.extend_from_slice(&[3, 4]);
        second.chip_id.extend_from_slice(&[2, 3]);

        let batches = [
            NeutronEventBatch {
                tdc_timestamp_25ns: 12,
                neutrons: first,
            },
            NeutronEventBatch {
                tdc_timestamp_25ns: 13,
                neutrons: second,
            },
        ];

        let file = NamedTempFile::new().unwrap();
        let options = NeutronWriteOptions {
            x_size: 512,
            y_size: 512,
            chunk_events: 2,
            compression: None,
            shuffle: false,
            flight_path_m: None,
            tof_offset_ns: None,
            energy_axis_kind: Some("tof".to_string()),
            include_xy: true,
            include_tot: true,
            include_chip_id: true,
            include_n_hits: true,
        };

        let mut sink = Hdf5NeutronSink::create(file.path(), options).unwrap();
        sink.write_neutrons(&batches[0]).unwrap();
        sink.write_neutrons(&batches[1]).unwrap();
        drop(sink);

        let data = read_neutrons_hdf5(file.path()).unwrap();
        assert_eq!(data.event_id.len(), 3);
        assert_eq!(
            data.event_time_zero_ns,
            vec![12 * NS_PER_TICK, 13 * NS_PER_TICK]
        );
        assert_eq!(data.event_index, vec![0, 1]);
    }

    #[test]
    fn test_hdf5_optional_fields_omitted() {
        let mut batch = HitBatch::with_capacity(1);
        batch.x.extend_from_slice(&[1]);
        batch.y.extend_from_slice(&[2]);
        batch.tof.extend_from_slice(&[10]);
        batch.tot.extend_from_slice(&[5]);
        batch.timestamp.extend_from_slice(&[100]);
        batch.chip_id.extend_from_slice(&[0]);
        batch.cluster_id.extend_from_slice(&[-1]);

        let event_batch = EventBatch {
            tdc_timestamp_25ns: 3,
            hits: batch,
        };

        let file = NamedTempFile::new().unwrap();
        let options = HitWriteOptions {
            x_size: 512,
            y_size: 512,
            chunk_events: 10,
            compression: None,
            shuffle: false,
            flight_path_m: None,
            tof_offset_ns: None,
            energy_axis_kind: None,
            include_xy: false,
            include_tot: false,
            include_chip_id: false,
            include_cluster_id: false,
        };

        write_hits_hdf5(file.path(), vec![event_batch], &options).unwrap();
        let data = read_hits_hdf5(file.path()).unwrap();

        assert!(data.x.is_none());
        assert!(data.y.is_none());
        assert!(data.time_over_threshold_ns.is_none());
        assert!(data.chip_id.is_none());
        assert!(data.cluster_id.is_none());
        assert!(data.attrs.energy_axis_kind.is_none());
    }

    #[test]
    fn test_hdf5_hit_event_id_overflow() {
        let mut batch = HitBatch::with_capacity(1);
        batch.x.push(1);
        batch.y.push(1);
        batch.tof.push(10);
        batch.tot.push(5);
        batch.timestamp.push(100);
        batch.chip_id.push(0);
        batch.cluster_id.push(-1);

        let event_batch = EventBatch {
            tdc_timestamp_25ns: 1,
            hits: batch,
        };

        let file = NamedTempFile::new().unwrap();
        let options = HitWriteOptions {
            x_size: i32::MAX as u32,
            y_size: 2,
            chunk_events: 10,
            compression: None,
            shuffle: false,
            flight_path_m: None,
            tof_offset_ns: None,
            energy_axis_kind: None,
            include_xy: true,
            include_tot: false,
            include_chip_id: false,
            include_cluster_id: false,
        };

        let err = write_hits_hdf5(file.path(), vec![event_batch], &options).unwrap_err();
        assert!(matches!(err, Error::InvalidFormat(_)));
    }

    #[test]
    fn test_hdf5_event_index_overflow() {
        let file = NamedTempFile::new().unwrap();
        let file = File::create(file.path()).unwrap();
        let group = file.create_group("hits").unwrap();

        let options = HitWriteOptions {
            x_size: 10,
            y_size: 10,
            chunk_events: 10,
            compression: None,
            shuffle: false,
            flight_path_m: None,
            tof_offset_ns: None,
            energy_axis_kind: None,
            include_xy: false,
            include_tot: false,
            include_chip_id: false,
            include_cluster_id: false,
        };

        let mut writer = HitEventWriter::new(&group, &options).unwrap();
        writer.event_count = i32::MAX as usize + 1;

        let mut batch = HitBatch::with_capacity(1);
        batch.x.push(0);
        batch.y.push(0);
        batch.tof.push(1);
        batch.tot.push(1);
        batch.timestamp.push(1);
        batch.chip_id.push(0);
        batch.cluster_id.push(-1);

        let event_batch = EventBatch {
            tdc_timestamp_25ns: 1,
            hits: batch,
        };

        let err = writer.append_batch(&event_batch, &options).unwrap_err();
        assert!(matches!(err, Error::InvalidFormat(_)));
    }

    #[test]
    fn test_hdf5_neutron_negative_coords() {
        let mut neutrons = NeutronBatch::with_capacity(1);
        neutrons.x.push(-1.0);
        neutrons.y.push(2.0);
        neutrons.tof.push(10);
        neutrons.tot.push(1);
        neutrons.n_hits.push(1);
        neutrons.chip_id.push(0);

        let event_batch = NeutronEventBatch {
            tdc_timestamp_25ns: 1,
            neutrons,
        };

        let file = NamedTempFile::new().unwrap();
        let options = NeutronWriteOptions {
            x_size: 10,
            y_size: 10,
            chunk_events: 10,
            compression: None,
            shuffle: false,
            flight_path_m: None,
            tof_offset_ns: None,
            energy_axis_kind: None,
            include_xy: true,
            include_tot: false,
            include_chip_id: false,
            include_n_hits: false,
        };

        let err = write_neutrons_hdf5(file.path(), vec![event_batch], &options).unwrap_err();
        assert!(matches!(err, Error::InvalidFormat(_)));
    }

    #[test]
    fn test_hdf5_neutron_out_of_bounds_coords() {
        let mut neutrons = NeutronBatch::with_capacity(1);
        neutrons.x.push(10.0);
        neutrons.y.push(0.0);
        neutrons.tof.push(10);
        neutrons.tot.push(1);
        neutrons.n_hits.push(1);
        neutrons.chip_id.push(0);

        let event_batch = NeutronEventBatch {
            tdc_timestamp_25ns: 1,
            neutrons,
        };

        let file = NamedTempFile::new().unwrap();
        let options = NeutronWriteOptions {
            x_size: 10,
            y_size: 10,
            chunk_events: 10,
            compression: None,
            shuffle: false,
            flight_path_m: None,
            tof_offset_ns: None,
            energy_axis_kind: None,
            include_xy: true,
            include_tot: false,
            include_chip_id: false,
            include_n_hits: false,
        };

        let err = write_neutrons_hdf5(file.path(), vec![event_batch], &options).unwrap_err();
        assert!(matches!(err, Error::InvalidFormat(_)));
    }

    #[test]
    fn test_hdf5_histogram_counts_shape_mismatch() {
        let data = HistogramWriteData {
            counts: vec![1, 2, 3],
            shape: HistogramShape {
                rot_angle: 1,
                y: 1,
                x: 1,
                time_of_flight: 2,
            },
            rot_angle: vec![0.0],
            y: vec![0.0],
            x: vec![0.0],
            time_of_flight_ns: vec![10.0, 20.0],
        };

        let file = NamedTempFile::new().unwrap();
        let options = HistogramWriteOptions {
            compression: None,
            shuffle: false,
            ..HistogramWriteOptions::default()
        };

        let err = write_histogram_hdf5(file.path(), &data, &options).unwrap_err();
        assert!(matches!(err, Error::InvalidFormat(_)));
    }

    #[test]
    fn test_hdf5_histogram_axis_len_mismatch() {
        let data = HistogramWriteData {
            counts: vec![1, 2],
            shape: HistogramShape {
                rot_angle: 1,
                y: 2,
                x: 1,
                time_of_flight: 1,
            },
            rot_angle: vec![0.0],
            y: vec![0.0],
            x: vec![0.0],
            time_of_flight_ns: vec![10.0],
        };

        let file = NamedTempFile::new().unwrap();
        let options = HistogramWriteOptions {
            compression: None,
            shuffle: false,
            ..HistogramWriteOptions::default()
        };

        let err = write_histogram_hdf5(file.path(), &data, &options).unwrap_err();
        assert!(matches!(err, Error::InvalidFormat(_)));
    }

    #[test]
    fn test_hdf5_histogram_roundtrip() {
        let data = HistogramWriteData {
            counts: vec![1, 2, 3, 4, 5, 6],
            shape: HistogramShape {
                rot_angle: 1,
                y: 1,
                x: 2,
                time_of_flight: 3,
            },
            rot_angle: vec![0.0],
            y: vec![0.0],
            x: vec![0.0, 1.0],
            time_of_flight_ns: vec![10.0, 20.0, 30.0],
        };

        let file = NamedTempFile::new().unwrap();
        let options = HistogramWriteOptions {
            compression: None,
            shuffle: false,
            ..HistogramWriteOptions::default()
        };

        write_histogram_hdf5(file.path(), &data, &options).unwrap();
        let loaded = read_histogram_hdf5(file.path()).unwrap();

        assert_eq!(loaded.shape, data.shape);
        assert_eq!(loaded.counts, data.counts);
        assert_eq!(loaded.rot_angle, data.rot_angle);
        assert_eq!(loaded.y, data.y);
        assert_eq!(loaded.x, data.x);
        assert_eq!(loaded.time_of_flight_ns, data.time_of_flight_ns);
        assert!(loaded.energy_ev.is_none());
    }

    #[test]
    fn test_hdf5_histogram_energy_axis_present() {
        let data = HistogramWriteData {
            counts: vec![1, 2, 3, 4],
            shape: HistogramShape {
                rot_angle: 1,
                y: 1,
                x: 1,
                time_of_flight: 4,
            },
            rot_angle: vec![0.0],
            y: vec![0.0],
            x: vec![0.0],
            time_of_flight_ns: vec![10.0, 20.0, 30.0, 40.0],
        };

        let file = NamedTempFile::new().unwrap();
        let options = HistogramWriteOptions {
            flight_path_m: Some(4.0),
            tof_offset_ns: Some(100.0),
            compression: None,
            shuffle: false,
            ..HistogramWriteOptions::default()
        };

        write_histogram_hdf5(file.path(), &data, &options).unwrap();
        let loaded = read_histogram_hdf5(file.path()).unwrap();

        let energy = loaded.energy_ev.expect("energy axis missing");
        assert_eq!(energy.len(), data.time_of_flight_ns.len());
    }
}
