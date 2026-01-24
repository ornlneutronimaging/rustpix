# rustpix

High-performance Rust library for pixel detector data processing. Supports Timepix3 (TPX3) neutron detection with 96M+ hits/sec throughput. Features multiple clustering algorithms (ABS, DBSCAN, Graph, Grid), centroid extraction, and thin Python bindings. Designed for extensibility to TPX4 and other detector types.

## Workspace Structure

- **rustpix-core**: Core traits and types
  - Hit/Neutron traits for detector-agnostic interfaces
  - Clustering and extraction trait definitions
  - Error types and common data structures

- **rustpix-tpx**: TPX3-specific implementation
  - TPX3 packet parser
  - TPX3 hit types with timing information
  - Parallel file processing

- **rustpix-algorithms**: Clustering algorithms
  - ABS (Adjacency-Based Search) - fast 8-connectivity clustering
  - DBSCAN - density-based clustering with spatial indexing
  - Graph - union-find based connected component detection
  - Grid - parallel grid-based clustering with spatial indexing

- **rustpix-io**: File I/O
  - memmap2-based memory-mapped file reading
  - CSV and binary output writers

- **rustpix-python**: Python bindings (thin wrapper)
  - PyO3 module exposing Rust pipelines
  - NumPy SoA outputs with metadata

- **rustpix-cli**: Command-line interface
  - Process TPX3 files to extract neutron events
  - File information and benchmarking commands

## Building

```bash
# Build all crates
cargo build --workspace

# Build in release mode
cargo build --workspace --release

# Run tests
cargo test --workspace
```

## CLI Usage

```bash
# Process TPX3 file
rustpix process input.tpx3 -o output.csv

# Process with custom clustering parameters
rustpix process input.tpx3 -o output.csv \
    --algorithm dbscan \
    --spatial-epsilon 2.0 \
    --temporal-epsilon 500

# Show file information
rustpix info input.tpx3

# Benchmark clustering algorithms
rustpix benchmark input.tpx3 --iterations 5
```

## Python Usage

```python
import rustpix

# Read hits (SoA) with metadata
hits = rustpix.read_tpx3_hits("input.tpx3")
hits_np = hits.to_numpy()
meta = hits.metadata()

# Stream hits in time order (pulse-merged)
for batch in rustpix.stream_tpx3_hits("input.tpx3"):
    batch_np = batch.to_numpy()

# Process file into neutrons (streaming by default)
clustering = rustpix.ClusteringConfig(radius=5.0, temporal_window_ns=75.0, min_cluster_size=1)
extraction = rustpix.ExtractionConfig(super_resolution_factor=8.0, weighted_by_tot=True, min_tot_threshold=10)
neutron_stream = rustpix.process_tpx3_neutrons(
    "input.tpx3",
    clustering_config=clustering,
    extraction_config=extraction,
    algorithm="abs",
)
for batch in neutron_stream:
    batch_np = batch.to_numpy()

# Collect full batch (for small files)
neutrons = rustpix.process_tpx3_neutrons(
    "input.tpx3",
    clustering_config=clustering,
    extraction_config=extraction,
    algorithm="abs",
    collect=True,
)
neutrons_np = neutrons.to_numpy()
```

## License

MIT License
