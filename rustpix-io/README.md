# rustpix-io

File I/O and data export for the rustpix library.

## Overview

This crate provides efficient file I/O capabilities:

- Memory-mapped file reading for large datasets
- HDF5 export for scientific data interchange
- Streaming processing for memory-constrained environments
- Progress tracking for long-running operations

## Features

- `hdf5` - Enable HDF5 file export (requires HDF5 library)
- `serde` - Enable serialization/deserialization support

## Usage

```rust
use rustpix_io::{Tpx3File, ProcessingConfig};

// Open a TPX3 file with memory mapping
let file = Tpx3File::open("data.tpx3")?;

// Configure processing
let config = ProcessingConfig::default()
    .with_clustering(true)
    .with_time_correction(true);

// Process the file
let results = file.process(&config)?;

// Export to HDF5
#[cfg(feature = "hdf5")]
results.export_hdf5("output.h5")?;
```

## Supported Formats

| Format | Read | Write |
|--------|------|-------|
| TPX3 binary | ✓ | - |
| HDF5 | - | ✓ |
| JSON | ✓ | ✓ |

## License

MIT License - see [LICENSE](../LICENSE) for details.
