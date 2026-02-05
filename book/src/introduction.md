<p align="center">
  <img src="images/logo.svg" alt="Rustpix Logo" width="128" height="128">
</p>

# Introduction

**Rustpix** is a high-performance pixel detector data processing library for neutron imaging. It processes Timepix3 (TPX3) data with throughput exceeding 96 million hits per second, featuring multiple clustering algorithms, centroid extraction, and Python bindings.

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

## Performance

- **Throughput**: 96M+ hits/sec on modern hardware
- **Memory**: Streaming architecture processes files larger than RAM
- **Parallel**: Multi-threaded clustering with rayon
- **Optimized**: SIMD-friendly data layouts (SoA)

## Getting Started

Choose the interface that best fits your workflow:

- **[Python API](python-api/README.md)** - For scripting and integration with scientific Python stack
- **[CLI Tool](cli/README.md)** - For batch processing and shell scripts
- **[GUI Application](gui/README.md)** - For interactive exploration and analysis

## Workspace Structure

Rustpix is organized as a Rust workspace with multiple crates:

| Crate | Description |
|-------|-------------|
| **rustpix-core** | Core traits and types |
| **rustpix-tpx** | TPX3 packet parser and hit types |
| **rustpix-algorithms** | Clustering algorithms (ABS, DBSCAN, Graph, Grid) |
| **rustpix-io** | File I/O with memory-mapped reading |
| **rustpix-python** | Python bindings (PyO3) |
| **rustpix-cli** | Command-line interface |
| **rustpix-gui** | GUI application (egui) |

## Links

- [GitHub Repository](https://github.com/ornlneutronimaging/rustpix)
- [PyPI Package](https://pypi.org/project/rustpix/)
- [crates.io (rustpix-core)](https://crates.io/crates/rustpix-core)
- [Rust API Documentation](https://docs.rs/rustpix-core)
