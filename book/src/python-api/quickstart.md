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
