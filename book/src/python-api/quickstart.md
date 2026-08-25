# Quick Start

## Reading Hits

Load all hits from a TPX3 file into memory:

```python
import rustpix

# Read all hits
hits = rustpix.read_tpx3_hits("data.tpx3")

# Convert to NumPy arrays
data = hits.to_numpy()
print(f"Loaded {len(data['x'])} hits")

# Access individual arrays
x = data['x']      # uint16
y = data['y']      # uint16
tof = data['tof']  # uint32, 25ns ticks (multiply by 25 for nanoseconds)
tot = data['tot']  # uint16
```

## Streaming Hits

For large files, stream hits in batches:

```python
import rustpix

for batch in rustpix.stream_tpx3_hits("large_data.tpx3"):
    data = batch.to_numpy()
    process_batch(data)
```

## Processing Neutrons

Convert hits to neutron events using clustering:

```python
import rustpix

# Configure clustering
clustering = rustpix.ClusteringConfig(
    radius=5.0,              # spatial epsilon (pixels)
    temporal_window_ns=75.0, # temporal epsilon (nanoseconds)
    min_cluster_size=1
)

# Configure centroid extraction
extraction = rustpix.ExtractionConfig(
    super_resolution_factor=8.0,
    weighted_by_tot=True,
    min_tot_threshold=10
)

# Process file (returns single batch)
neutrons = rustpix.process_tpx3_neutrons(
    "data.tpx3",
    clustering_config=clustering,
    extraction_config=extraction,
    algorithm="abs",
    collect=True
)

# Convert to NumPy
data = neutrons.to_numpy()
print(f"Found {len(data['x'])} neutron events")
```

## Streaming Neutrons

Stream neutron events for large files:

```python
import rustpix

clustering = rustpix.ClusteringConfig(radius=5.0, temporal_window_ns=75.0)

# Stream neutrons (default mode)
for batch in rustpix.stream_tpx3_neutrons(
    "large_data.tpx3",
    clustering_config=clustering
):
    data = batch.to_numpy()
    save_batch(data)
```

Or use `process_tpx3_neutrons` without `collect=True`:

```python
# Streaming is the default
for batch in rustpix.process_tpx3_neutrons(
    "large_data.tpx3",
    clustering_config=clustering
):
    process_batch(batch.to_numpy())
```

## Clustering Hits

Cluster an existing HitBatch:

```python
import rustpix

# Read hits
hits = rustpix.read_tpx3_hits("data.tpx3")

# Cluster
clustering = rustpix.ClusteringConfig(radius=5.0, temporal_window_ns=75.0)
neutrons = rustpix.cluster_hits(
    hits,
    clustering_config=clustering,
    algorithm="dbscan"
)

data = neutrons.to_numpy()
```

## PyArrow Integration

Export to PyArrow for Parquet, Arrow IPC, or DataFrame conversion:

```python
import rustpix

neutrons = rustpix.process_tpx3_neutrons("data.tpx3", collect=True)

# Convert to PyArrow Table
table = neutrons.to_arrow()

# Save as Parquet
import pyarrow.parquet as pq
pq.write_table(table, "neutrons.parquet")

# Convert to Pandas
df = table.to_pandas()
```

## VENUS Detector Defaults

For VENUS detector at SNS:

```python
import rustpix

# Use VENUS-specific defaults
detector = rustpix.DetectorConfig.venus_defaults()
clustering = rustpix.ClusteringConfig.venus_defaults()
extraction = rustpix.ExtractionConfig.venus_defaults()

neutrons = rustpix.process_tpx3_neutrons(
    "venus_data.tpx3",
    detector_config=detector,
    clustering_config=clustering,
    extraction_config=extraction,
    collect=True
)
```

## HDF5 Export

Write neutron events directly to HDF5 by passing an `output_path`:

```python
import rustpix

clustering = rustpix.ClusteringConfig.venus_defaults()

# Generic NeXus HDF5 (scipp-compatible)
rustpix.process_tpx3_neutrons(
    "venus_data.tpx3",
    clustering_config=clustering,
    output_path="neutrons.h5"
)

# ORNL SNS NXsnsevent HDF5
rustpix.process_tpx3_neutrons(
    "venus_data.tpx3",
    clustering_config=clustering,
    output_path="VENUS_12345.nxs.h5"
)
```

The format is auto-detected from the file extension:
- `.h5`, `.hdf5`, `.nxs` → Generic NeXus HDF5
- `.nxs.h5` → ORNL SNS NXsnsevent HDF5

## Reading SNS NeXus Files

Read decoded events back from SNS NeXus files — both facility files
produced by the ADARA translation service (e.g. `VENUS_15159.nxs.h5`)
and rustpix's own NXsnsevent exports:

```python
import rustpix

# Summarize banks and run metadata
info = rustpix.sns_file_info("/SNS/VENUS/IPTS-35004/nexus/VENUS_15159.nxs.h5")
print(info["banks"])       # {'bank100': {'events': 1545255312, 'pulses': 3675}, ...}
print(info["run_number"], info["start_time"])

# Read a slice of events (defaults: bank100, VENUS 512x512 geometry)
events = rustpix.read_sns_events(
    "/SNS/VENUS/IPTS-35004/nexus/VENUS_15159.nxs.h5",
    start=0,
    count=10_000_000,
)
events["x"], events["y"]      # uint16 pixel coordinates (gap-removed grid)
events["tof_ns"]              # uint64 time-of-flight in nanoseconds
events["pulse_time_ns"]       # uint64 absolute pulse time (Unix epoch ns)
```

`count=None` reads the whole bank. For multi-billion-event files, loop in
chunks — `start`/`count` are clamped to the bank size, so iterate until the
returned arrays are empty:

```python
start = 0
while True:
    events = rustpix.read_sns_events(path, start=start, count=100_000_000)
    if len(events["x"]) == 0:
        break
    ...  # process chunk
    start += len(events["x"])
```

Note: facility files store only pixel ID and TOF per event — ToT, chip IDs,
and per-hit timestamps from the raw `.tpx3` stream are not recorded, so
re-clustering is not possible from a NeXus file alone.

## Out-of-Core Processing

For files larger than RAM:

```python
import rustpix

# Configure memory-bounded processing
for batch in rustpix.stream_tpx3_neutrons(
    "huge_file.tpx3",
    clustering_config=rustpix.ClusteringConfig(),
    memory_fraction=0.5,    # Use up to 50% of RAM
    parallelism=4,          # Worker threads
    async_io=True           # Async reader pipeline
):
    save_batch(batch.to_numpy())
```
