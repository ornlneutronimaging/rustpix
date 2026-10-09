# rustpix-io

File I/O for [rustpix](https://github.com/ornlneutronimaging/rustpix). It reads
Timepix3 `.tpx3` files through a memory map, returns hits time-ordered one pulse
at a time, clusters a file into neutrons pulse by pulse within a memory budget,
and writes neutrons as CSV or binary. The `hdf5` feature adds NeXus HDF5 and SNS
`NXsnsevent` reading and writing.

## Main types

- `Tpx3FileReader`: opens a `.tpx3` file; `stream_time_ordered_events()` yields
  one batch per pulse, `read_batch()` returns every hit in one `HitBatch`.
- `out_of_core_neutron_stream`: clusters a file pulse by pulse and yields one
  `PulseNeutronBatch` per pulse; `OutOfCoreConfig` sets the memory budget and
  worker threads.
- `DataFileWriter`: neutrons as CSV or 28-byte binary records.
- `hdf5` and `hdf5_sns` modules (feature `hdf5`): NeXus hits, neutrons,
  histograms and pixel masks; SNS event files.

## Example

```toml
[dependencies]
rustpix-io = "1.4"
rustpix-core = "1.4"
rustpix-algorithms = "1.4"
```

```rust
use rustpix_algorithms::{AlgorithmParams, ClusteringAlgorithm};
use rustpix_core::{ClusteringConfig, ExtractionConfig};
use rustpix_io::{out_of_core_neutron_stream, DataFileWriter, OutOfCoreConfig, Tpx3FileReader};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let reader = Tpx3FileReader::open("run.tpx3")?;
    let pulses = out_of_core_neutron_stream(
        &reader,
        ClusteringAlgorithm::Abs,
        &ClusteringConfig::default(),
        &ExtractionConfig::default(),
        &AlgorithmParams::default(),
        &OutOfCoreConfig::default(),
    )?;

    let mut writer = DataFileWriter::create("neutrons.csv")?;
    for (i, pulse) in pulses.enumerate() {
        writer.write_neutron_batch_csv(&pulse?.neutrons, i == 0)?;
    }
    Ok(())
}
```

## Features

- `hdf5`: builds the HDF5 C library from source; needs CMake and a C compiler.
- `serde`: `Serialize` and `Deserialize` for `HitBatch` and `scanner::Section`.

## License

MIT. See [LICENSE](https://github.com/ornlneutronimaging/rustpix/blob/main/LICENSE).
