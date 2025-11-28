# rustpix

High-performance Rust library for pixel detector data processing. Supports Timepix3 (TPX3) neutron detection with 96M+ hits/sec throughput. Features multiple clustering algorithms (ABS, DBSCAN, Graph, Grid), centroid extraction, and first-class Python bindings via PyO3. Designed for extensibility to TPX4 and other detector types.

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

- **rustpix-python**: Python bindings
  - PyO3-based Python module
  - NumPy structured array support for efficient data exchange

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
cargo test --workspace --exclude rustpix-python
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

# Read hits from TPX3 file
hits = rustpix.read_tpx3_file("input.tpx3")

# Or get numpy arrays directly
data = rustpix.read_tpx3_file_numpy("input.tpx3")
# data["x"], data["y"], data["toa"], data["tot"] are numpy arrays

# Cluster hits
config = rustpix.ClusteringConfig(
    spatial_epsilon=1.5,
    temporal_epsilon=1000,
    min_cluster_size=2
)
clusters = rustpix.cluster_hits(hits, config, algorithm="abs")

# Extract centroids
centroids = rustpix.extract_centroids(clusters)

# Or process file in one call
centroids = rustpix.process_tpx3_file("input.tpx3")
```

## License

MIT License
