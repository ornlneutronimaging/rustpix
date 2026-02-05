# rustpix

[![CI](https://github.com/ornlneutronimaging/rustpix/actions/workflows/ci.yml/badge.svg)](https://github.com/ornlneutronimaging/rustpix/actions/workflows/ci.yml)
[![Documentation](https://img.shields.io/badge/docs-mdBook-blue.svg)](https://ornlneutronimaging.github.io/rustpix/)
[![Crates.io](https://img.shields.io/crates/v/rustpix-core.svg)](https://crates.io/crates/rustpix-core)
[![PyPI](https://img.shields.io/pypi/v/rustpix.svg)](https://pypi.org/project/rustpix/)
[![DOI](https://zenodo.org/badge/DOI/10.5281/zenodo.18496371.svg)](https://doi.org/10.5281/zenodo.18496371)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

High-performance pixel detector data processing for neutron imaging. Supports Timepix3 (TPX3) with 96M+ hits/sec throughput. Features multiple clustering algorithms, centroid extraction, and Python bindings.

## Features

- **Fast TPX3 Processing**: Parallel packet parsing with memory-mapped I/O
- **Multiple Clustering Algorithms**:
  - ABS (Adjacency-Based Search) - 8-connectivity clustering
  - DBSCAN - Density-based with spatial indexing
  - Graph - Union-find connected components
  - Grid - Parallel grid-based clustering
- **Streaming Architecture**: Process files larger than RAM
- **Python Bindings**: Thin wrappers with NumPy integration
- **CLI Tool**: Command-line interface for batch processing
- **GUI Application**: Interactive analysis with real-time visualization
- **Multiple Output Formats**: HDF5, Arrow, CSV

## Installation

### Python (pip)

```bash
pip install rustpix
```

### macOS (Homebrew)

```bash
brew tap ornlneutronimaging/rustpix
brew install --cask rustpix  # GUI app
```

### Rust (cargo)

```bash
# CLI tool
cargo install rustpix-cli

# Library
cargo add rustpix-core rustpix-algorithms
```

### From Source

```bash
# Using pixi (recommended)
git clone https://github.com/ornlneutronimaging/rustpix
cd rustpix
pixi install
pixi run build

# Or with cargo
cargo build --release --workspace
```

## Quick Start

### Python

```python
import rustpix

# Process TPX3 file to neutron events
config = rustpix.ClusteringConfig(radius=5.0, temporal_window_ns=75.0)
neutrons = rustpix.process_tpx3_neutrons(
    "data.tpx3",
    clustering_config=config,
    algorithm="abs"
)

# Convert to NumPy
data = neutrons.to_numpy()
print(f"Found {len(data['x'])} neutron events")
```

### Command Line

```bash
# Process file
rustpix process input.tpx3 -o output.h5

# Show file info
rustpix info input.tpx3

# Benchmark algorithms
rustpix benchmark input.tpx3
```

### GUI Application

Launch the GUI for interactive analysis:

```bash
# macOS (Homebrew)
open -a Rustpix

# From source
pixi run gui
cargo run -p rustpix-gui --release
```

## Workspace Structure

| Crate                    | Description                                          |
| ------------------------ | ---------------------------------------------------- |
| **rustpix-core**         | Core traits and types                                |
| **rustpix-tpx**          | TPX3 packet parser and hit types                     |
| **rustpix-algorithms**   | Clustering algorithms (ABS, DBSCAN, Graph, Grid)     |
| **rustpix-io**           | File I/O with memory-mapped reading                  |
| **rustpix-python**       | Python bindings (PyO3)                               |
| **rustpix-cli**          | Command-line interface                               |
| **rustpix-gui**          | GUI application (egui)                               |

## Python API

### Read Hits

```python
import rustpix

# Read all hits
hits = rustpix.read_tpx3_hits("input.tpx3")
hits_np = hits.to_numpy()

# Stream hits in batches
for batch in rustpix.stream_tpx3_hits("input.tpx3"):
    process(batch.to_numpy())
```

### Process Neutrons

```python
# Configure clustering
clustering = rustpix.ClusteringConfig(
    radius=5.0,
    temporal_window_ns=75.0,
    min_cluster_size=1
)

# Configure centroid extraction
extraction = rustpix.ExtractionConfig(
    super_resolution_factor=8.0,
    weighted_by_tot=True,
    min_tot_threshold=10
)

# Stream processing (low memory)
for batch in rustpix.process_tpx3_neutrons(
    "input.tpx3",
    clustering_config=clustering,
    extraction_config=extraction,
    algorithm="abs"
):
    save_batch(batch.to_numpy())

# Batch processing (collect all)
neutrons = rustpix.process_tpx3_neutrons(
    "input.tpx3",
    clustering_config=clustering,
    extraction_config=extraction,
    algorithm="abs",
    collect=True
)
```

## CLI Usage

```bash
# Process with custom parameters
rustpix process input.tpx3 -o output.csv \
    --algorithm dbscan \
    --spatial-epsilon 2.0 \
    --temporal-epsilon 500

# Show file information
rustpix info input.tpx3

# Benchmark different algorithms
rustpix benchmark input.tpx3 --iterations 5

# Convert formats
rustpix convert input.tpx3 -f hdf5 -o output.h5
```

## Performance

- **Throughput**: 96M+ hits/sec on modern hardware
- **Memory**: Streaming architecture processes files larger than RAM
- **Parallel**: Multi-threaded clustering with rayon
- **Optimized**: SIMD-friendly data layouts (SoA)

## Development

```bash
# Install dependencies with pixi
pixi install

# Run tests
pixi run test

# Format and lint
pixi run lint

# Build documentation
pixi run docs

# Run GUI in debug mode
pixi run gui-debug
```

## Documentation

- **User Guide**: [ornlneutronimaging.github.io/rustpix](https://ornlneutronimaging.github.io/rustpix/)
- **Rust API**: [docs.rs/rustpix-core](https://docs.rs/rustpix-core)
- **Design Docs**: [docs/](docs/) directory

## Citation

If you use rustpix in your research, please cite:

```bibtex
@software{rustpix2026,
  title = {rustpix: High-performance pixel detector data processing},
  author = {{ORNL Neutron Imaging Team}},
  year = {2026},
  url = {https://github.com/ornlneutronimaging/rustpix},
  doi = {10.5281/zenodo.18496371}
}
```

## License

MIT License - see [LICENSE](LICENSE) for details.

## Contributing

Contributions welcome! See [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

## Acknowledgments

Developed by the Neutron Imaging Team at Oak Ridge National Laboratory.
