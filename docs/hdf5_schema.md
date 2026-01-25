# rustpix HDF5 / NeXus Schema (scipp-compatible)

This document defines the on-disk HDF5 layout for rustpix event data and
histograms. The goal is **scipp compatibility** via **NeXus**, using
**NXevent_data** for events and **NXdata** for histograms, while keeping the
layout simple and stable for large files. NeXus recommends NXevent_data for
event-mode neutron data, and SNS/ISIS event files use that base class.

This is a **format contract** for issues #47 and #48.

## Goals

- Bounded-memory processing for large TPX3 datasets.
- scipp-compatible layout via NeXus (NXevent_data + NXdata).
- Clear units and metadata to support TOF <-> eV conversion.
- Optional fields (tot, chip_id, cluster_id) are truly optional.

## File-level conventions

- Root group has attribute: `rustpix_format_version = "0.1"`.
- All groups use `NX_class` attributes where applicable.
- Units are stored as dataset attributes: `units = "ns"`, `"pixel"`, `"deg"`, etc.
- Endianness is native (HDF5 handles portability).
- If a dataset is optional and not present, readers must return `None`.
- Versioning policy: `rustpix_format_version` follows semantic versioning. Major
  changes may be breaking, minor versions are additive. NeXus base classes carry
  their own versions and should be validated by readers.

## Top-level structure

```
/
  rustpix_format_version = "0.1"
  entry/                     (NXentry)
    hits/                    (NXevent_data) [optional]
    neutrons/                (NXevent_data) [optional]
    histogram/               (NXdata)       [optional]
    metadata/                (group)        [optional]
```

## Event data (hits / neutrons) — NXevent_data

Event groups follow the NeXus **NXevent_data** base class, which is used by
SNS/ISIS event files for pulsed-source event data.

Group attributes:

- `NX_class = "NXevent_data"`

### Required datasets (ns units)

| Name               | Type | Shape | Units | Notes |
|--------------------|------|-------|-------|-------|
| `event_id`         | i32  | (N)   | id    | detector element id (see mapping below) |
| `event_time_offset`| u64  | (N)   | ns    | time-of-flight relative to pulse start |

### Pulse indexing (required for pulsed sources)

| Name             | Type | Shape | Units | Notes |
|------------------|------|-------|-------|-------|
| `event_time_zero`| u64  | (J)   | ns    | start time of each pulse |
| `event_index`    | i32  | (J)   | id    | index into event arrays for each pulse |

These fields are part of the NXevent_data definition and are standard for pulsed
sources.

### Optional datasets

| Name                  | Type | Shape | Units | Notes |
|-----------------------|------|-------|-------|-------|
| `time_over_threshold` | u64  | (N)   | ns    | time-over-threshold in nanoseconds |
| `chip_id`             | u8   | (N)   | id    | chip identifier |
| `cluster_id`          | i32  | (N)   | id    | cluster assignment |
| `x`                   | u16  | (N)   | pixel | global pixel X (auxiliary) |
| `y`                   | u16  | (N)   | pixel | global pixel Y (auxiliary) |

Cluster assignment convention:

- `cluster_id >= 0` : valid cluster index
- `cluster_id = -1` : unclustered / noise

### event_id mapping

For imaging data, `event_id` SHOULD be present for NeXus compatibility. The
mapping from `event_id` to pixel coordinates should be documented. Recommended
simple mapping:

```
event_id = y * x_size + x
```

with group attributes `x_size` and `y_size`. The auxiliary `x` and `y` datasets
may also be provided for direct access and visualization.

### Conversion metadata (attributes)

Used for optional TOF -> eV conversion and for schema self-description:

- `flight_path_m` (f64) - effective flight path length
- `tof_offset_ns` (f64) - instrument TOF window shift
- `energy_axis_kind` (string) - typically `"tof"`

If `flight_path_m` or `tof_offset_ns` is missing, readers must not attempt energy
conversion.

**Canonical location:** store these attributes at `/entry` and optionally
duplicate them on `hits`, `neutrons`, and `histogram`. If both entry-level and
group-level values are present, the group-level values take precedence; readers
may warn on mismatches.

## Histogram / hyperspectra — NXdata

Histogram data is stored in a single `NXdata` group named `histogram`.

Group attributes:

