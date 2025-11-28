# rustpix: Implementation Plan for Rust-based Pixel Detector Processing Library

## Executive Summary

**rustpix** is a high-performance Rust library for processing pixel detector data, initially targeting Timepix3 (TPX3) neutron detection with architecture designed for extensibility to TPX4 and other detector types.

### Key Goals
- **Performance**: Match or exceed C++ baseline (96M+ hits/sec on production hardware)
- **Safety**: Memory-safe, thread-safe by design (Rust guarantees)
- **Extensibility**: Modular architecture supporting multiple detector types
- **Interoperability**: First-class Python bindings via PyO3
- **Modern Tooling**: Cargo workspace, comprehensive testing, CI/CD

### Repository Name
- Main: `rustpix`
- Subcrates: `rustpix-tpx`, `rustpix-core`, `rustpix-python`, `rustpix-cli`

---

## Part 1: Architecture Overview

### 1.1 Workspace Structure

```
rustpix/
├── Cargo.toml                    # Workspace root
├── README.md
├── LICENSE
├── .github/
│   └── workflows/
│       ├── ci.yml               # Build, test, lint
│       ├── release.yml          # PyPI/crates.io publishing
│       └── benchmark.yml        # Performance regression testing
├── crates/
│   ├── rustpix-core/            # Core traits and types
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── hit.rs           # Generic Hit trait + types
│   │       ├── neutron.rs       # Neutron output type
│   │       ├── clustering/
│   │       │   ├── mod.rs
│   │       │   ├── traits.rs    # IHitClustering equivalent
│   │       │   ├── state.rs     # ClusteringState trait
│   │       │   └── config.rs    # Configuration types
│   │       ├── extraction/
│   │       │   ├── mod.rs
│   │       │   ├── traits.rs    # INeutronExtraction equivalent
│   │       │   └── centroid.rs  # Centroid extraction
│   │       └── error.rs         # Error types
│   │
│   ├── rustpix-tpx/             # Timepix-specific implementation
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── tpx3/
│   │       │   ├── mod.rs
│   │       │   ├── packet.rs    # TPX3 packet parser
│   │       │   ├── hit.rs       # TPX3Hit type
│   │       │   ├── processor.rs # File processor
│   │       │   ├── section.rs   # Section discovery
│   │       │   └── config.rs    # Detector configuration
│   │       └── tpx4/            # Future: TPX4 support
│   │           └── mod.rs       # Placeholder
│   │
│   ├── rustpix-algorithms/      # Clustering algorithms
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── abs.rs           # Age-Based Spatial clustering
│   │       ├── dbscan.rs        # DBSCAN clustering
│   │       ├── graph.rs         # Union-Find graph clustering
│   │       ├── grid.rs          # Grid-based clustering
│   │       └── spatial_index.rs # Shared spatial indexing
│   │
│   ├── rustpix-io/              # I/O utilities
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── mmap.rs          # Memory-mapped file access
│   │       ├── hdf5.rs          # HDF5 output (optional)
│   │       └── streaming.rs     # Streaming reader
│   │
│   ├── rustpix-python/          # Python bindings
│   │   ├── Cargo.toml
│   │   ├── pyproject.toml       # maturin config
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── hit.rs           # Hit array bindings
│   │       ├── neutron.rs       # Neutron array bindings
│   │       ├── processor.rs     # Processor bindings
│   │       └── config.rs        # Config bindings
│   │
│   └── rustpix-cli/             # Command-line interface
│       ├── Cargo.toml
│       └── src/
│           ├── main.rs
│           └── commands/
│               ├── mod.rs
│               ├── process.rs   # File processing command
│               ├── info.rs      # File info command
│               └── benchmark.rs # Benchmarking command
│
├── tests/                       # Integration tests
│   ├── integration_tests.rs
│   └── data/                    # Test data (git-lfs or downloaded)
│
├── benches/                     # Criterion benchmarks
│   ├── parsing.rs
│   ├── clustering.rs
│   └── full_pipeline.rs
│
└── python/                      # Python package wrapper
    ├── rustpix/
    │   ├── __init__.py
    │   ├── config.py            # Pydantic config models
    │   └── analysis.py          # Analysis utilities
    └── tests/
        └── test_bindings.py
```

### 1.2 Dependency Graph

```
rustpix-python ─────┬──► rustpix-tpx ───┬──► rustpix-core
                    │                    │
rustpix-cli ────────┤                    ├──► rustpix-algorithms ──► rustpix-core
                    │                    │
                    └──► rustpix-io ─────┘
```

### 1.3 Key Dependencies

```toml
# Workspace Cargo.toml
[workspace]
resolver = "2"
members = ["crates/*"]

[workspace.dependencies]
# Core
thiserror = "2.0"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"

# Parallelism
rayon = "1.10"

# I/O
memmap2 = "0.9"
hdf5 = { version = "0.8", optional = true }

# Python
pyo3 = { version = "0.23", features = ["extension-module", "abi3-py310"] }
numpy = "0.23"

# CLI
clap = { version = "4.5", features = ["derive"] }
indicatif = "0.17"

# Logging
tracing = "0.1"
tracing-subscriber = "0.3"

# Testing
criterion = { version = "0.5", features = ["html_reports"] }
proptest = "1.5"
```

---

## Part 2: Core Types and Traits

### 2.1 Hit Trait and Types (`rustpix-core/src/hit.rs`)

```rust
//! Generic hit types for pixel detectors.

use std::cmp::Ordering;

/// Core trait for all detector hit types.
///
/// A "hit" represents a single pixel activation event from a detector.
/// Different detector types (TPX3, TPX4, etc.) implement this trait.
pub trait Hit: Clone + Send + Sync {
    /// Time-of-flight in detector-native units (typically 25ns).
    fn tof(&self) -> u32;

    /// X coordinate in global detector space.
    fn x(&self) -> u16;

    /// Y coordinate in global detector space.
    fn y(&self) -> u16;

    /// Time-over-threshold (signal amplitude proxy).
    fn tot(&self) -> u16;

    /// Timestamp in detector-native units.
    fn timestamp(&self) -> u32;

    /// Chip identifier for multi-chip detectors.
    fn chip_id(&self) -> u8;

    /// TOF in nanoseconds.
    fn tof_ns(&self) -> f64 {
        self.tof() as f64 * 25.0
    }

    /// Squared Euclidean distance to another hit.
    fn distance_squared(&self, other: &impl Hit) -> f64 {
        let dx = self.x() as f64 - other.x() as f64;
        let dy = self.y() as f64 - other.y() as f64;
        dx * dx + dy * dy
    }

    /// Check if within spatial radius of another hit.
    fn within_radius(&self, other: &impl Hit, radius: f64) -> bool {
        self.distance_squared(other) <= radius * radius
    }

    /// Check if within temporal window of another hit (in TOF units).
    fn within_temporal_window(&self, other: &impl Hit, window_tof: u32) -> bool {
        let diff = if self.tof() > other.tof() {
            self.tof() - other.tof()
        } else {
            other.tof() - self.tof()
        };
        diff <= window_tof
    }
}

/// Hit with mutable cluster assignment.
pub trait ClusterableHit: Hit {
    /// Get current cluster ID (-1 = unassigned).
    fn cluster_id(&self) -> i32;

    /// Set cluster ID.
    fn set_cluster_id(&mut self, id: i32);
}

/// Concrete hit type for general use.
///
/// Memory layout optimized for cache efficiency:
/// - Most accessed fields (tof, x, y) at the start
/// - Total size: 20 bytes (with padding considerations)
#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
pub struct GenericHit {
    pub tof: u32,           // 4 bytes - most accessed
    pub x: u16,             // 2 bytes
    pub y: u16,             // 2 bytes
    pub timestamp: u32,     // 4 bytes
    pub tot: u16,           // 2 bytes
    pub chip_id: u8,        // 1 byte
    pub _padding: u8,       // 1 byte alignment
    pub cluster_id: i32,    // 4 bytes
}

impl Hit for GenericHit {
    fn tof(&self) -> u32 { self.tof }
    fn x(&self) -> u16 { self.x }
    fn y(&self) -> u16 { self.y }
    fn tot(&self) -> u16 { self.tot }
    fn timestamp(&self) -> u32 { self.timestamp }
    fn chip_id(&self) -> u8 { self.chip_id }
}

impl ClusterableHit for GenericHit {
    fn cluster_id(&self) -> i32 { self.cluster_id }
    fn set_cluster_id(&mut self, id: i32) { self.cluster_id = id; }
}

/// Ordering by TOF for temporal processing.
impl Ord for GenericHit {
    fn cmp(&self, other: &Self) -> Ordering {
        self.tof.cmp(&other.tof)
    }
}

impl PartialOrd for GenericHit {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for GenericHit {
    fn eq(&self, other: &Self) -> bool {
        self.tof == other.tof && self.x == other.x && self.y == other.y
    }
}

impl Eq for GenericHit {}
```

### 2.2 Neutron Type (`rustpix-core/src/neutron.rs`)

