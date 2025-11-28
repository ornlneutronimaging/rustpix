# rustpix Development Guide

## Project Overview

rustpix is a high-performance Rust library for processing Timepix3 (TPX3) pixel detector data, primarily for neutron imaging at ORNL. This is a rewrite of the C++ TDCSophiread library.

## CRITICAL: Read IMPLEMENTATION_PLAN.md First

Before making ANY changes, read `IMPLEMENTATION_PLAN.md` thoroughly. It contains:
- Detailed algorithm implementations (with correct Rust code)
- TPX3 packet format specification (bit-level details)
- The REAL ABS clustering algorithm (bucket-based, O(n), NOT simple adjacency)
- TDC propagation and section discovery logic
- Python binding patterns with numpy structured arrays

## Key Implementation Details

### TPX3 Packet Format (64-bit)
```
Hit packets (ID 0xB*):
- Bits 0-15: SPIDR time
- Bits 16-19: Fine ToA (4-bit)
- Bits 20-29: ToT (10-bit)
- Bits 30-43: ToA (14-bit)
- Bits 44-59: Pixel address (16-bit)
- Bits 60-63: Packet type ID

TDC packets (ID 0x6F):
- Bits 12-41: 30-bit TDC timestamp
```

### Pixel Address Decoding
```rust
let dcol = ((addr & 0xFE00) >> 8) as u16;
let spix = ((addr & 0x1F8) >> 1) as u16;
let pix = (addr & 0x7) as u16;
let x = dcol + (pix >> 2);
let y = spix + (pix & 0x3);
```

### Critical Hit Fields
The hit structure MUST include:
- `tof: u32` - Time-of-flight in 25ns units (NOT just ToA!)
- `x: u16`, `y: u16` - Global coordinates (after chip transform)
- `timestamp: u32` - Hit timestamp in 25ns units
- `tot: u16` - Time-over-threshold
- `chip_id: u8` - Chip identifier (0-3)
- `cluster_id: i32` - Cluster assignment (-1 = unassigned)

### ABS Clustering Algorithm (CORRECT VERSION)
The ABS (Age-Based Spatial) algorithm is NOT simple adjacency clustering!

It uses:
1. **Bucket pool** - Pre-allocated, reusable buckets
2. **Spatial indexing** - 32x32 grid for O(1) bucket lookup
3. **Age-based closure** - Buckets closed when TOF exceeds temporal window
4. **Scan interval** - Check for aged buckets every N hits (default: 100)

Key parameters:
- `radius: 5.0` pixels
- `neutron_correlation_window: 75.0` nanoseconds
- `scan_interval: 100` hits

### Section Discovery & TDC Propagation
TPX3 files have sections marked by TPX3 headers. Each section:
1. Has a chip_id
2. Needs TDC state propagated from previous section of same chip
3. Can be processed in parallel once TDC state is known

### Performance Target
- 96M+ hits/sec on production hardware
- Use Rayon for parallelism
- Use memmap2 for memory-mapped I/O

## Build Commands

```bash
# Build all crates
cargo build --workspace

# Run tests
cargo test --workspace --exclude rustpix-python

# Build Python wheel
cd rustpix-python && maturin build --release
```

## Crate Dependencies

```
rustpix-python → rustpix-tpx → rustpix-core
                            → rustpix-algorithms → rustpix-core
               → rustpix-io

rustpix-cli → rustpix-tpx
            → rustpix-algorithms
            → rustpix-io
```

## Reference Implementation

The C++ reference implementation is at:
`/Users/8cz/github.com/ornlneutronimaging/mcpevent2hist/sophiread/TDCSophiread/`

Key files to reference:
- `include/tdc_packet.h` - Packet parsing
- `include/tdc_hit.h` - Hit structure
- `src/neutron_processing/abs_clustering.cpp` - ABS algorithm
- `include/tdc_processor.h` - Section discovery

## Testing

Sample TPX3 files for testing are in the reference repo:
`/Users/8cz/github.com/ornlneutronimaging/mcpevent2hist/sophiread/resources/data/`
