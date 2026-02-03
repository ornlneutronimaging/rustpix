# rustpix-tpx

TPX3 (Timepix3) packet parser and file processor for the rustpix library.

## Overview

This crate handles parsing and processing of TPX3 detector data:

- Binary packet parsing (pixel hits, timestamps, global timestamps)
- Detector configuration support
- Hit filtering and sorting
- Timestamp reconstruction and correction

## Supported Packet Types

- **Pixel Data (0xB)** - Individual pixel hit events
- **Timestamp (0x6)** - Coarse timestamps for synchronization
- **Global Time (0x4)** - Global timing information

## Usage

```rust
use rustpix_tpx::{Tpx3Parser, Tpx3Packet};

// Parse raw TPX3 data
let data: &[u8] = /* your TPX3 binary data */;
let parser = Tpx3Parser::new();
let packets = parser.parse(data)?;

for packet in packets {
    match packet {
        Tpx3Packet::PixelHit(hit) => {
            println!("Hit at ({}, {})", hit.x(), hit.y());
        }
        _ => {}
    }
}
```

## License

MIT License - see [LICENSE](../LICENSE) for details.