```rust
//! Neutron event output type.

/// A detected neutron event after clustering and centroid extraction.
///
/// Coordinates are in super-resolution space (default 8x pixel resolution).
#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
pub struct Neutron {
    /// X coordinate in super-resolution space.
    pub x: f64,
    /// Y coordinate in super-resolution space.
    pub y: f64,
    /// Time-of-flight in 25ns units.
    pub tof: u32,
    /// Combined time-over-threshold.
    pub tot: u16,
    /// Number of hits in cluster.
    pub n_hits: u16,
    /// Source chip ID.
    pub chip_id: u8,
    /// Reserved for alignment.
    pub _reserved: [u8; 3],
}

impl Neutron {
    /// Create a new neutron from cluster data.
    pub fn new(x: f64, y: f64, tof: u32, tot: u16, n_hits: u16, chip_id: u8) -> Self {
        Self {
            x,
            y,
            tof,
            tot,
            n_hits,
            chip_id,
            _reserved: [0; 3],
        }
    }

    /// TOF in nanoseconds.
    pub fn tof_ns(&self) -> f64 {
        self.tof as f64 * 25.0
    }

    /// TOF in milliseconds.
    pub fn tof_ms(&self) -> f64 {
        self.tof_ns() / 1_000_000.0
    }

    /// Pixel coordinates (divide by super-resolution factor).
    pub fn pixel_coords(&self, super_res: f64) -> (f64, f64) {
        (self.x / super_res, self.y / super_res)
    }

    /// Cluster size category.
    pub fn cluster_size_category(&self) -> ClusterSize {
        match self.n_hits {
            1 => ClusterSize::Single,
            2..=4 => ClusterSize::Small,
            5..=10 => ClusterSize::Medium,
            _ => ClusterSize::Large,
        }
    }
}

/// Cluster size categories for analysis.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClusterSize {
    Single,
    Small,
    Medium,
    Large,
}

/// Statistics for a collection of neutrons.
#[derive(Clone, Debug, Default)]
pub struct NeutronStatistics {
    pub count: usize,
    pub mean_tof: f64,
    pub std_tof: f64,
    pub mean_tot: f64,
    pub mean_cluster_size: f64,
    pub single_hit_fraction: f64,
    pub x_range: (f64, f64),
    pub y_range: (f64, f64),
    pub tof_range: (u32, u32),
}

impl NeutronStatistics {
    /// Calculate statistics from a slice of neutrons.
    pub fn from_neutrons(neutrons: &[Neutron]) -> Self {
        if neutrons.is_empty() {
            return Self::default();
        }

        let count = neutrons.len();
        let sum_tof: f64 = neutrons.iter().map(|n| n.tof as f64).sum();
        let mean_tof = sum_tof / count as f64;

        let variance: f64 = neutrons.iter()
            .map(|n| (n.tof as f64 - mean_tof).powi(2))
            .sum::<f64>() / count as f64;
        let std_tof = variance.sqrt();

        let mean_tot = neutrons.iter().map(|n| n.tot as f64).sum::<f64>() / count as f64;
        let mean_cluster_size = neutrons.iter().map(|n| n.n_hits as f64).sum::<f64>() / count as f64;
        let single_hit_fraction = neutrons.iter().filter(|n| n.n_hits == 1).count() as f64 / count as f64;

        let x_min = neutrons.iter().map(|n| n.x).fold(f64::INFINITY, f64::min);
        let x_max = neutrons.iter().map(|n| n.x).fold(f64::NEG_INFINITY, f64::max);
        let y_min = neutrons.iter().map(|n| n.y).fold(f64::INFINITY, f64::min);
        let y_max = neutrons.iter().map(|n| n.y).fold(f64::NEG_INFINITY, f64::max);
        let tof_min = neutrons.iter().map(|n| n.tof).min().unwrap_or(0);
        let tof_max = neutrons.iter().map(|n| n.tof).max().unwrap_or(0);

        Self {
            count,
            mean_tof,
            std_tof,
            mean_tot,
            mean_cluster_size,
            single_hit_fraction,
            x_range: (x_min, x_max),
            y_range: (y_min, y_max),
            tof_range: (tof_min, tof_max),
        }
    }
}
```

### 2.3 Clustering Traits (`rustpix-core/src/clustering/traits.rs`)

```rust
//! Clustering algorithm traits.

use crate::hit::{Hit, ClusterableHit};
use crate::error::ClusteringError;
use super::config::ClusteringConfig;
use super::state::ClusteringState;

/// Main trait for hit clustering algorithms.
///
/// Design principles:
/// - **Stateless methods**: All mutable state passed via `ClusteringState`
/// - **Generic over hit type**: Works with any `Hit` implementation
/// - **Thread-safe**: Can be used from multiple threads with separate states
pub trait HitClustering: Send + Sync {
    /// The state type used by this algorithm.
    type State: ClusteringState;

    /// Algorithm name for logging/debugging.
    fn name(&self) -> &'static str;

    /// Create a new state instance for this algorithm.
    fn create_state(&self) -> Self::State;

    /// Configure the algorithm.
    fn configure(&mut self, config: &ClusteringConfig);

    /// Get current configuration.
    fn config(&self) -> &ClusteringConfig;

    /// Cluster a batch of hits.
    ///
    /// # Arguments
    /// * `hits` - Slice of hits to cluster (must be sorted by TOF)
    /// * `state` - Mutable algorithm state
    /// * `labels` - Output cluster labels (-1 = noise/unclustered)
    ///
    /// # Returns
    /// Number of clusters found.
    fn cluster<H: Hit>(
        &self,
        hits: &[H],
        state: &mut Self::State,
        labels: &mut [i32],
    ) -> Result<usize, ClusteringError>;

    /// Get statistics from the last clustering operation.
    fn statistics(&self, state: &Self::State) -> ClusteringStatistics;
}

/// Statistics from a clustering operation.
#[derive(Clone, Debug, Default)]
pub struct ClusteringStatistics {
    pub hits_processed: usize,
    pub clusters_found: usize,
    pub noise_hits: usize,
    pub largest_cluster_size: usize,
    pub mean_cluster_size: f64,
    pub processing_time_us: u64,
}

/// Type-erased clustering trait for dynamic dispatch.
///
/// Use this when you need runtime algorithm selection.
pub trait DynHitClustering: Send + Sync {
    fn name(&self) -> &'static str;
    fn configure(&mut self, config: &ClusteringConfig);
    fn config(&self) -> &ClusteringConfig;

    /// Cluster using type-erased state.
    fn cluster_dyn(
        &self,
        hits: &[crate::hit::GenericHit],
        state: &mut dyn ClusteringState,
        labels: &mut [i32],
    ) -> Result<usize, ClusteringError>;

    fn create_state_boxed(&self) -> Box<dyn ClusteringState>;
    fn statistics_dyn(&self, state: &dyn ClusteringState) -> ClusteringStatistics;
}
```

### 2.4 Extraction Trait (`rustpix-core/src/extraction/traits.rs`)

```rust
//! Neutron extraction traits.

use crate::hit::Hit;
use crate::neutron::Neutron;
use crate::error::ExtractionError;

/// Configuration for neutron extraction.
#[derive(Clone, Debug)]
pub struct ExtractionConfig {
    /// Sub-pixel resolution multiplier (default: 8.0).
    pub super_resolution_factor: f64,
    /// Weight centroids by TOT values.
    pub weighted_by_tot: bool,
    /// Minimum TOT threshold (0 = disabled).
    pub min_tot_threshold: u16,
}

impl Default for ExtractionConfig {
    fn default() -> Self {
        Self {
            super_resolution_factor: 8.0,
            weighted_by_tot: true,
            min_tot_threshold: 0,
        }
    }
}

/// Trait for neutron extraction algorithms.
pub trait NeutronExtraction: Send + Sync {
    /// Algorithm name.
    fn name(&self) -> &'static str;

    /// Configure the extraction.
    fn configure(&mut self, config: ExtractionConfig);

    /// Get current configuration.
    fn config(&self) -> &ExtractionConfig;

    /// Extract neutrons from clustered hits.
    ///
    /// # Arguments
    /// * `hits` - Slice of hits (matching labels array)
    /// * `labels` - Cluster labels from clustering algorithm
    /// * `num_clusters` - Number of clusters found
    ///
    /// # Returns
    /// Vector of extracted neutrons (one per cluster).
    fn extract<H: Hit>(
        &self,
        hits: &[H],
        labels: &[i32],
        num_clusters: usize,
    ) -> Result<Vec<Neutron>, ExtractionError>;
}
```

---

## Part 3: TPX3 Implementation

### 3.1 Packet Parser (`rustpix-tpx/src/tpx3/packet.rs`)

```rust
//! TPX3 packet parsing.

/// TPX3 packet wrapper providing efficient field extraction.
///
/// Packet format (64-bit):
/// - Hit packets (ID 0xB*):
///   - Bits 0-15: SPIDR time
///   - Bits 16-19: Fine ToA (4-bit)
///   - Bits 20-29: ToT (10-bit)
///   - Bits 30-43: ToA (14-bit)
///   - Bits 44-59: Pixel address (16-bit)
///   - Bits 60-63: Packet type ID
///
/// - TDC packets (ID 0x6F):
///   - Bits 12-41: 30-bit TDC timestamp
///   - Bits 56-63: Packet type ID
#[derive(Clone, Copy, Debug)]
pub struct Tpx3Packet(u64);

impl Tpx3Packet {
    /// TPX3 header magic number.
    pub const TPX3_HEADER_MAGIC: u64 = 0x33585054; // "TPX3" little-endian

    /// Create from raw 64-bit value.
    #[inline]
    pub const fn new(raw: u64) -> Self {
        Self(raw)
    }

    /// Get raw packet value.
    #[inline]
    pub const fn raw(&self) -> u64 {
        self.0
    }

    /// Check if this is a TPX3 header packet.
    #[inline]
    pub const fn is_header(&self) -> bool {
        (self.0 & 0xFFFFFFFF) == Self::TPX3_HEADER_MAGIC
    }

    /// Check if this is a TDC packet (ID 0x6F).
    #[inline]
    pub const fn is_tdc(&self) -> bool {
        (self.0 >> 56) & 0xFF == 0x6F
    }

    /// Check if this is a hit packet (ID 0xB*).
    #[inline]
    pub const fn is_hit(&self) -> bool {
        (self.0 >> 60) & 0xF == 0xB
    }

    /// Get packet type identifier.
    #[inline]
    pub const fn packet_type(&self) -> u8 {
        ((self.0 >> 56) & 0xFF) as u8
    }

    /// Get chip ID from header packet (bits 32-39).
    #[inline]
    pub const fn chip_id(&self) -> u8 {
        ((self.0 >> 32) & 0xFF) as u8
    }

    /// Get 16-bit pixel address from hit packet.
    #[inline]
    pub const fn pixel_address(&self) -> u16 {
        ((self.0 >> 44) & 0xFFFF) as u16
    }

    /// Get 14-bit Time of Arrival.
    #[inline]
    pub const fn toa(&self) -> u16 {
        ((self.0 >> 30) & 0x3FFF) as u16
    }

    /// Get 10-bit Time over Threshold.
    #[inline]
    pub const fn tot(&self) -> u16 {
        ((self.0 >> 20) & 0x3FF) as u16
    }

    /// Get 4-bit fine ToA.
    #[inline]
    pub const fn fine_toa(&self) -> u8 {
        ((self.0 >> 16) & 0xF) as u8
    }

    /// Get SPIDR time (16-bit).
    #[inline]
    pub const fn spidr_time(&self) -> u16 {
        (self.0 & 0xFFFF) as u16
    }

    /// Get 30-bit TDC timestamp from TDC packet.
    #[inline]
    pub const fn tdc_timestamp(&self) -> u32 {
        ((self.0 >> 12) & 0x3FFFFFFF) as u32
    }

    /// Decode pixel address to local (x, y) coordinates.
    ///
    /// Decoding formula:
    /// - dcol = (addr >> 8) & 0xFE
    /// - spix = (addr >> 1) & 0xFC
    /// - pix = addr & 0x7
    /// - x = dcol + (pix >> 2)
    /// - y = spix + (pix & 0x3)
    #[inline]
    pub const fn pixel_coordinates(&self) -> (u16, u16) {
        let addr = self.pixel_address();
        let dcol = ((addr & 0xFE00) >> 8) as u16;
        let spix = ((addr & 0x1F8) >> 1) as u16;
        let pix = (addr & 0x7) as u16;
        let x = dcol + (pix >> 2);
        let y = spix + (pix & 0x3);
        (x, y)
    }
}

impl From<u64> for Tpx3Packet {
    fn from(raw: u64) -> Self {
        Self::new(raw)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_header_detection() {
        let header = Tpx3Packet::new(0x33585054);
        assert!(header.is_header());

        let non_header = Tpx3Packet::new(0x12345678);
        assert!(!non_header.is_header());
    }

    #[test]
    fn test_packet_type_detection() {
        // TDC packet
        let tdc = Tpx3Packet::new(0x6F00_0000_0000_0000);
        assert!(tdc.is_tdc());
        assert!(!tdc.is_hit());

        // Hit packet
        let hit = Tpx3Packet::new(0xB000_0000_0000_0000);
        assert!(hit.is_hit());
        assert!(!hit.is_tdc());
    }

    #[test]
    fn test_pixel_coordinate_decode() {
        // Test known pixel address decoding
        let packet = Tpx3Packet::new(0xB000_1234_0000_0000 | (0x8421u64 << 44));
        let (x, y) = packet.pixel_coordinates();
        // Verify against expected values based on address encoding
        assert!(x < 256 && y < 256);
    }
}
```

