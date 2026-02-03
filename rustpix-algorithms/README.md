# rustpix-algorithms

Clustering algorithms for pixel detector data processing.

## Overview

This crate provides high-performance clustering algorithms optimized for pixel detector data:

- **ABS (Adaptive Box Search)** - Fast grid-based clustering
- **DBSCAN** - Density-based spatial clustering
- **Graph Clustering** - Connected component analysis
- **Grid Clustering** - Regular grid-based grouping

All algorithms support parallel processing via Rayon for maximum performance.

## Usage

```rust
use rustpix_algorithms::{AbsClustering, ClusteringAlgorithm};
use rustpix_core::PixelHit;

// Create clustering algorithm
let mut clusterer = AbsClustering::new(5.0, 100.0); // spatial_eps, time_eps

// Process hits
let hits: Vec<PixelHit> = /* your pixel hits */;
let clusters = clusterer.cluster(&hits);

println!("Found {} clusters", clusters.len());
```

## Algorithm Selection Guide

| Algorithm | Best For | Performance |
|-----------|----------|-------------|
| ABS | Dense, uniform data | Very fast |
| DBSCAN | Variable density | Fast |
| Graph | Sparse data | Moderate |
| Grid | Regular patterns | Very fast |

## Features

- `serde` - Enable serialization/deserialization support

## License

MIT License - see [LICENSE](../LICENSE) for details.
