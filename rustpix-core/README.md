# rustpix-core

Core traits and types for the rustpix pixel detector data processing library.

## Overview

This crate provides the foundational types and traits used across the rustpix ecosystem:

- `PixelHit` - Represents a single pixel hit with coordinates, time-of-arrival, and time-over-threshold
- `Cluster` - A collection of pixel hits grouped together
- `ClusterStats` - Statistical properties of a cluster (centroid, total ToT, etc.)
- Traits for clustering algorithms and data processing

## Usage

```rust
use rustpix_core::{PixelHit, Cluster, ClusterStats};

// Create a pixel hit
let hit = PixelHit::new(100, 200, 1000.0, 50);

// Access hit properties
println!("Position: ({}, {})", hit.x(), hit.y());
println!("Time of Arrival: {} ns", hit.toa());
println!("Time over Threshold: {}", hit.tot());
```

## Features

- `serde` - Enable serialization/deserialization support

## License

MIT License - see [LICENSE](../LICENSE) for details.