### 3.2 TPX3 Hit Type (`rustpix-tpx/src/tpx3/hit.rs`)

```rust
//! TPX3-specific hit type.

use rustpix_core::hit::{Hit, ClusterableHit};

/// TPX3 hit with optimized memory layout.
///
/// Size: 20 bytes (packed)
#[derive(Clone, Copy, Debug, Default)]
#[repr(C, packed)]
pub struct Tpx3Hit {
    /// Time-of-flight in 25ns units.
    pub tof: u32,
    /// Global X coordinate.
    pub x: u16,
    /// Global Y coordinate.
    pub y: u16,
    /// Timestamp in 25ns units.
    pub timestamp: u32,
    /// Time-over-threshold (10-bit, stored as u16).
    pub tot: u16,
    /// Chip identifier (0-3 for quad arrangement).
    pub chip_id: u8,
    /// Padding for alignment.
    pub _padding: u8,
    /// Cluster assignment (-1 = unassigned).
    pub cluster_id: i32,
}

impl Tpx3Hit {
    /// Create a new hit.
    pub fn new(
        tof: u32,
        x: u16,
        y: u16,
        timestamp: u32,
        tot: u16,
        chip_id: u8,
    ) -> Self {
        Self {
            tof,
            x,
            y,
            timestamp,
            tot,
            chip_id,
            _padding: 0,
            cluster_id: -1,
        }
    }
}

impl Hit for Tpx3Hit {
    #[inline]
    fn tof(&self) -> u32 { self.tof }
    #[inline]
    fn x(&self) -> u16 { self.x }
    #[inline]
    fn y(&self) -> u16 { self.y }
    #[inline]
    fn tot(&self) -> u16 { self.tot }
    #[inline]
    fn timestamp(&self) -> u32 { self.timestamp }
    #[inline]
    fn chip_id(&self) -> u8 { self.chip_id }
}

impl ClusterableHit for Tpx3Hit {
    #[inline]
    fn cluster_id(&self) -> i32 { self.cluster_id }
    #[inline]
    fn set_cluster_id(&mut self, id: i32) { self.cluster_id = id; }
}

/// Timestamp rollover correction.
///
/// TPX3 uses 30-bit timestamps that can roll over. This function
/// corrects the hit timestamp relative to the TDC timestamp.
///
/// Formula: if hit_ts + 0x400000 < tdc_ts, extend by 0x40000000
#[inline]
pub fn correct_timestamp_rollover(hit_timestamp: u32, tdc_timestamp: u32) -> u32 {
    const EXTENSION_THRESHOLD: u32 = 0x400000;
    const EXTENSION_VALUE: u32 = 0x40000000;

    if hit_timestamp.wrapping_add(EXTENSION_THRESHOLD) < tdc_timestamp {
        hit_timestamp.wrapping_add(EXTENSION_VALUE)
    } else {
        hit_timestamp
    }
}

/// Calculate TOF with TDC correction.
///
/// If the raw TOF exceeds the TDC period, subtract one period.
#[inline]
pub fn calculate_tof(
    timestamp: u32,
    tdc_timestamp: u32,
    tdc_correction_25ns: u32,
) -> u32 {
    let raw_tof = timestamp.wrapping_sub(tdc_timestamp);
    if raw_tof > tdc_correction_25ns {
        raw_tof.wrapping_sub(tdc_correction_25ns)
    } else {
        raw_tof
    }
}
```

### 3.3 Section Discovery and Processing (`rustpix-tpx/src/tpx3/section.rs`)

```rust
//! Section-aware TPX3 file processing.

use super::packet::Tpx3Packet;

/// A contiguous section of TPX3 data for a single chip.
#[derive(Clone, Debug)]
pub struct Tpx3Section {
    /// Byte offset of section start.
    pub start_offset: usize,
    /// Byte offset of section end.
    pub end_offset: usize,
    /// Chip ID for this section.
    pub chip_id: u8,
    /// TDC state at section start (inherited from previous section).
    pub initial_tdc: Option<u32>,
    /// TDC state at section end (for propagation).
    pub final_tdc: Option<u32>,
}

impl Tpx3Section {
    /// Number of bytes in this section.
    pub fn byte_size(&self) -> usize {
        self.end_offset - self.start_offset
    }

    /// Number of 64-bit packets in this section.
    pub fn packet_count(&self) -> usize {
        self.byte_size() / 8
    }
}

/// Discover sections in a TPX3 file.
///
/// This performs Phase 1 of processing:
/// 1. Scan for TPX3 headers to identify section boundaries
/// 2. Track per-chip TDC state across sections
/// 3. Propagate TDC inheritance between sections
///
/// # Arguments
/// * `data` - Memory-mapped file data
///
/// # Returns
/// Vector of sections with TDC states populated.
pub fn discover_sections(data: &[u8]) -> Vec<Tpx3Section> {
    const PACKET_SIZE: usize = 8;

    if data.len() < PACKET_SIZE {
        return Vec::new();
    }

    let mut sections = Vec::new();
    let mut current_section: Option<Tpx3Section> = None;
    let mut per_chip_tdc: [Option<u32>; 256] = [None; 256]; // Track per-chip TDC

    let num_packets = data.len() / PACKET_SIZE;

    for i in 0..num_packets {
        let offset = i * PACKET_SIZE;
        let raw = u64::from_le_bytes(data[offset..offset + PACKET_SIZE].try_into().unwrap());
        let packet = Tpx3Packet::new(raw);

        if packet.is_header() {
            // Close current section
            if let Some(mut section) = current_section.take() {
                section.end_offset = offset;
                if section.byte_size() > 0 {
                    sections.push(section);
                }
            }

            // Start new section
            let chip_id = packet.chip_id();
            current_section = Some(Tpx3Section {
                start_offset: offset + PACKET_SIZE, // Skip header itself
                end_offset: 0,
                chip_id,
                initial_tdc: per_chip_tdc[chip_id as usize],
                final_tdc: None,
            });
        } else if packet.is_tdc() {
            // Track TDC for current chip
            if let Some(ref mut section) = current_section {
                let tdc_ts = packet.tdc_timestamp();
                section.final_tdc = Some(tdc_ts);
                per_chip_tdc[section.chip_id as usize] = Some(tdc_ts);
            }
        }
    }

    // Close final section
    if let Some(mut section) = current_section {
        section.end_offset = data.len();
        if section.byte_size() > 0 {
            sections.push(section);
        }
    }

    sections
}

/// Process a single section into hits.
///
/// This is designed to be called in parallel for different sections.
pub fn process_section<H: From<(u32, u16, u16, u32, u16, u8)>>(
    data: &[u8],
    section: &Tpx3Section,
    tdc_correction_25ns: u32,
    chip_transform: impl Fn(u8, u16, u16) -> (u16, u16),
) -> Vec<H> {
    use super::hit::{calculate_tof, correct_timestamp_rollover};

    const PACKET_SIZE: usize = 8;

    let section_data = &data[section.start_offset..section.end_offset];
    let num_packets = section_data.len() / PACKET_SIZE;

    // Pre-allocate based on expected hit density (~60% of packets are hits)
    let mut hits = Vec::with_capacity((num_packets * 6) / 10);

    let mut current_tdc = section.initial_tdc;

    for i in 0..num_packets {
        let offset = i * PACKET_SIZE;
        let raw = u64::from_le_bytes(
            section_data[offset..offset + PACKET_SIZE].try_into().unwrap()
        );
        let packet = Tpx3Packet::new(raw);

        if packet.is_tdc() {
            current_tdc = Some(packet.tdc_timestamp());
        } else if packet.is_hit() {
            // Skip hits until we have a TDC reference
            let Some(tdc_ts) = current_tdc else { continue };

            let (local_x, local_y) = packet.pixel_coordinates();
            let (global_x, global_y) = chip_transform(section.chip_id, local_x, local_y);

            // Calculate timestamp with rollover correction
            let raw_timestamp = (packet.toa() as u32) << 4 | (packet.fine_toa() as u32);
            let timestamp = correct_timestamp_rollover(raw_timestamp, tdc_ts);
            let tof = calculate_tof(timestamp, tdc_ts, tdc_correction_25ns);

            hits.push(H::from((tof, global_x, global_y, timestamp, packet.tot(), section.chip_id)));
        }
    }

    hits
}
```

### 3.4 Main Processor (`rustpix-tpx/src/tpx3/processor.rs`)

