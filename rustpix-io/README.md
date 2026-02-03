# rustpix-io

Memory-mapped file I/O and output writers for rustpix.

## Features

- **Memory-Mapped Reading**: Efficient large file handling with memmap2
- **Multiple Output Formats**: HDF5, Arrow/Parquet, CSV
- **Streaming Writers**: Write data incrementally without buffering
- **Metadata Preservation**: Store detector configuration and processing parameters

## Usage

### Memory-Mapped Reading

```rust
use rustpix_io::MmapReader;

let reader = MmapReader::open("large_file.tpx3")?;
let data = reader.read_region(offset, length)?;
```

### HDF5 Output

```rust
use rustpix_io::hdf5::Hdf5Writer;

let mut writer = Hdf5Writer::create("output.h5")?;
writer.write_neutrons(&neutrons)?;
writer.write_metadata(&metadata)?;
```

### CSV Output

```rust
use rustpix_io::csv::CsvWriter;

let mut writer = CsvWriter::create("output.csv")?;
for batch in neutron_stream {
    writer.write_batch(&batch)?;
}
```

## HDF5 Schema

```
output.h5
├── neutrons/
│   ├── x          (f64) X coordinates
│   ├── y          (f64) Y coordinates
│   ├── toa        (u64) Time of Arrival
│   ├── tot_sum    (u32) Total TOT
│   └── size       (u32) Cluster size
└── metadata/
    ├── version
    ├── algorithm
    └── parameters
```

## Optional Features

- `hdf5` - Enable HDF5 output (requires static linking)
- `serde` - Enable serialization support

## License

MIT License - see [LICENSE](../LICENSE) for details.
