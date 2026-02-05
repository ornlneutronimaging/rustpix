# Python API

The `rustpix` Python package provides thin wrappers around the high-performance Rust core. Data is returned as NumPy arrays or PyArrow Tables for seamless integration with the scientific Python ecosystem.

## Overview

| Function | Description |
|----------|-------------|
| [`read_tpx3_hits`](quickstart.md#reading-hits) | Read all hits from a TPX3 file |
| [`stream_tpx3_hits`](quickstart.md#streaming-hits) | Stream hits in batches |
| [`process_tpx3_neutrons`](quickstart.md#processing-neutrons) | Process hits into neutron events |
| [`stream_tpx3_neutrons`](quickstart.md#streaming-neutrons) | Stream neutron events in batches |
| [`cluster_hits`](quickstart.md#clustering-hits) | Cluster an existing HitBatch |

## Data Types

### HitBatch

Contains raw detector hits with the following fields:

| Field | Type | Description |
|-------|------|-------------|
| `x` | `uint16` | X coordinate (pixels) |
| `y` | `uint16` | Y coordinate (pixels) |
| `tof` | `uint32` | Time-of-flight (25ns ticks) |
| `tot` | `uint16` | Time-over-threshold (charge proxy) |
| `timestamp` | `uint32` | Raw timestamp |
| `chip_id` | `uint8` | Detector chip ID |
| `cluster_id` | `int32` | Cluster assignment (-1 if unclustered) |

> **Note:** The `tof` field is stored in 25ns tick units for efficiency. To convert to nanoseconds: `tof_ns = tof * 25`

### NeutronBatch

Contains processed neutron events:

| Field | Type | Description |
|-------|------|-------------|
| `x` | `float64` | Centroid X (sub-pixel resolution) |
| `y` | `float64` | Centroid Y (sub-pixel resolution) |
| `tof` | `uint32` | Time-of-flight (25ns ticks) |
| `tot` | `uint16` | Total charge (sum of hit ToT) |
| `n_hits` | `uint16` | Number of hits in cluster |
| `chip_id` | `uint8` | Detector chip ID |

> **Note:** The `tof` field is stored in 25ns tick units. To convert to nanoseconds: `tof_ns = tof * 25`

## Output Formats

Both `HitBatch` and `NeutronBatch` support:

```python
# Convert to NumPy dict of arrays
data = batch.to_numpy()

# Convert to PyArrow Table (requires pyarrow)
table = batch.to_arrow()
```

## Algorithms

Three clustering algorithms are available:

| Algorithm | Description | Best For |
|-----------|-------------|----------|
| `abs` | Adjacency-Based Search (8-connectivity) | General use, balanced |
| `dbscan` | Density-based spatial clustering | Noisy data |
| `grid` | Parallel grid-based clustering | Large datasets |

Specify with the `algorithm` keyword argument:

```python
neutrons = rustpix.process_tpx3_neutrons(
    "data.tpx3",
    algorithm="abs"  # or "dbscan", "grid"
)
```

## Next Steps

- [Quick Start](quickstart.md) - Basic usage examples
- [Configuration](configuration.md) - Detailed configuration options