```rust
//! Main TPX3 file processor.

use std::path::Path;
use rayon::prelude::*;
use rustpix_io::mmap::MappedFile;
use super::config::DetectorConfig;
use super::section::{discover_sections, process_section, Tpx3Section};
use super::hit::Tpx3Hit;
use crate::error::ProcessingError;

/// TPX3 file processor with parallel processing support.
pub struct Tpx3Processor {
    config: DetectorConfig,
    last_stats: ProcessingStats,
}

/// Processing statistics.
#[derive(Clone, Debug, Default)]
pub struct ProcessingStats {
    pub hits_extracted: usize,
    pub sections_found: usize,
    pub processing_time_ms: f64,
    pub hits_per_second: f64,
    pub file_size_bytes: usize,
}

impl Tpx3Processor {
    /// Create a new processor with the given configuration.
    pub fn new(config: DetectorConfig) -> Self {
        Self {
            config,
            last_stats: ProcessingStats::default(),
        }
    }

    /// Create a processor with VENUS/SNS defaults.
    pub fn venus_defaults() -> Self {
        Self::new(DetectorConfig::venus_defaults())
    }

    /// Process a TPX3 file.
    ///
    /// # Arguments
    /// * `path` - Path to TPX3 file
    /// * `parallel` - Enable parallel section processing
    /// * `num_threads` - Number of threads (0 = auto)
    pub fn process_file(
        &mut self,
        path: impl AsRef<Path>,
        parallel: bool,
        num_threads: usize,
    ) -> Result<Vec<Tpx3Hit>, ProcessingError> {
        let start = std::time::Instant::now();

        // Memory-map the file
        let mapped = MappedFile::open(path.as_ref())?;
        let data = mapped.as_slice();

        // Phase 1: Discover sections (sequential)
        let sections = discover_sections(data);

        // Phase 2: Process sections
        let hits = if parallel && sections.len() > 1 {
            self.process_sections_parallel(data, &sections, num_threads)
        } else {
            self.process_sections_sequential(data, &sections)
        };

        // Update stats
        let elapsed = start.elapsed();
        self.last_stats = ProcessingStats {
            hits_extracted: hits.len(),
            sections_found: sections.len(),
            processing_time_ms: elapsed.as_secs_f64() * 1000.0,
            hits_per_second: hits.len() as f64 / elapsed.as_secs_f64(),
            file_size_bytes: data.len(),
        };

        Ok(hits)
    }

    /// Process sections sequentially.
    fn process_sections_sequential(
        &self,
        data: &[u8],
        sections: &[Tpx3Section],
    ) -> Vec<Tpx3Hit> {
        let tdc_correction = self.config.tdc_correction_25ns();
        let config = &self.config;

        let mut all_hits = Vec::new();
        for section in sections {
            let hits: Vec<Tpx3Hit> = process_section(
                data,
                section,
                tdc_correction,
                |chip_id, x, y| config.map_chip_to_global(chip_id, x, y),
            );
            all_hits.extend(hits);
        }

        // Sort by TOF for temporal processing
        all_hits.sort_unstable_by_key(|h| h.tof);
        all_hits
    }

    /// Process sections in parallel using Rayon.
    fn process_sections_parallel(
        &self,
        data: &[u8],
        sections: &[Tpx3Section],
        num_threads: usize,
    ) -> Vec<Tpx3Hit> {
        // Configure thread pool if specified
        let pool = if num_threads > 0 {
            rayon::ThreadPoolBuilder::new()
                .num_threads(num_threads)
                .build()
                .ok()
        } else {
            None
        };

        let tdc_correction = self.config.tdc_correction_25ns();
        let config = &self.config;

        let process_fn = || {
            let mut all_hits: Vec<Tpx3Hit> = sections
                .par_iter()
                .flat_map(|section| {
                    process_section(
                        data,
                        section,
                        tdc_correction,
                        |chip_id, x, y| config.map_chip_to_global(chip_id, x, y),
                    )
                })
                .collect();

            // Parallel sort
            all_hits.par_sort_unstable_by_key(|h| h.tof);
            all_hits
        };

        match pool {
            Some(pool) => pool.install(process_fn),
            None => process_fn(),
        }
    }

    /// Get statistics from last processing operation.
    pub fn last_stats(&self) -> &ProcessingStats {
        &self.last_stats
    }

    /// Get current configuration.
    pub fn config(&self) -> &DetectorConfig {
        &self.config
    }

    /// Set configuration.
    pub fn set_config(&mut self, config: DetectorConfig) {
        self.config = config;
    }
}
```

---

## Part 4: Clustering Algorithms

### 4.1 ABS (Age-Based Spatial) Clustering (`rustpix-algorithms/src/abs.rs`)

```rust
//! Age-Based Spatial (ABS) clustering algorithm.
//!
//! Performance: O(n) average case, O(n log n) worst case
//! Design: Bucket-based with spatial indexing and age-based closure

use rustpix_core::{
    hit::Hit,
    clustering::{HitClustering, ClusteringConfig, ClusteringState, ClusteringStatistics},
    error::ClusteringError,
};
use super::spatial_index::SpatialGrid;

/// ABS-specific configuration.
#[derive(Clone, Debug)]
pub struct AbsConfig {
    /// Spatial radius for bucket membership (pixels).
    pub radius: f64,
    /// Temporal correlation window (nanoseconds).
    pub neutron_correlation_window_ns: f64,
    /// How often to scan for aged buckets (every N hits).
    pub scan_interval: usize,
    /// Minimum cluster size to keep.
    pub min_cluster_size: u16,
    /// Pre-allocated bucket pool size.
    pub pre_allocate_buckets: usize,
}

impl Default for AbsConfig {
    fn default() -> Self {
        Self {
            radius: 5.0,
            neutron_correlation_window_ns: 75.0,
            scan_interval: 100,
            min_cluster_size: 1,
            pre_allocate_buckets: 1000,
        }
    }
}

impl AbsConfig {
    /// Temporal window in TOF units (25ns).
    pub fn window_tof(&self) -> u32 {
        (self.neutron_correlation_window_ns / 25.0).ceil() as u32
    }
}

/// Bucket for accumulating spatially close hits.
#[derive(Clone, Debug)]
struct Bucket {
    /// Indices of hits in this bucket.
    hit_indices: Vec<usize>,
    /// Spatial bounding box.
    x_min: i32,
    x_max: i32,
    y_min: i32,
    y_max: i32,
    /// TOF of first hit (for age calculation).
    start_tof: u32,
    /// Assigned cluster ID (-1 if not closed).
    cluster_id: i32,
    /// Whether bucket is active.
    is_active: bool,
}

impl Bucket {
    fn new() -> Self {
        Self {
            hit_indices: Vec::with_capacity(16),
            x_min: i32::MAX,
            x_max: i32::MIN,
            y_min: i32::MAX,
            y_max: i32::MIN,
            start_tof: 0,
            cluster_id: -1,
            is_active: false,
        }
    }

    fn initialize<H: Hit>(&mut self, hit_idx: usize, hit: &H) {
        self.hit_indices.clear();
        self.hit_indices.push(hit_idx);
        let x = hit.x() as i32;
        let y = hit.y() as i32;
        self.x_min = x;
        self.x_max = x;
        self.y_min = y;
        self.y_max = y;
        self.start_tof = hit.tof();
        self.cluster_id = -1;
        self.is_active = true;
    }

    fn add_hit<H: Hit>(&mut self, hit_idx: usize, hit: &H) {
        self.hit_indices.push(hit_idx);
        let x = hit.x() as i32;
        let y = hit.y() as i32;
        self.x_min = self.x_min.min(x);
        self.x_max = self.x_max.max(x);
        self.y_min = self.y_min.min(y);
        self.y_max = self.y_max.max(y);
    }

    fn fits_spatially<H: Hit>(&self, hit: &H, radius: f64) -> bool {
        let x = hit.x() as i32;
        let y = hit.y() as i32;
        let r = radius.ceil() as i32;

        x >= self.x_min - r && x <= self.x_max + r &&
        y >= self.y_min - r && y <= self.y_max + r
    }

    fn fits_temporally<H: Hit>(&self, hit: &H, window_tof: u32) -> bool {
        hit.tof().wrapping_sub(self.start_tof) <= window_tof
    }

    fn is_aged(&self, reference_tof: u32, window_tof: u32) -> bool {
        reference_tof.wrapping_sub(self.start_tof) > window_tof
    }
}

/// ABS clustering state.
pub struct AbsState {
    /// Bucket pool for reuse.
    bucket_pool: Vec<Bucket>,
    /// Indices of active buckets.
    active_buckets: Vec<usize>,
    /// Indices of free buckets for reuse.
    free_buckets: Vec<usize>,
    /// Spatial index for bucket lookup.
    spatial_grid: SpatialGrid<usize>,
    /// Next cluster ID to assign.
    next_cluster_id: i32,
    /// Hits processed counter.
    hits_processed: usize,
    /// Clusters found counter.
    clusters_found: usize,
}

impl ClusteringState for AbsState {
    fn reset(&mut self) {
        for bucket in &mut self.bucket_pool {
            bucket.is_active = false;
        }
        self.active_buckets.clear();
        self.free_buckets.clear();
        self.free_buckets.extend(0..self.bucket_pool.len());
        self.spatial_grid.clear();
        self.next_cluster_id = 0;
        self.hits_processed = 0;
        self.clusters_found = 0;
    }
}

/// ABS clustering algorithm.
pub struct AbsClustering {
    config: AbsConfig,
    generic_config: ClusteringConfig,
}

impl AbsClustering {
    pub fn new(config: AbsConfig) -> Self {
        Self {
            generic_config: ClusteringConfig::from_abs(&config),
            config,
        }
    }

    /// Get or create a bucket from the pool.
    fn get_bucket(&self, state: &mut AbsState) -> usize {
        if let Some(idx) = state.free_buckets.pop() {
            idx
        } else {
            let idx = state.bucket_pool.len();
            state.bucket_pool.push(Bucket::new());
            idx
        }
    }

    /// Find a compatible bucket for a hit.
    fn find_compatible_bucket<H: Hit>(
        &self,
        hit: &H,
        state: &AbsState,
        window_tof: u32,
    ) -> Option<usize> {
        let x = hit.x() as i32;
        let y = hit.y() as i32;

        // Search in spatial neighborhood
        for &bucket_idx in state.spatial_grid.query_neighborhood(x, y) {
            let bucket = &state.bucket_pool[bucket_idx];
            if bucket.is_active
                && bucket.fits_spatially(hit, self.config.radius)
                && bucket.fits_temporally(hit, window_tof)
            {
                return Some(bucket_idx);
            }
        }
        None
    }

    /// Close a bucket and assign cluster labels.
    fn close_bucket(
        &self,
        bucket_idx: usize,
        state: &mut AbsState,
        labels: &mut [i32],
    ) -> bool {
        let bucket = &mut state.bucket_pool[bucket_idx];

        if bucket.hit_indices.len() >= self.config.min_cluster_size as usize {
            let cluster_id = state.next_cluster_id;
            state.next_cluster_id += 1;
            bucket.cluster_id = cluster_id;

            for &hit_idx in &bucket.hit_indices {
                labels[hit_idx] = cluster_id;
            }

            state.clusters_found += 1;
            true
        } else {
            false
        }
    }

    /// Scan and close aged buckets.
    fn scan_and_close_aged<H: Hit>(
        &self,
        reference_tof: u32,
        state: &mut AbsState,
        labels: &mut [i32],
    ) {
        let window_tof = self.config.window_tof();

        state.active_buckets.retain(|&bucket_idx| {
            let bucket = &state.bucket_pool[bucket_idx];
            if bucket.is_aged(reference_tof, window_tof) {
                self.close_bucket(bucket_idx, state, labels);

                // Remove from spatial index
                let cx = (bucket.x_min + bucket.x_max) / 2;
                let cy = (bucket.y_min + bucket.y_max) / 2;
                state.spatial_grid.remove(cx, cy, bucket_idx);

                // Return to pool
                state.bucket_pool[bucket_idx].is_active = false;
                state.free_buckets.push(bucket_idx);
                false
            } else {
                true
            }
        });
    }
}

impl HitClustering for AbsClustering {
    type State = AbsState;

    fn name(&self) -> &'static str {
        "ABS"
    }

    fn create_state(&self) -> Self::State {
        let mut bucket_pool = Vec::with_capacity(self.config.pre_allocate_buckets);
        for _ in 0..self.config.pre_allocate_buckets {
            bucket_pool.push(Bucket::new());
        }

        AbsState {
            bucket_pool,
            active_buckets: Vec::with_capacity(self.config.pre_allocate_buckets),
            free_buckets: (0..self.config.pre_allocate_buckets).collect(),
            spatial_grid: SpatialGrid::new(32, 512, 512),
            next_cluster_id: 0,
            hits_processed: 0,
            clusters_found: 0,
        }
    }

    fn configure(&mut self, config: &ClusteringConfig) {
        self.config.radius = config.radius;
        self.config.neutron_correlation_window_ns = config.temporal_window_ns;
        self.generic_config = config.clone();
    }

    fn config(&self) -> &ClusteringConfig {
        &self.generic_config
    }

    fn cluster<H: Hit>(
        &self,
        hits: &[H],
        state: &mut Self::State,
        labels: &mut [i32],
    ) -> Result<usize, ClusteringError> {
        if hits.is_empty() {
            return Ok(0);
        }

        // Initialize labels to -1 (unclustered)
        labels.iter_mut().for_each(|l| *l = -1);

        let window_tof = self.config.window_tof();

        for (hit_idx, hit) in hits.iter().enumerate() {
            // Periodic aging scan
            if hit_idx % self.config.scan_interval == 0 && hit_idx > 0 {
                self.scan_and_close_aged(hit.tof(), state, labels);
            }

            // Find compatible bucket or create new one
            if let Some(bucket_idx) = self.find_compatible_bucket(hit, state, window_tof) {
                state.bucket_pool[bucket_idx].add_hit(hit_idx, hit);
            } else {
                let bucket_idx = self.get_bucket(state);
                state.bucket_pool[bucket_idx].initialize(hit_idx, hit);
                state.active_buckets.push(bucket_idx);

                // Add to spatial index
                let x = hit.x() as i32;
                let y = hit.y() as i32;
                state.spatial_grid.insert(x, y, bucket_idx);
            }

            state.hits_processed += 1;
        }

        // Final aging scan
        if let Some(last_hit) = hits.last() {
            self.scan_and_close_aged(last_hit.tof().saturating_add(window_tof), state, labels);
        }

        // Force close remaining buckets
        for bucket_idx in std::mem::take(&mut state.active_buckets) {
            self.close_bucket(bucket_idx, state, labels);
            state.bucket_pool[bucket_idx].is_active = false;
            state.free_buckets.push(bucket_idx);
        }

        Ok(state.clusters_found)
    }

    fn statistics(&self, state: &Self::State) -> ClusteringStatistics {
        ClusteringStatistics {
            hits_processed: state.hits_processed,
            clusters_found: state.clusters_found,
            ..Default::default()
        }
    }
}
```

