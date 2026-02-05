# HDF5 Schema

This document defines the on-disk HDF5 layout for rustpix event data and histograms. The schema is designed for **scipp compatibility** via **NeXus**, using **NXevent_data** for events and **NXdata** for histograms.

## Goals

- Bounded-memory processing for large TPX3 datasets
- scipp-compatible layout via NeXus (NXevent_data + NXdata)
- Clear units and metadata to support TOF ↔ eV conversion
- Optional fields (tot, chip_id, cluster_id) are truly optional

## File Structure

```
/
  rustpix_format_version = "0.1"
  entry/                     (NXentry)
    hits/                    (NXevent_data) [optional]
    neutrons/                (NXevent_data) [optional]
    histogram/               (NXdata)       [optional]
    metadata/                (group)        [optional]
```

### File-Level Conventions

- Root group has attribute: `rustpix_format_version = "0.1"`
- All groups use `NX_class` attributes where applicable
- Units are stored as dataset attributes: `units = "ns"`, `"pixel"`, `"deg"`, etc.
- Endianness is native (HDF5 handles portability)

## Event Data (NXevent_data)

Event groups follow the NeXus **NXevent_data** base class, used by SNS/ISIS event files and expected by Mantid.

### Required Datasets

| Name | Type | Shape | Units | Description |
|------|------|-------|-------|-------------|
| `event_id` | i32 | (N) | id | Detector element ID |
| `event_time_offset` | u64 | (N) | ns | Time-of-flight relative to pulse |

### Pulse Indexing (for pulsed sources)

| Name | Type | Shape | Units | Description |
|------|------|-------|-------|-------------|
| `event_time_zero` | u64 | (J) | ns | Start time of each pulse |
| `event_index` | i32 | (J) | id | Index into event arrays |

### Optional Datasets

| Name | Type | Shape | Units | Description |
|------|------|-------|-------|-------------|
| `time_over_threshold` | u64 | (N) | ns | ToT in nanoseconds |
| `chip_id` | u8 | (N) | id | Chip identifier |
| `cluster_id` | i32 | (N) | id | Cluster assignment |
| `n_hits` | u16 | (N) | count | Hits per neutron |
| `x` | u16 | (N) | pixel | Global pixel X |
| `y` | u16 | (N) | pixel | Global pixel Y |

### Cluster ID Convention

- `cluster_id >= 0`: Valid cluster index
- `cluster_id = -1`: Unclustered / noise

### Event ID Mapping

For imaging data, `event_id` maps to pixel coordinates:

```
event_id = y * x_size + x
```

Group attributes `x_size` and `y_size` define the detector dimensions.

## Histogram Data (NXdata)

Histogram data is stored in a single `NXdata` group named `histogram`.

### Group Attributes

```
NX_class = "NXdata"
signal = "counts"
axes = ["rot_angle", "y", "x", "time_of_flight"]
rot_angle_indices = 0
y_indices = 1
x_indices = 2
time_of_flight_indices = 3
```

### Required Datasets

| Name | Type | Shape | Units | Description |
|------|------|-------|-------|-------------|
| `counts` | u64 | (R, Y, X, E) | count | Histogram counts |
| `rot_angle` | f64 | (R) | deg | Rotation angle |
| `y` | f64 | (Y) | pixel | Y axis |
| `x` | f64 | (X) | pixel | X axis |
| `time_of_flight` | f64 | (E) | ns | TOF axis |

### Axis Representation

- **Centers**: axis length = N, `axis_mode = "centers"`
- **Edges**: axis length = N+1, `axis_mode = "edges"`

### Optional Energy Axis

If `flight_path_m` and `tof_offset_ns` are provided:

| Name | Type | Shape | Units | Description |
|------|------|-------|-------|-------------|
| `energy_eV` | f64 | (E) | eV | Derived energy axis |

## Conversion Metadata

Stored as attributes at `/entry`:

| Attribute | Type | Description |
|-----------|------|-------------|
| `flight_path_m` | f64 | Effective flight path length |
| `tof_offset_ns` | f64 | Instrument TOF window shift |
| `energy_axis_kind` | string | Typically `"tof"` |

### TOF to Energy Conversion

Using the non-relativistic relation:

```
E = (m_n / 2) * (L / t)²

where:
  t = (event_time_offset + tof_offset_ns) * 1e-9  [seconds]
  L = flight_path_m  [meters]
  m_n = neutron mass
```

## Metadata Group

`/entry/metadata` may contain:

- Detector config (chip transforms, pixel size)
- Clustering config
- Extraction config
- Processing provenance (git sha, rustpix version)
- Instrument context (facility, run ID)

Preferred storage: UTF-8 string dataset named `metadata_json`.

## Implementation Notes

### Chunking Strategy

- **Events**: Chunk along event dimension, 50k–200k events per chunk
- **Histograms**: Chunk along slowest-changing dimensions (e.g., `rot_angle`)

### Compression

- Start with gzip level 1–4 for balanced I/O
- Use shuffle + compression for integer datasets

### Data Types

- Use `u64` for timestamps in ns to prevent overflow
- Use `f64` for coordinates requiring sub-pixel precision
