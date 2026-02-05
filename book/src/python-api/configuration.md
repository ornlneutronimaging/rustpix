# Configuration

## DetectorConfig

Configure detector-specific parameters:

```python
import rustpix

config = rustpix.DetectorConfig(
    tdc_frequency_hz=60.0,            # TDC frequency (Hz)
    enable_missing_tdc_correction=True,
    chip_size_x=256,                  # Chip width in pixels
    chip_size_y=256,                  # Chip height in pixels
    chip_transforms=None              # Custom chip transformations
)
```

### Parameters

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `tdc_frequency_hz` | `float` | `60.0` | TDC frequency in Hz |
| `enable_missing_tdc_correction` | `bool` | `True` | Correct for missing TDC packets |
| `chip_size_x` | `int` | `256` | Chip width in pixels |
| `chip_size_y` | `int` | `256` | Chip height in pixels |
| `chip_transforms` | `list` | `None` | Chip coordinate transformations |

### Chip Transforms

Chip transforms are 2x2 affine matrices plus translation:

```python
# Transform tuple: (a, b, c, d, tx, ty)
# x' = a*x + b*y + tx
# y' = c*x + d*y + ty

config = rustpix.DetectorConfig(
    chip_transforms=[
        (1, 0, 0, 1, 0, 0),      # Identity for chip 0
        (1, 0, 0, 1, 256, 0),    # Chip 1 offset by 256 in X
    ]
)
```

### Presets

```python
# VENUS detector defaults
config = rustpix.DetectorConfig.venus_defaults()

# Load from JSON
config = rustpix.DetectorConfig.from_json('{"tdc_frequency_hz": 60.0}')
```

## ClusteringConfig

Configure the clustering algorithm:

```python
config = rustpix.ClusteringConfig(
    radius=5.0,
    temporal_window_ns=75.0,
    min_cluster_size=1,
    max_cluster_size=None
)
```

### Parameters

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `radius` | `float` | `5.0` | Spatial epsilon in pixels |
| `temporal_window_ns` | `float` | `75.0` | Temporal epsilon in nanoseconds |
| `min_cluster_size` | `int` | `1` | Minimum hits per cluster |
| `max_cluster_size` | `int` | `None` | Maximum hits per cluster (optional) |

### Tuning Tips

- **radius**: Larger values merge more hits. Start with 5.0 for typical neutron events.
- **temporal_window_ns**: Should match detector timing characteristics. 75ns works for most TPX3 setups.
- **min_cluster_size**: Set to 2+ to filter noise (single-hit events).
- **max_cluster_size**: Set to filter large background events (e.g., gamma showers).

## ExtractionConfig

Configure centroid extraction:

```python
config = rustpix.ExtractionConfig(
    super_resolution_factor=8.0,
    weighted_by_tot=True,
    min_tot_threshold=10
)
```

### Parameters

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `super_resolution_factor` | `float` | `8.0` | Sub-pixel resolution multiplier |
| `weighted_by_tot` | `bool` | `True` | Weight centroid by ToT (charge) |
| `min_tot_threshold` | `int` | `0` | Filter hits below this ToT |

### Super Resolution

The `super_resolution_factor` controls sub-pixel precision:
- `1.0`: Integer pixel coordinates
- `8.0`: 1/8 pixel precision (default)
- `16.0`: 1/16 pixel precision

### ToT Weighting

When `weighted_by_tot=True`, the centroid is computed as:

```
x_centroid = Σ(x_i * tot_i) / Σ(tot_i)
y_centroid = Σ(y_i * tot_i) / Σ(tot_i)
```

This improves resolution by weighting toward hits with higher charge deposition.

## Algorithm-Specific Parameters

Pass algorithm parameters as keyword arguments:

```python
# ABS algorithm
neutrons = rustpix.process_tpx3_neutrons(
    "data.tpx3",
    algorithm="abs",
    abs_scan_interval=1000,  # Scan interval for ABS
    collect=True
)

# DBSCAN algorithm
neutrons = rustpix.process_tpx3_neutrons(
    "data.tpx3",
    algorithm="dbscan",
    dbscan_min_points=2,  # Min points for core sample
    collect=True
)

# Grid algorithm
neutrons = rustpix.process_tpx3_neutrons(
    "data.tpx3",
    algorithm="grid",
    grid_cell_size=10,  # Grid cell size in pixels
    collect=True
)
```

## Out-of-Core Processing Parameters

For streaming with memory constraints:

```python
for batch in rustpix.stream_tpx3_neutrons(
    "huge_file.tpx3",
    clustering_config=rustpix.ClusteringConfig(),

    # Memory management
    out_of_core=True,           # Enable (default: True for streaming)
    memory_fraction=0.5,        # Target 50% of available RAM
    memory_budget_bytes=4_000_000_000,  # Or explicit 4GB limit

    # Parallelism
    parallelism=4,              # Worker threads
    queue_depth=8,              # Pipeline queue depth
    async_io=True               # Async I/O pipeline
):
    process_batch(batch)
```

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `out_of_core` | `bool` | `True` | Enable out-of-core processing |
| `memory_fraction` | `float` | `0.5` | Fraction of RAM to target |
| `memory_budget_bytes` | `int` | Auto | Explicit memory budget |
| `parallelism` | `int` | CPU count | Worker thread count |
| `queue_depth` | `int` | `8` | Pipeline queue depth |
| `async_io` | `bool` | `True` | Enable async I/O |