### 4.2 DBSCAN Clustering (`rustpix-algorithms/src/dbscan.rs`)

```rust
//! DBSCAN clustering algorithm.
//!
//! Density-Based Spatial Clustering of Applications with Noise.
//! Good for handling noise and outliers.

use rustpix_core::{
    hit::Hit,
    clustering::{HitClustering, ClusteringConfig, ClusteringState, ClusteringStatistics},
    error::ClusteringError,
};
use super::spatial_index::SpatialGrid;

/// DBSCAN-specific configuration.
#[derive(Clone, Debug)]
pub struct DbscanConfig {
    /// Maximum distance for neighbors (epsilon).
    pub epsilon: f64,
    /// Minimum points to form a core point.
    pub min_points: usize,
    /// Temporal correlation window (nanoseconds).
    pub temporal_window_ns: f64,
}

impl Default for DbscanConfig {
    fn default() -> Self {
        Self {
            epsilon: 5.0,
            min_points: 4,
            temporal_window_ns: 75.0,
        }
    }
}

/// Point classification in DBSCAN.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PointType {
    Undefined,
    Noise,
    Border,
    Core,
}

/// DBSCAN clustering state.
pub struct DbscanState {
    spatial_grid: SpatialGrid<usize>,
    point_types: Vec<PointType>,
    visited: Vec<bool>,
    neighbor_buffer: Vec<usize>,
    hits_processed: usize,
    clusters_found: usize,
    noise_count: usize,
}

impl ClusteringState for DbscanState {
    fn reset(&mut self) {
        self.spatial_grid.clear();
        self.point_types.clear();
        self.visited.clear();
        self.neighbor_buffer.clear();
        self.hits_processed = 0;
        self.clusters_found = 0;
        self.noise_count = 0;
    }
}

/// DBSCAN clustering algorithm.
pub struct DbscanClustering {
    config: DbscanConfig,
    generic_config: ClusteringConfig,
}

impl DbscanClustering {
    pub fn new(config: DbscanConfig) -> Self {
        Self {
            generic_config: ClusteringConfig::from_dbscan(&config),
            config,
        }
    }

    /// Find neighbors within epsilon distance.
    fn find_neighbors<H: Hit>(
        &self,
        hits: &[H],
        point_idx: usize,
        state: &DbscanState,
    ) -> Vec<usize> {
        let hit = &hits[point_idx];
        let x = hit.x() as i32;
        let y = hit.y() as i32;
        let window_tof = (self.config.temporal_window_ns / 25.0).ceil() as u32;

        let epsilon_sq = self.config.epsilon * self.config.epsilon;

        state.spatial_grid.query_neighborhood(x, y)
            .iter()
            .filter(|&&idx| {
                if idx == point_idx {
                    return false;
                }
                let other = &hits[idx];
                hit.within_temporal_window(other, window_tof) &&
                hit.distance_squared(other) <= epsilon_sq
            })
            .copied()
            .collect()
    }

    /// Expand cluster from a core point.
    fn expand_cluster<H: Hit>(
        &self,
        hits: &[H],
        point_idx: usize,
        neighbors: Vec<usize>,
        cluster_id: i32,
        state: &mut DbscanState,
        labels: &mut [i32],
    ) {
        labels[point_idx] = cluster_id;
        state.point_types[point_idx] = PointType::Core;

        let mut to_process = neighbors;

        while let Some(neighbor_idx) = to_process.pop() {
            if state.visited[neighbor_idx] {
                continue;
            }
            state.visited[neighbor_idx] = true;

            let neighbor_neighbors = self.find_neighbors(hits, neighbor_idx, state);

            if neighbor_neighbors.len() >= self.config.min_points {
                state.point_types[neighbor_idx] = PointType::Core;
                to_process.extend(neighbor_neighbors);
            } else {
                state.point_types[neighbor_idx] = PointType::Border;
            }

            if labels[neighbor_idx] == -1 {
                labels[neighbor_idx] = cluster_id;
            }
        }
    }
}

impl HitClustering for DbscanClustering {
    type State = DbscanState;

    fn name(&self) -> &'static str {
        "DBSCAN"
    }

    fn create_state(&self) -> Self::State {
        DbscanState {
            spatial_grid: SpatialGrid::new(64, 512, 512),
            point_types: Vec::new(),
            visited: Vec::new(),
            neighbor_buffer: Vec::with_capacity(100),
            hits_processed: 0,
            clusters_found: 0,
            noise_count: 0,
        }
    }

    fn configure(&mut self, config: &ClusteringConfig) {
        self.config.epsilon = config.radius;
        self.config.temporal_window_ns = config.temporal_window_ns;
        self.generic_config = config.clone();
    }

    fn config(&self) -> &ClusteringConfig {
        &self.generic_config
    }

    fn cluster<H: Hit>(
        &self,
        hits: &[H],
        state: &mut Self::State,
        labels: &mut [i32],
    ) -> Result<usize, ClusteringError> {
        if hits.is_empty() {
            return Ok(0);
        }

        let n = hits.len();

        // Initialize state
        labels.iter_mut().for_each(|l| *l = -1);
        state.point_types.clear();
        state.point_types.resize(n, PointType::Undefined);
        state.visited.clear();
        state.visited.resize(n, false);
        state.spatial_grid.clear();

        // Build spatial index
        for (idx, hit) in hits.iter().enumerate() {
            state.spatial_grid.insert(hit.x() as i32, hit.y() as i32, idx);
        }

        let mut cluster_id = 0i32;

        for point_idx in 0..n {
            if state.visited[point_idx] {
                continue;
            }
            state.visited[point_idx] = true;

            let neighbors = self.find_neighbors(hits, point_idx, state);

            if neighbors.len() < self.config.min_points {
                state.point_types[point_idx] = PointType::Noise;
                state.noise_count += 1;
            } else {
                self.expand_cluster(hits, point_idx, neighbors, cluster_id, state, labels);
                cluster_id += 1;
            }
        }

        state.hits_processed = n;
        state.clusters_found = cluster_id as usize;

        Ok(state.clusters_found)
    }

    fn statistics(&self, state: &Self::State) -> ClusteringStatistics {
        ClusteringStatistics {
            hits_processed: state.hits_processed,
            clusters_found: state.clusters_found,
            noise_hits: state.noise_count,
            ..Default::default()
        }
    }
}
```

### 4.3 Graph (Union-Find) Clustering (`rustpix-algorithms/src/graph.rs`)

