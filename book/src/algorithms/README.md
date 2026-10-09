# Clustering Algorithms

Rustpix provides three clustering algorithms for grouping detector hits into neutron events. Each algorithm has different performance characteristics and is suited for different use cases.

## Overview

| Algorithm | Complexity | Best For | Parallelism |
|-----------|------------|----------|-------------|
| **ABS** | O(n) average | General use, balanced performance | Single-threaded |
| **DBSCAN** | O(n log n) | Noisy data, irregular clusters | Single-threaded |
| **Grid** | O(n) | Large datasets, parallel processing | Multi-threaded |

## Input Requirements

Each clustering call works on the hits of one pulse. TOF restarts at every
pulse, so if a batch holds several pulses, hits from different pulses with
similar position and TOF are grouped together. `stream_tpx3_neutrons` and
`process_tpx3_neutrons` (with the default `time_ordered=True`) cluster pulse by
pulse.

ABS and Grid also require the hits in ascending TOF order; on unsorted input,
hits that belong together can end up in separate clusters. DBSCAN does not
depend on order.

## How `radius` and `temporal_window_ns` Are Applied

The same `ClusteringConfig` gives different clusters depending on the algorithm:

| Algorithm | `radius` | `temporal_window_ns` | Longest cluster duration |
|-----------|----------|----------------------|--------------------------|
| ABS | Hit vs. the cluster's bounding box, expanded by `radius` | Hit vs. the cluster's first hit | `temporal_window_ns` |
| DBSCAN | Hit vs. hit, Euclidean | Hit vs. hit | Unbounded (neighbours chain) |
| Grid | Hit vs. hit, Euclidean | Hit vs. hit | Unbounded (links chain) |

For example, six hits in adjacent pixels, each 50 ns after the previous one,
form one cluster with Grid or DBSCAN and three clusters with ABS (75 ns window).

## ABS (Age-Based Spatial)

The default algorithm. Builds clusters in a single pass over TOF-ordered hits.

### How It Works

1. Hits are processed in TOF order
2. A hit joins an open cluster if it lies within `radius` of the cluster's bounding box and within `temporal_window_ns` of the cluster's first hit
3. Otherwise it starts a new cluster
4. Every `abs_scan_interval` hits, clusters whose first hit is older than the temporal window are closed

### Parameters

| Parameter | Description | Typical Value |
|-----------|-------------|---------------|
| `radius` | Maximum distance from the cluster's bounding box (pixels) | 5.0 |
| `temporal_window_ns` | Maximum time after the cluster's first hit | 75.0 ns |
| `abs_scan_interval` | Hits between cluster scans | 100 |

### When to Use

- General-purpose neutron imaging
- Files with moderate noise levels
- When processing speed is important

```python
neutrons = rustpix.process_tpx3_neutrons(
    "data.tpx3",
    algorithm="abs",
    abs_scan_interval=1000,
    collect=True
)
```

## DBSCAN

Density-Based Spatial Clustering of Applications with Noise. Groups points based on density reachability.

### How It Works

1. Build spatial index of all hits
2. For each unvisited hit, find neighbors within epsilon
3. If the neighborhood holds at least `min_points` hits (counting the hit itself), start a cluster
4. Recursively expand cluster with density-reachable points
5. Points not in any cluster are marked as noise

### Parameters

| Parameter | Description | Typical Value |
|-----------|-------------|---------------|
| `radius` | Epsilon: maximum distance between neighbouring hits (pixels) | 5.0 |
| `temporal_window_ns` | Maximum time difference between neighbouring hits | 75.0 ns |
| `dbscan_min_points` | Minimum hits within the neighborhood, including the hit itself, for a core point (1 keeps isolated hits) | 2 |

### When to Use

- High noise environments
- When cluster shape is irregular
- When you need to identify noise points

```python
neutrons = rustpix.process_tpx3_neutrons(
    "data.tpx3",
    algorithm="dbscan",
    dbscan_min_points=2,
    collect=True
)
```

## Grid

Parallel grid-based clustering with spatial indexing.

### How It Works

1. Divide detector space into cells
2. Assign hits to cells based on position
3. Process cells in parallel using rayon
4. Merge clusters that span cell boundaries
5. Use union-find for efficient cluster merging

### Parameters

| Parameter | Description | Typical Value |
|-----------|-------------|---------------|
| `radius` | Maximum distance between two linked hits (pixels) | 5.0 |
| `temporal_window_ns` | Maximum time difference between two linked hits | 75.0 ns |
| `grid_cell_size` | Cell size in pixels | 32 |

### When to Use

- Very large datasets
- Multi-core systems
- When throughput is critical

```python
neutrons = rustpix.process_tpx3_neutrons(
    "data.tpx3",
    algorithm="grid",
    grid_cell_size=32,
    collect=True
)
```

## Performance Comparison

Benchmark results on a typical neutron imaging dataset (5M hits):

| Algorithm | Time (ms) | Memory | Notes |
|-----------|-----------|--------|-------|
| ABS | ~250 | Low | Consistent, predictable |
| DBSCAN | ~1200 | Medium | Slower but noise-robust |
| Grid | ~300 | Medium | Scales with cores |

## Choosing an Algorithm

```
Start with ABS (default)
    │
    ├─ Too much noise? → Try DBSCAN
    │
    ├─ Need more speed? → Try Grid
    │   └─ (especially on multi-core systems)
    │
    └─ Results look good? → Stick with ABS
```

## Parameter Tuning

### Spatial Radius

- **Too small**: Clusters split into multiple events
- **Too large**: Separate events merged together
- **Start with**: 5.0 pixels, adjust based on results

### Temporal Window

- **Too small**: Events spanning multiple TDC cycles split
- **Too large**: Unrelated events merged
- **Start with**: 75.0 ns (matches typical TPX3 timing)

### Min Cluster Size

- **1**: Accept all clusters (including noise)
- **2+**: Filter single-hit noise events
- **Typical**: 1-3 depending on noise level
