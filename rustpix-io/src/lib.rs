//! rustpix-io: Memory-mapped file I/O for rustpix.
//!
//! This crate provides efficient file reading and writing using
//! memory-mapped files via memmap2.

mod error;
mod reader;
mod writer;

pub use error::{Error, Result};
pub use reader::{MappedFileReader, Tpx3FileReader};
pub use writer::Tpx3FileWriter;