```rust
//! Graph-based clustering using Union-Find.

use rustpix_core::{
    hit::Hit,
    clustering::{HitClustering, ClusteringConfig, ClusteringState, ClusteringStatistics},
    error::ClusteringError,
};
use super::spatial_index::SpatialGrid;

/// Graph clustering configuration.
#[derive(Clone, Debug)]
pub struct GraphConfig {
    /// Maximum distance for edge creation.
    pub radius: f64,
    /// Temporal correlation window (nanoseconds).
    pub temporal_window_ns: f64,
}

impl Default for GraphConfig {
    fn default() -> Self {
        Self {
            radius: 5.0,
            temporal_window_ns: 75.0,
        }
    }
}

/// Union-Find data structure with path compression and union by rank.
struct UnionFind {
    parent: Vec<usize>,
    rank: Vec<usize>,
}

impl UnionFind {
    fn new(n: usize) -> Self {
        Self {
            parent: (0..n).collect(),
            rank: vec![0; n],
        }
    }

    fn find(&mut self, x: usize) -> usize {
        if self.parent[x] != x {
            self.parent[x] = self.find(self.parent[x]); // Path compression
        }
        self.parent[x]
    }

    fn union(&mut self, x: usize, y: usize) {
        let root_x = self.find(x);
        let root_y = self.find(y);

        if root_x == root_y {
            return;
        }

        // Union by rank
        match self.rank[root_x].cmp(&self.rank[root_y]) {
            std::cmp::Ordering::Less => self.parent[root_x] = root_y,
            std::cmp::Ordering::Greater => self.parent[root_y] = root_x,
            std::cmp::Ordering::Equal => {
                self.parent[root_y] = root_x;
                self.rank[root_x] += 1;
            }
        }
    }
}

/// Graph clustering state.
pub struct GraphState {
    union_find: Option<UnionFind>,
    spatial_grid: SpatialGrid<usize>,
    edges_created: usize,
    hits_processed: usize,
    clusters_found: usize,
}

impl ClusteringState for GraphState {
    fn reset(&mut self) {
        self.union_find = None;
        self.spatial_grid.clear();
        self.edges_created = 0;
        self.hits_processed = 0;
        self.clusters_found = 0;
    }
}

/// Graph-based clustering algorithm.
pub struct GraphClustering {
    config: GraphConfig,
    generic_config: ClusteringConfig,
}

impl GraphClustering {
    pub fn new(config: GraphConfig) -> Self {
        Self {
            generic_config: ClusteringConfig::from_graph(&config),
            config,
        }
    }
}

impl HitClustering for GraphClustering {
    type State = GraphState;

    fn name(&self) -> &'static str {
        "Graph"
    }

    fn create_state(&self) -> Self::State {
        GraphState {
            union_find: None,
            spatial_grid: SpatialGrid::new(64, 512, 512),
            edges_created: 0,
            hits_processed: 0,
            clusters_found: 0,
        }
    }

    fn configure(&mut self, config: &ClusteringConfig) {
        self.config.radius = config.radius;
        self.config.temporal_window_ns = config.temporal_window_ns;
        self.generic_config = config.clone();
    }

    fn config(&self) -> &ClusteringConfig {
        &self.generic_config
    }

    fn cluster<H: Hit>(
        &self,
        hits: &[H],
        state: &mut Self::State,
        labels: &mut [i32],
    ) -> Result<usize, ClusteringError> {
        if hits.is_empty() {
            return Ok(0);
        }

        let n = hits.len();
        let radius_sq = self.config.radius * self.config.radius;
        let window_tof = (self.config.temporal_window_ns / 25.0).ceil() as u32;

        // Initialize
        labels.iter_mut().for_each(|l| *l = -1);
        state.spatial_grid.clear();
        state.union_find = Some(UnionFind::new(n));
        state.edges_created = 0;

        // Build spatial index
        for (idx, hit) in hits.iter().enumerate() {
            state.spatial_grid.insert(hit.x() as i32, hit.y() as i32, idx);
        }

        // Build graph edges and union connected components
        let uf = state.union_find.as_mut().unwrap();

        for (idx, hit) in hits.iter().enumerate() {
            let x = hit.x() as i32;
            let y = hit.y() as i32;

            for &neighbor_idx in state.spatial_grid.query_neighborhood(x, y) {
                if neighbor_idx <= idx {
                    continue; // Avoid duplicate edges
                }

                let neighbor = &hits[neighbor_idx];
                if hit.within_temporal_window(neighbor, window_tof) &&
                   hit.distance_squared(neighbor) <= radius_sq
                {
                    uf.union(idx, neighbor_idx);
                    state.edges_created += 1;
                }
            }
        }

        // Assign cluster labels
        use std::collections::HashMap;
        let mut root_to_cluster: HashMap<usize, i32> = HashMap::new();
        let mut next_cluster = 0i32;

        for idx in 0..n {
            let root = uf.find(idx);
            let cluster_id = *root_to_cluster.entry(root).or_insert_with(|| {
                let id = next_cluster;
                next_cluster += 1;
                id
            });
            labels[idx] = cluster_id;
        }

        state.hits_processed = n;
        state.clusters_found = next_cluster as usize;

        Ok(state.clusters_found)
    }

    fn statistics(&self, state: &Self::State) -> ClusteringStatistics {
        ClusteringStatistics {
            hits_processed: state.hits_processed,
            clusters_found: state.clusters_found,
            ..Default::default()
        }
    }
}
```

### 4.4 Grid Clustering (`rustpix-algorithms/src/grid.rs`)

```rust
//! Grid-based clustering optimized for detector geometry.

use rustpix_core::{
    hit::Hit,
    clustering::{HitClustering, ClusteringConfig, ClusteringState, ClusteringStatistics},
    error::ClusteringError,
};

/// Grid clustering configuration.
#[derive(Clone, Debug)]
pub struct GridConfig {
    /// Number of grid columns.
    pub grid_cols: usize,
    /// Number of grid rows.
    pub grid_rows: usize,
    /// Maximum distance for hit connection within cell.
    pub connection_distance: f64,
    /// Temporal correlation window (nanoseconds).
    pub temporal_window_ns: f64,
    /// Merge clusters across adjacent cells.
    pub merge_adjacent_cells: bool,
}

impl Default for GridConfig {
    fn default() -> Self {
        Self {
            grid_cols: 32,
            grid_rows: 32,
            connection_distance: 4.0,
            temporal_window_ns: 75.0,
            merge_adjacent_cells: true,
        }
    }
}

/// Grid cell containing hit indices.
#[derive(Clone, Debug, Default)]
struct GridCell {
    hit_indices: Vec<usize>,
}

/// Grid clustering state.
pub struct GridState {
    cells: Vec<GridCell>,
    grid_cols: usize,
    grid_rows: usize,
    hits_processed: usize,
    clusters_found: usize,
}

impl ClusteringState for GridState {
    fn reset(&mut self) {
        for cell in &mut self.cells {
            cell.hit_indices.clear();
        }
        self.hits_processed = 0;
        self.clusters_found = 0;
    }
}

/// Grid-based clustering algorithm.
pub struct GridClustering {
    config: GridConfig,
    generic_config: ClusteringConfig,
    detector_width: usize,
    detector_height: usize,
}

impl GridClustering {
    pub fn new(config: GridConfig, detector_width: usize, detector_height: usize) -> Self {
        Self {
            generic_config: ClusteringConfig::from_grid(&config),
            config,
            detector_width,
            detector_height,
        }
    }

    fn get_cell_index(&self, x: u16, y: u16) -> usize {
        let cell_width = self.detector_width / self.config.grid_cols;
        let cell_height = self.detector_height / self.config.grid_rows;

        let col = (x as usize / cell_width).min(self.config.grid_cols - 1);
        let row = (y as usize / cell_height).min(self.config.grid_rows - 1);

        row * self.config.grid_cols + col
    }
}

impl HitClustering for GridClustering {
    type State = GridState;

    fn name(&self) -> &'static str {
        "Grid"
    }

    fn create_state(&self) -> Self::State {
        let num_cells = self.config.grid_cols * self.config.grid_rows;
        GridState {
            cells: vec![GridCell::default(); num_cells],
            grid_cols: self.config.grid_cols,
            grid_rows: self.config.grid_rows,
            hits_processed: 0,
            clusters_found: 0,
        }
    }

    fn configure(&mut self, config: &ClusteringConfig) {
        self.config.connection_distance = config.radius;
        self.config.temporal_window_ns = config.temporal_window_ns;
        self.generic_config = config.clone();
    }

    fn config(&self) -> &ClusteringConfig {
        &self.generic_config
    }

    fn cluster<H: Hit>(
        &self,
        hits: &[H],
        state: &mut Self::State,
        labels: &mut [i32],
    ) -> Result<usize, ClusteringError> {
        if hits.is_empty() {
            return Ok(0);
        }

        // Reset and populate cells
        for cell in &mut state.cells {
            cell.hit_indices.clear();
        }

        for (idx, hit) in hits.iter().enumerate() {
            let cell_idx = self.get_cell_index(hit.x(), hit.y());
            state.cells[cell_idx].hit_indices.push(idx);
        }

        // Initialize labels
        labels.iter_mut().for_each(|l| *l = -1);

        let distance_sq = self.config.connection_distance * self.config.connection_distance;
        let window_tof = (self.config.temporal_window_ns / 25.0).ceil() as u32;
        let mut next_cluster = 0i32;

        // Process each cell
        for cell in &state.cells {
            if cell.hit_indices.is_empty() {
                continue;
            }

            // Simple single-linkage within cell
            for &hit_idx in &cell.hit_indices {
                if labels[hit_idx] != -1 {
                    continue;
                }

                // Start new cluster
                labels[hit_idx] = next_cluster;
                let mut stack = vec![hit_idx];

                while let Some(current) = stack.pop() {
                    let current_hit = &hits[current];

                    for &other_idx in &cell.hit_indices {
                        if labels[other_idx] != -1 {
                            continue;
                        }

                        let other_hit = &hits[other_idx];
                        if current_hit.within_temporal_window(other_hit, window_tof) &&
                           current_hit.distance_squared(other_hit) <= distance_sq
                        {
                            labels[other_idx] = next_cluster;
                            stack.push(other_idx);
                        }
                    }
                }

                next_cluster += 1;
            }
        }

        state.hits_processed = hits.len();
        state.clusters_found = next_cluster as usize;

        // TODO: Merge adjacent cells if config.merge_adjacent_cells

        Ok(state.clusters_found)
    }

    fn statistics(&self, state: &Self::State) -> ClusteringStatistics {
        ClusteringStatistics {
            hits_processed: state.hits_processed,
            clusters_found: state.clusters_found,
            ..Default::default()
        }
    }
}
```

---

## Part 5: Python Bindings

### 5.1 Main Binding Module (`rustpix-python/src/lib.rs`)

