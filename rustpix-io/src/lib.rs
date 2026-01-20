//! rustpix-io: Memory-mapped file I/O for rustpix.
//!
//! This crate provides efficient file reading and writing using
//! memory-mapped files via memmap2.
//!
//! See IMPLEMENTATION_PLAN.md Part 3.4 for file processing specification.

mod error;
mod reader;
pub mod scanner;
mod writer;

pub use error::{Error, Result};
pub use reader::{MappedFileReader, Tpx3FileReader};
pub use scanner::PacketScanner;
pub use writer::DataFileWriter;
