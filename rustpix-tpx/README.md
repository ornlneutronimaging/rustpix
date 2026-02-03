# rustpix-tpx

TPX3 packet parser, hit types, and parallel file processor for the rustpix ecosystem.

## Features

- **TPX3 Packet Parsing**: Fast parsing of Timepix3 binary data packets
- **Hit Types**: Strongly-typed hit structures with timing information
- **Parallel Processing**: Multi-threaded file processing with rayon
- **Streaming**: Process large files chunk-by-chunk

## Usage

```rust
use rustpix_tpx::{Tpx3File, Tpx3Hit};

// Open and parse TPX3 file
let file = Tpx3File::open("data.tpx3")?;
let hits: Vec<Tpx3Hit> = file.parse_hits()?;

// Stream processing
for chunk in file.stream_hits()? {
    process_chunk(chunk);
}
```

## Hit Structure

```rust
pub struct Tpx3Hit {
    pub x: u16,           // Pixel X coordinate
    pub y: u16,           // Pixel Y coordinate
    pub toa: u64,         // Time of Arrival (ns)
    pub tot: u16,         // Time over Threshold (ns)
    pub ftoa: u8,         // Fine Time of Arrival
}
```

## License

MIT License - see [LICENSE](../LICENSE) for details.
