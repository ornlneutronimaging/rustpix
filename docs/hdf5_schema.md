# rustpix HDF5 / NeXus Schema (scipp-compatible)

This document defines the on-disk HDF5 layout for rustpix event data and
histograms. The goal is **scipp compatibility** via **NeXus/NXdata** while
keeping the layout simple and stable for large files.

This is a **format contract** for issues #47 and #48.

## Goals

- Bounded-memory processing for large TPX3 datasets.
- scipp-compatible layout via NeXus (NXdata).
- Clear units and metadata to support TOF <-> eV conversion.
- Optional fields (tot, chip_id, cluster_id) are truly optional.

## File-level conventions

- Root group has attribute: `rustpix_format_version = "0.1"`.
- All groups use `NX_class` attributes where applicable.
- Units are stored as dataset attributes: `units = "ns"`, `"pixel"`, `"deg"`, etc.
- Endianness is native (HDF5 handles portability).
- If a dataset is optional and not present, readers must return `None`.

## Top-level structure

```
/
  rustpix_format_version = "0.1"
  entry/                     (NXentry)
    hits/                    (NXdata)     [optional]
    neutrons/                (NXdata)     [optional]
    histogram/               (NXdata)     [optional]
    metadata/                (group)      [optional]
```

## Event data (hits / neutrons)

Event groups store **columnar** datasets, all sharing the same 1-D `event`
dimension. Each group is an `NXdata` group.

Group attributes:

- `NX_class = "NXdata"`
- `signal = "tof"` (primary signal for the event table)
- `axes = ["event"]`

Optional `event` coordinate dataset:

- `event` (int64), length = number of events
- If omitted, readers may assume `event = 0..N-1`.

### Required datasets (ns units)

| Name       | Type  | Shape    | Units | Notes |
|------------|-------|----------|-------|-------|
| `x`        | u16   | (N)      | pixel | global pixel X |
| `y`        | u16   | (N)      | pixel | global pixel Y |
| `tof`      | u64   | (N)      | ns    | time-of-flight in nanoseconds |
| `timestamp`| u64   | (N)      | ns    | global timestamp in nanoseconds |

### Optional datasets

| Name       | Type  | Shape | Units | Notes |
|------------|-------|-------|-------|-------|
| `tot`      | u16   | (N)   | tick  | time-over-threshold |
| `chip_id`  | u8    | (N)   | id    | chip identifier |
| `cluster_id` | i32 | (N)   | id    | cluster assignment |

### Conversion metadata (group attributes)

Used for optional TOF -> eV conversion and for schema self-description:

- `flight_path_m` (f64) - required to compute energy
- `tof_offset_ns` (f64) - required to compute energy
- `energy_axis_kind` (string) - typically `"tof"`

If `flight_path_m` or `tof_offset_ns` is missing, readers must not attempt
energy conversion.

## Histogram / hyperspectra

Histogram data is stored in a single `NXdata` group named `histogram`.

Group attributes:

- `NX_class = "NXdata"`
- `signal = "counts"`
- `axes = ["rot_angle", "energy_axis", "y", "x"]`
- `<axis>_indices` attributes must be present with integer indices
  (`rot_angle_indices=0`, `energy_axis_indices=1`, etc.)

### Required datasets

| Name          | Type | Shape | Units | Notes |
|---------------|------|-------|-------|-------|
| `counts`      | u64  | (R, E, Y, X) | count | histogram counts |
| `rot_angle`   | f64  | (R)   | deg   | rotation angle |
| `energy_axis` | f64  | (E)   | ns    | TOF axis in nanoseconds |
| `y`           | f64  | (Y)   | pixel | Y axis (center values) |
| `x`           | f64  | (X)   | pixel | X axis (center values) |

### Optional derived energy axis

If **both** `flight_path_m` and `tof_offset_ns` are provided, store a derived
energy axis:

| Name            | Type | Shape | Units | Notes |
|-----------------|------|-------|-------|-------|
| `energy_axis_eV`| f64  | (E)   | eV    | energy axis derived from TOF |

If either metadata field is missing, `energy_axis_eV` must be absent.

### Conversion metadata (group attributes)

Same as events:

- `flight_path_m` (f64)
- `tof_offset_ns` (f64)
- `energy_axis_kind` (string) - `"tof"` by default

## Metadata group (optional)

`/entry/metadata` may contain JSON or structured datasets for:

- detector config
- clustering config
- extraction config
- provenance (git sha, rustpix version)

The exact schema is optional and may evolve; keep it additive.

## Round-trip expectations

- **Events:** read/write preserves all datasets and units. Optional datasets
  must round-trip if present and return `None` if absent.
- **Histograms:** read/write preserves counts and axes. `energy_axis_eV` must
  round-trip when present and be omitted otherwise.
- **Units:** units attributes must be preserved.

## Notes for implementers

- For large datasets, use chunked writes and avoid full in-memory copies.
- Use `u64` for `tof` and `timestamp` in ns to prevent overflow when converting
  from 25 ns ticks.
