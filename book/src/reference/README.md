# Technical Reference

This section provides detailed technical specifications for rustpix.

## Contents

- [HDF5 Schema](hdf5-schema.md) - NeXus-compatible HDF5 file format specification

## API Documentation

### Rust API

Comprehensive Rust API documentation is available on docs.rs:

- [rustpix-core](https://docs.rs/rustpix-core) - Core traits and types
- [rustpix-algorithms](https://docs.rs/rustpix-algorithms) - Clustering algorithms
- [rustpix-tpx](https://docs.rs/rustpix-tpx) - TPX3 parser
- [rustpix-io](https://docs.rs/rustpix-io) - File I/O

### Python API

See the [Python API](../python-api/README.md) chapter for comprehensive Python documentation.

## Data Formats

### Input: TPX3

Rustpix reads Timepix3 (TPX3) binary files. TPX3 files contain:
- Hit packets (pixel coordinates, timestamp, ToT)
- TDC packets (timing reference)
- Metadata headers

### Output Formats

| Format | Extension | Description |
|--------|-----------|-------------|
| HDF5 | `.h5`, `.hdf5` | NeXus-compatible, recommended for large datasets |
| Arrow | `.arrow` | Apache Arrow IPC format |
| Parquet | `.parquet` | Columnar format, good for analytics |
| CSV | `.csv` | Human-readable, simple export |
| Binary | `.bin`, `.dat` | Compact, fastest I/O |

## Performance Characteristics

### Throughput

| Operation | Throughput | Notes |
|-----------|------------|-------|
| TPX3 parsing | 96M+ hits/sec | Memory-mapped, parallel |
| ABS clustering | ~20M hits/sec | Single-threaded |
| Grid clustering | ~15M hits/sec | Multi-threaded, scales with cores |
| DBSCAN clustering | ~4M hits/sec | Spatial index overhead |

### Memory Usage

- **Streaming mode**: Bounded memory, configurable via `memory_fraction`
- **Batch mode**: ~100 bytes per hit for full processing
- **HDF5 export**: Chunked writes, minimal peak memory

## Version Compatibility

| Rustpix Version | Python | Rust | macOS | Linux | Windows |
|-----------------|--------|------|-------|-------|---------|
| 1.0.x | 3.11+ | 1.70+ | 11.0+ | glibc 2.28+ | 10+ |
