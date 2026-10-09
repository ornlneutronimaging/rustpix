# rustpix-tpx

Decoding of Timepix3 (TPX3) raw data for the rustpix crates. It splits a TPX3
byte buffer into per-chip sections, decodes hit and TDC packets, maps chip
pixels to detector coordinates, computes each hit's time-of-flight from its
pulse's TDC, and merges the chips into one stream ordered by pulse. To read
`.tpx3` files, use [`rustpix-io`](https://crates.io/crates/rustpix-io).

## Main API

- `Tpx3Packet`: one 64-bit packet with accessors for hit and TDC fields.
- `DetectorConfig`: TDC frequency, chip size and per-chip affine transforms.
  `venus_defaults()` (also `Default`) is the four-chip VENUS layout;
  `from_file` and `from_json` load a JSON description.
- `section::discover_sections`: finds the per-chip sections in a buffer.
- `ordering::TimeOrderedStream`: yields one `HitBatch` per pulse that has hits,
  all chips merged and sorted by TOF.

`tof` and `timestamp` are in 25 ns ticks.

## Example

```rust
use rustpix_tpx::ordering::TimeOrderedStream;
use rustpix_tpx::section::discover_sections;
use rustpix_tpx::DetectorConfig;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let data = std::fs::read("run.tpx3")?;
    let config = DetectorConfig::venus_defaults();
    let sections = discover_sections(&data);

    for pulse in TimeOrderedStream::new(data.as_slice(), &sections, &config) {
        println!("{} hits, first TOF {:?}", pulse.len(), pulse.tof.first());
    }
    Ok(())
}
```

## License

MIT. See [LICENSE](https://github.com/ornlneutronimaging/rustpix/blob/main/LICENSE).
