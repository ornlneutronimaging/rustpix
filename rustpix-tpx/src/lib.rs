//! rustpix-tpx: TPX3 packet parser, hit types, and file processor.
//!
//! This crate provides TPX3-specific data structures and parsing logic
//! for Timepix3 pixel detector data.

mod error;
mod hit;
mod packet;
mod parser;

pub use error::{Error, Result};
pub use hit::Tpx3Hit;
pub use packet::{PacketType, Tpx3Packet};
pub use parser::{Tpx3Parser, Tpx3ParserConfig};