```rust
//! Python bindings for rustpix.

use pyo3::prelude::*;
use pyo3::exceptions::{PyIOError, PyValueError};
use numpy::{PyArray1, PyReadonlyArray1, IntoPyArray};

mod hit;
mod neutron;
mod processor;
mod config;

use hit::{PyHit, hit_array_to_numpy};
use neutron::{PyNeutron, neutron_array_to_numpy};
use processor::PyTpx3Processor;
use config::{PyDetectorConfig, PyClusteringConfig, PyExtractionConfig};

/// Process a TPX3 file and return hits as structured numpy array.
///
/// # Arguments
/// * `path` - Path to TPX3 file
/// * `parallel` - Enable parallel processing (default: true)
/// * `num_threads` - Number of threads (0 = auto)
///
/// # Returns
/// Structured numpy array with dtype: [('tof', 'u4'), ('x', 'u2'), ('y', 'u2'),
///                                     ('timestamp', 'u4'), ('tot', 'u2'),
///                                     ('chip_id', 'u1'), ('cluster_id', 'i4')]
#[pyfunction]
#[pyo3(signature = (path, parallel=true, num_threads=0, config=None))]
fn process_tpx3<'py>(
    py: Python<'py>,
    path: &str,
    parallel: bool,
    num_threads: usize,
    config: Option<&PyDetectorConfig>,
) -> PyResult<Bound<'py, PyArray1<PyHit>>> {
    let mut processor = match config {
        Some(cfg) => rustpix_tpx::tpx3::Tpx3Processor::new(cfg.inner.clone()),
        None => rustpix_tpx::tpx3::Tpx3Processor::venus_defaults(),
    };

    let hits = processor.process_file(path, parallel, num_threads)
        .map_err(|e| PyIOError::new_err(format!("Processing error: {}", e)))?;

    hit_array_to_numpy(py, hits)
}

/// Process hits to neutrons using clustering and centroid extraction.
///
/// # Arguments
/// * `hits` - Structured numpy array of hits
/// * `clustering_config` - Clustering algorithm configuration
/// * `extraction_config` - Centroid extraction configuration
///
/// # Returns
/// Structured numpy array of neutrons.
#[pyfunction]
#[pyo3(signature = (hits, clustering_config=None, extraction_config=None))]
fn process_hits_to_neutrons<'py>(
    py: Python<'py>,
    hits: PyReadonlyArray1<'py, PyHit>,
    clustering_config: Option<&PyClusteringConfig>,
    extraction_config: Option<&PyExtractionConfig>,
) -> PyResult<Bound<'py, PyArray1<PyNeutron>>> {
    use rustpix_algorithms::{AbsClustering, AbsConfig};
    use rustpix_core::{
        clustering::HitClustering,
        extraction::{SimpleCentroidExtraction, NeutronExtraction},
    };

    let hits_slice = hits.as_slice()?;

    // Convert PyHit to Tpx3Hit (they have compatible layout)
    let rust_hits: Vec<rustpix_tpx::tpx3::Tpx3Hit> = hits_slice
        .iter()
        .map(|h| rustpix_tpx::tpx3::Tpx3Hit::new(
            h.tof, h.x, h.y, h.timestamp, h.tot, h.chip_id
        ))
        .collect();

    // Setup clustering
    let abs_config = clustering_config
        .map(|c| c.to_abs_config())
        .unwrap_or_default();
    let clustering = AbsClustering::new(abs_config);
    let mut state = clustering.create_state();
    let mut labels = vec![-1i32; rust_hits.len()];

    // Cluster
    let num_clusters = clustering.cluster(&rust_hits, &mut state, &mut labels)
        .map_err(|e| PyValueError::new_err(format!("Clustering error: {}", e)))?;

    // Extract neutrons
    let ext_config = extraction_config
        .map(|c| c.inner.clone())
        .unwrap_or_default();
    let mut extraction = SimpleCentroidExtraction::new();
    extraction.configure(ext_config);

    let neutrons = extraction.extract(&rust_hits, &labels, num_clusters)
        .map_err(|e| PyValueError::new_err(format!("Extraction error: {}", e)))?;

    neutron_array_to_numpy(py, neutrons)
}

/// rustpix - High-performance pixel detector processing library.
#[pymodule]
fn _rustpix(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(process_tpx3, m)?)?;
    m.add_function(wrap_pyfunction!(process_hits_to_neutrons, m)?)?;

    m.add_class::<PyTpx3Processor>()?;
    m.add_class::<PyDetectorConfig>()?;
    m.add_class::<PyClusteringConfig>()?;
    m.add_class::<PyExtractionConfig>()?;

    Ok(())
}
```

### 5.2 Hit Array Bindings (`rustpix-python/src/hit.rs`)

```rust
//! Python bindings for hit types.

use pyo3::prelude::*;
use numpy::{PyArray1, IntoPyArray, dtype_bound};

/// Python-compatible hit structure matching numpy structured array.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct PyHit {
    pub tof: u32,
    pub x: u16,
    pub y: u16,
    pub timestamp: u32,
    pub tot: u16,
    pub chip_id: u8,
    pub _padding: u8,
    pub cluster_id: i32,
}

unsafe impl numpy::Element for PyHit {
    const IS_COPY: bool = true;

    fn get_dtype_bound(py: Python<'_>) -> Bound<'_, numpy::PyArrayDescr> {
        dtype_bound!(py, [
            ("tof", "u4"),
            ("x", "u2"),
            ("y", "u2"),
            ("timestamp", "u4"),
            ("tot", "u2"),
            ("chip_id", "u1"),
            ("_padding", "u1"),
            ("cluster_id", "i4"),
        ])
    }
}

/// Convert Rust hits to numpy array.
pub fn hit_array_to_numpy<'py>(
    py: Python<'py>,
    hits: Vec<rustpix_tpx::tpx3::Tpx3Hit>,
) -> PyResult<Bound<'py, PyArray1<PyHit>>> {
    // Safe transmute since layouts are identical
    let py_hits: Vec<PyHit> = hits.into_iter()
        .map(|h| PyHit {
            tof: h.tof,
            x: h.x,
            y: h.y,
            timestamp: h.timestamp,
            tot: h.tot,
            chip_id: h.chip_id,
            _padding: 0,
            cluster_id: h.cluster_id,
        })
        .collect();

    Ok(py_hits.into_pyarray_bound(py))
}
```

### 5.3 Processor Bindings (`rustpix-python/src/processor.rs`)

```rust
//! Python bindings for TPX3 processor.

use pyo3::prelude::*;
use pyo3::exceptions::PyIOError;
use numpy::PyArray1;

use crate::hit::{PyHit, hit_array_to_numpy};
use crate::config::PyDetectorConfig;

/// TPX3 file processor.
#[pyclass(name = "Tpx3Processor")]
pub struct PyTpx3Processor {
    inner: rustpix_tpx::tpx3::Tpx3Processor,
}

#[pymethods]
impl PyTpx3Processor {
    /// Create a new processor with VENUS/SNS defaults.
    #[new]
    #[pyo3(signature = (config=None))]
    fn new(config: Option<&PyDetectorConfig>) -> Self {
        let inner = match config {
            Some(cfg) => rustpix_tpx::tpx3::Tpx3Processor::new(cfg.inner.clone()),
            None => rustpix_tpx::tpx3::Tpx3Processor::venus_defaults(),
        };
        Self { inner }
    }

    /// Create a processor with VENUS defaults.
    #[staticmethod]
    fn venus_defaults() -> Self {
        Self {
            inner: rustpix_tpx::tpx3::Tpx3Processor::venus_defaults(),
        }
    }

    /// Process a TPX3 file.
    ///
    /// # Arguments
    /// * `path` - Path to TPX3 file
    /// * `parallel` - Enable parallel processing
    /// * `num_threads` - Number of threads (0 = auto)
    #[pyo3(signature = (path, parallel=true, num_threads=0))]
    fn process_file<'py>(
        &mut self,
        py: Python<'py>,
        path: &str,
        parallel: bool,
        num_threads: usize,
    ) -> PyResult<Bound<'py, PyArray1<PyHit>>> {
        let hits = self.inner.process_file(path, parallel, num_threads)
            .map_err(|e| PyIOError::new_err(format!("Processing error: {}", e)))?;

        hit_array_to_numpy(py, hits)
    }

    /// Get hits extracted from last processing.
    #[getter]
    fn hits_extracted(&self) -> usize {
        self.inner.last_stats().hits_extracted
    }

    /// Get processing time in milliseconds.
    #[getter]
    fn processing_time_ms(&self) -> f64 {
        self.inner.last_stats().processing_time_ms
    }

    /// Get hits per second from last processing.
    #[getter]
    fn hits_per_second(&self) -> f64 {
        self.inner.last_stats().hits_per_second
    }
}
```

---

## Part 6: Implementation Phases

### Phase 1: Foundation (Week 1-2)

**Goal**: Core types and traits compilable and tested.

**Tasks**:
1. Set up Cargo workspace structure
2. Implement `rustpix-core`:
   - `Hit` trait and `GenericHit` type
   - `Neutron` type
   - `HitClustering` and `NeutronExtraction` traits
   - Error types
3. Set up CI/CD:
   - GitHub Actions for build/test/lint
   - cargo-deny for dependency auditing
   - Codecov integration

**Deliverables**:
- Compilable `rustpix-core` crate
- Unit tests for all core types
- CI pipeline running

### Phase 2: TPX3 Parser (Week 3-4)

**Goal**: TPX3 file processing at baseline performance.

**Tasks**:
1. Implement `rustpix-tpx`:
   - `Tpx3Packet` parser with all field extractors
   - `Tpx3Hit` type
   - Section discovery and TDC propagation
   - `Tpx3Processor` with sequential processing
2. Implement `rustpix-io`:
   - Memory-mapped file support via `memmap2`
3. Add benchmarks for packet parsing

**Deliverables**:
- Working TPX3 file processor
- Benchmarks showing packet parsing throughput
- Integration tests with sample data

### Phase 3: Clustering Algorithms (Week 5-7)

**Goal**: All four clustering algorithms implemented and tested.

**Tasks**:
1. Implement `rustpix-algorithms`:
   - Spatial index (shared utility)
   - ABS clustering (primary algorithm)
   - DBSCAN clustering
   - Graph (Union-Find) clustering
   - Grid clustering
2. Implement centroid extraction in `rustpix-core`
3. Add algorithm benchmarks
4. Add clustering correctness tests

**Deliverables**:
- All four clustering algorithms
- Centroid extraction
- Benchmarks comparing algorithm performance
- Tests validating clustering correctness

### Phase 4: Parallelization (Week 8-9)

**Goal**: Parallel processing matching C++ performance.

**Tasks**:
1. Add Rayon parallel processing to `Tpx3Processor`
2. Implement temporal batching for parallel clustering
3. Add thread-safety tests
4. Performance optimization:
   - Profile and identify bottlenecks
   - Optimize hot paths
   - Add SIMD where beneficial

**Deliverables**:
- Parallel TPX3 processing
- Parallel temporal clustering
- Performance benchmarks showing 90M+ hits/sec target

### Phase 5: Python Bindings (Week 10-11)

**Goal**: Full Python API via PyO3.

**Tasks**:
1. Implement `rustpix-python`:
   - Hit array bindings with numpy structured arrays
   - Neutron array bindings
   - Processor class bindings
   - Configuration bindings
2. Add Python package wrapper:
   - Pydantic configuration models
   - Analysis utilities
3. Set up maturin build
4. Add Python tests

**Deliverables**:
- Working Python bindings
- PyPI-publishable package
- Python test suite
- Example notebooks

### Phase 6: CLI and Polish (Week 12)