- `NX_class = "NXdata"`
- `signal = "counts"`
- `axes = ["rot_angle", "y", "x", "time_of_flight"]`
- `<axis>_indices` attributes must be present with integer indices
  (`rot_angle_indices=0`, `y_indices=1`, `x_indices=2`,
   `time_of_flight_indices=3`, etc.)

NeXus specifies that `AXISNAME_indices` attributes are integer arrays (scalar
integers are valid for 1-D axes) and that axis lengths must match the data
dimensions (or be length+1 for histogram edges).

Axis order rationale:

- Imaging data is naturally a stack of 2-D frames `(rot_angle, y, x)`.
- `time_of_flight` is added as the energy-resolving axis.
- Consumers **must** use `axes`/`*_indices` and not assume a fixed order.

### Required datasets

| Name             | Type | Shape        | Units | Notes |
|------------------|------|--------------|-------|-------|
| `counts`         | u64  | (R, Y, X, E) | count | histogram counts |
| `rot_angle`      | f64  | (R)          | deg   | rotation angle |
| `y`              | f64  | (Y)          | pixel | Y axis (centers or edges) |
| `x`              | f64  | (X)          | pixel | X axis (centers or edges) |
| `time_of_flight` | f64  | (E)          | ns    | TOF axis (centers or edges) |

Axis representation:

- **Centers**: axis length = N
- **Edges**: axis length = N+1 (bin edges)

The axes attribute defines the mapping, so consumers should not assume a
particular axis order beyond `axes`/`_indices`.

### Optional derived energy axis

If **both** `flight_path_m` and `tof_offset_ns` are provided, store a derived
energy axis:

| Name        | Type | Shape | Units | Notes |
|-------------|------|-------|-------|-------|
| `energy_eV` | f64  | (E)   | eV    | energy axis derived from TOF |

If either metadata field is missing, `energy_eV` must be absent.

Relationship between TOF and energy:

Mantid defines energy conversion from TOF using the non-relativistic relation
`E = (m_n / 2) * (L / t)^2`, where `t` is the time-of-flight and `L` is the
total flight path length. Here, `L` should be taken from `flight_path_m`.

For rustpix, use:

```
TOF_seconds = (event_time_offset_ns + tof_offset_ns) * 1e-9
```

and apply the standard relation above. `energy_eV` is derived data; readers may
use it directly but should treat it as consistent with `time_of_flight` and the
conversion metadata.

## Metadata group (optional)

`/entry/metadata` may contain JSON or structured datasets for:

- detector config (chip transforms, pixel size, etc.)
- clustering config
- extraction config
- processing provenance (git sha, rustpix version)
- instrument context (facility/instrument name, run id)

Preferred JSON storage: a UTF-8 string dataset named `metadata_json`. If structured
datasets are used instead, they must be additive and documented.

## Round-trip expectations

- **Events:** read/write preserves all datasets and units. Optional datasets
  must round-trip if present and return `None` if absent.
- **Histograms:** read/write preserves counts and axes. `energy_eV` must
  round-trip when present and be omitted otherwise.
- **Units:** units attributes must be preserved.

## Notes for implementers

- For large datasets, use chunked writes and avoid full in-memory copies.
- Recommended starting point for events: chunk along the `event` dimension with
  50k–200k events per chunk (target ~1–8 MiB per chunk depending on columns).
- For histograms, chunk along slowest-changing dimensions (e.g., `rot_angle`) and
  keep `x`, `y`, and `time_of_flight` contiguous for fast slicing.
- Compression: start with gzip/deflate level 1–4 for balanced I/O; adjust based
  on benchmarks. Consider shuffle + compression for integer datasets.
- Use `u64` for `event_time_offset` and `event_time_zero` in ns to prevent
  overflow when converting from 25 ns ticks.

## References

- NeXus: Storing Event Data (SNS/ISIS event files and NXevent_data usage)
  https://www.nexusformat.org/Storing_Event_Data.html
- NeXus: NXevent_data base class
  https://manual.nexusformat.org/classes/base_classes/NXevent_data.html
- NeXus: NXdata base class (axes and *_indices rules)
  https://manual.nexusformat.org/classes/base_classes/NXdata.html
- Mantid: TOF <-> Energy conversion (UnitFactory)
  https://docs.mantidproject.org/nightly/concepts/UnitFactory.html
