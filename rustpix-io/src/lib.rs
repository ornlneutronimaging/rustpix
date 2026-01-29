//! rustpix-io: Memory-mapped file I/O for rustpix.
//!
//! This crate provides efficient file reading and writing using
//! memory-mapped files via memmap2.
//!

mod error;
#[cfg(feature = "hdf5")]
pub mod hdf5;
pub mod out_of_core;
mod out_of_core_pipeline;
mod reader;
pub mod scanner;
mod writer;

pub use error::{Error, Result};
#[cfg(feature = "hdf5")]
pub use hdf5::{Hdf5HistogramSink, Hdf5HitSink, Hdf5NeutronSink, HistogramAxisData, HistogramBin};
pub use out_of_core::{pulse_batches, OutOfCoreConfig, PulseBatchGroup, PulseBatcher, PulseSlice};
pub use out_of_core_pipeline::{
    out_of_core_neutron_stream, out_of_core_neutron_stream_handle, OutOfCoreNeutronStream,
    OutOfCoreNeutronStreamHandle, PulseNeutronBatch, ThreadedOutOfCoreNeutronStream,
};
pub use reader::{
    EventBatch, MappedFileReader, TimeOrderedEventStream, TimeOrderedHitStream, Tpx3FileReader,
};
pub use scanner::PacketScanner;
pub use writer::DataFileWriter;
