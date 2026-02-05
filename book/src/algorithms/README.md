# Clustering Algorithms

Rustpix provides three clustering algorithms for grouping detector hits into neutron events. Each algorithm has different performance characteristics and is suited for different use cases.

## Overview

| Algorithm | Complexity | Best For | Parallelism |
|-----------|------------|----------|-------------|
| **ABS** | O(n) average | General use, balanced performance | Single-threaded |
| **DBSCAN** | O(n log n) | Noisy data, irregular clusters | Single-threaded |
| **Grid** | O(n) | Large datasets, parallel processing | Multi-threaded |

## ABS (Adjacency-Based Search)

The default algorithm. Uses 8-connectivity search to find adjacent pixels within temporal and spatial thresholds.

### How It Works

1. Hits are processed in time order
2. For each hit, search for neighbors within radius and temporal window
3. Group connected hits into clusters using flood-fill
4. Periodically scan for completed clusters (configurable interval)

### Parameters

| Parameter | Description | Typical Value |
|-----------|-------------|---------------|
| `radius` | Maximum pixel distance | 5.0 |
| `temporal_window_ns` | Maximum time difference | 75.0 ns |
| `abs_scan_interval` | Hits between cluster scans | 1000 |

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
3. If enough neighbors (min_points), start a cluster
4. Recursively expand cluster with density-reachable points
5. Points not in any cluster are marked as noise

### Parameters

| Parameter | Description | Typical Value |
|-----------|-------------|---------------|
| `radius` | Epsilon (spatial search radius) | 5.0 |
| `temporal_window_ns` | Temporal epsilon | 75.0 ns |
| `dbscan_min_points` | Minimum neighbors for core point | 2 |

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
| `radius` | Maximum pixel distance | 5.0 |
| `temporal_window_ns` | Maximum time difference | 75.0 ns |
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