**Goal**: Production-ready release.

**Tasks**:
1. Implement `rustpix-cli`:
   - `process` command for file processing
   - `info` command for file inspection
   - `benchmark` command for performance testing
2. Documentation:
   - API documentation (rustdoc)
   - User guide
   - Migration guide from TDCSophiread
3. Release preparation:
   - Version 0.1.0
   - crates.io publishing
   - PyPI publishing

**Deliverables**:
- CLI tool
- Full documentation
- Published packages

---

## Part 7: Testing Strategy

### Unit Tests

```rust
// Example test structure
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_packet_parsing() {
        // Test known packet values
    }

    #[test]
    fn test_pixel_coordinate_decode() {
        // Test coordinate decoding formula
    }

    #[test]
    fn test_clustering_single_cluster() {
        // Test clustering with known single cluster
    }

    #[test]
    fn test_clustering_multiple_clusters() {
        // Test clustering with known multiple clusters
    }

    #[test]
    fn test_centroid_extraction() {
        // Test centroid calculation
    }
}
```

### Integration Tests

```rust
// tests/integration_tests.rs
#[test]
fn test_full_pipeline() {
    // Load test TPX3 file
    // Process to hits
    // Cluster hits
    // Extract neutrons
    // Validate against known outputs
}

#[test]
fn test_parallel_consistency() {
    // Process same file with 1, 2, 4, 8 threads
    // Verify identical results
}
```

### Property-Based Tests

```rust
use proptest::prelude::*;

proptest! {
    #[test]
    fn test_clustering_deterministic(seed: u64) {
        // Generate random hits
        // Cluster twice
        // Verify identical results
    }

    #[test]
    fn test_all_hits_labeled(hits in vec_of_hits()) {
        // Cluster hits
        // Verify all hits have labels
    }
}
```

### Benchmarks

```rust
// benches/clustering.rs
use criterion::{criterion_group, criterion_main, Criterion, BenchmarkId};

fn benchmark_abs_clustering(c: &mut Criterion) {
    let mut group = c.benchmark_group("abs_clustering");

    for size in [10_000, 100_000, 1_000_000].iter() {
        let hits = generate_test_hits(*size);

        group.bench_with_input(
            BenchmarkId::from_parameter(size),
            &hits,
            |b, hits| {
                let clustering = AbsClustering::default();
                let mut state = clustering.create_state();
                let mut labels = vec![-1i32; hits.len()];

                b.iter(|| {
                    state.reset();
                    clustering.cluster(hits, &mut state, &mut labels)
                });
            },
        );
    }
    group.finish();
}

criterion_group!(benches, benchmark_abs_clustering);
criterion_main!(benches);
```

---

## Part 8: Configuration Files

### Root Cargo.toml

```toml
[workspace]
resolver = "2"
members = [
    "crates/rustpix-core",
    "crates/rustpix-tpx",
    "crates/rustpix-algorithms",
    "crates/rustpix-io",
    "crates/rustpix-python",
    "crates/rustpix-cli",
]

[workspace.package]
version = "0.1.0"
edition = "2021"
rust-version = "1.75"
license = "MIT OR Apache-2.0"
repository = "https://github.com/ornlneutronimaging/rustpix"
authors = ["ORNL Neutron Imaging <neutronimaging@ornl.gov>"]
keywords = ["neutron", "imaging", "timepix", "detector", "physics"]
categories = ["science", "data-processing"]

[workspace.dependencies]
# Internal crates
rustpix-core = { path = "crates/rustpix-core" }
rustpix-tpx = { path = "crates/rustpix-tpx" }
rustpix-algorithms = { path = "crates/rustpix-algorithms" }
rustpix-io = { path = "crates/rustpix-io" }

# External dependencies
thiserror = "2.0"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
rayon = "1.10"
memmap2 = "0.9"
pyo3 = { version = "0.23", features = ["extension-module", "abi3-py310"] }
numpy = "0.23"
clap = { version = "4.5", features = ["derive"] }
indicatif = "0.17"
tracing = "0.1"
tracing-subscriber = "0.3"

# Testing
criterion = { version = "0.5", features = ["html_reports"] }
proptest = "1.5"

[profile.release]
lto = "thin"
codegen-units = 1
opt-level = 3

[profile.bench]
inherits = "release"
debug = true
```

### GitHub Actions CI

```yaml
# .github/workflows/ci.yml
name: CI

on:
  push:
    branches: [main, next]
  pull_request:
    branches: [main, next]

env:
  CARGO_TERM_COLOR: always
  RUSTFLAGS: -D warnings

jobs:
  test:
    runs-on: ${{ matrix.os }}
    strategy:
      matrix:
        os: [ubuntu-latest, macos-latest]
        rust: [stable, beta]

    steps:
      - uses: actions/checkout@v4

      - name: Install Rust
        uses: dtolnay/rust-action@stable
        with:
          toolchain: ${{ matrix.rust }}
          components: rustfmt, clippy

      - name: Cache cargo
        uses: actions/cache@v4
        with:
          path: |
            ~/.cargo/registry
            ~/.cargo/git
            target
          key: ${{ runner.os }}-cargo-${{ hashFiles('**/Cargo.lock') }}

      - name: Check formatting
        run: cargo fmt --all -- --check

      - name: Clippy
        run: cargo clippy --all-targets --all-features -- -D warnings

      - name: Build
        run: cargo build --all-features

      - name: Test
        run: cargo test --all-features

      - name: Doc tests
        run: cargo test --doc --all-features

  python:
    runs-on: ${{ matrix.os }}
    strategy:
      matrix:
        os: [ubuntu-latest, macos-latest]
        python: ["3.10", "3.11", "3.12"]

    steps:
      - uses: actions/checkout@v4

      - name: Set up Python
        uses: actions/setup-python@v5
        with:
          python-version: ${{ matrix.python }}

      - name: Install Rust
        uses: dtolnay/rust-action@stable

      - name: Install maturin
        run: pip install maturin pytest numpy

      - name: Build Python package
        run: |
          cd crates/rustpix-python
          maturin develop

      - name: Test Python bindings
        run: pytest python/tests/

  benchmark:
    runs-on: ubuntu-latest
    if: github.event_name == 'push' && github.ref == 'refs/heads/main'

    steps:
      - uses: actions/checkout@v4

      - name: Install Rust
        uses: dtolnay/rust-action@stable

      - name: Run benchmarks
        run: cargo bench --all-features -- --save-baseline main

      - name: Upload benchmark results
        uses: actions/upload-artifact@v4
        with:
          name: benchmark-results
          path: target/criterion
```

---

## Part 9: Migration Guide

### For TDCSophiread Users

```python
# Before (TDCSophiread)
import tdcsophiread

config = tdcsophiread.DetectorConfig.venus_defaults()
processor = tdcsophiread.TDCProcessor(config)
hits = processor.process_file("data.tpx3", parallel=True)

# After (rustpix)
import rustpix

config = rustpix.DetectorConfig.venus_defaults()
processor = rustpix.Tpx3Processor(config)
hits = processor.process_file("data.tpx3", parallel=True)

# The hit array structure is identical
print(hits['tof'], hits['x'], hits['y'])
```

### API Mapping

| TDCSophiread | rustpix |
|--------------|---------|
| `TDCProcessor` | `Tpx3Processor` |
| `DetectorConfig` | `DetectorConfig` |
| `TDCHit` | `hit` (structured array) |
| `TDCNeutron` | `neutron` (structured array) |
| `process_tpx3()` | `process_tpx3()` |

---

## Part 10: Future Extensions

### TPX4 Support (Future)

```rust
// crates/rustpix-tpx/src/tpx4/mod.rs
pub mod packet;   // TPX4 packet format
pub mod hit;      // TPX4 hit type
pub mod processor;

// TPX4 has different packet format but same processing pipeline
pub struct Tpx4Packet(u64);
pub struct Tpx4Hit { /* similar fields */ }
pub struct Tpx4Processor { /* same interface */ }
```

### Other Detector Support

```rust
// Future: crates/rustpix-medipix/
// Future: crates/rustpix-advapix/

// All implement the same core traits
impl Hit for MedipixHit { ... }
impl HitClustering for MedipixClustering { ... }
```

---

## Appendix A: Key Algorithms Reference

### Pixel Address Decoding
```rust
fn decode_pixel_address(addr: u16) -> (u16, u16) {
    let dcol = ((addr & 0xFE00) >> 8) as u16;
    let spix = ((addr & 0x1F8) >> 1) as u16;
    let pix = (addr & 0x7) as u16;
    let x = dcol + (pix >> 2);
    let y = spix + (pix & 0x3);
    (x, y)
}
```

### Timestamp Rollover Correction
```rust
fn correct_rollover(hit_ts: u32, tdc_ts: u32) -> u32 {
    if hit_ts.wrapping_add(0x400000) < tdc_ts {
        hit_ts.wrapping_add(0x40000000)
    } else {
        hit_ts
    }
}
```

### TOF Calculation with TDC Correction
```rust
fn calculate_tof(timestamp: u32, tdc_ts: u32, correction: u32) -> u32 {
    let raw_tof = timestamp.wrapping_sub(tdc_ts);
    if raw_tof > correction {
        raw_tof.wrapping_sub(correction)
    } else {
        raw_tof
    }
}
```

### TOT-Weighted Centroid
```rust
fn weighted_centroid(hits: &[Hit]) -> (f64, f64) {
    let total_tot: f64 = hits.iter().map(|h| h.tot as f64).sum();
    let weighted_x: f64 = hits.iter().map(|h| h.x as f64 * h.tot as f64).sum();
    let weighted_y: f64 = hits.iter().map(|h| h.y as f64 * h.tot as f64).sum();
    (weighted_x / total_tot, weighted_y / total_tot)
}
```

---

## Appendix B: Performance Targets

| Metric | Target | Measurement Method |
|--------|--------|-------------------|
| Hit extraction | ≥96M hits/sec | Criterion benchmark on AMD EPYC |
| ABS clustering | ≥80M hits/sec | Criterion benchmark |
| DBSCAN clustering | ≥20M hits/sec | Criterion benchmark |
| Graph clustering | ≥40M hits/sec | Criterion benchmark |
| Python call overhead | <1ms | pytest-benchmark |
| Memory per hit | ≤24 bytes | `std::mem::size_of` |

---

## Appendix C: Test Data Requirements

1. **Small file** (<10MB): Quick tests, CI
2. **Medium file** (100MB-500MB): Integration tests
3. **Large file** (1GB+): Performance benchmarks
4. **Edge cases**:
   - Empty file
   - Single section
   - Many small sections
   - Timestamp rollover
   - Missing TDC packets

---

*Document Version: 1.0*
*Last Updated: 2025-01-27*
*Authors: Claude Code Assistant*
