//! Memory-mapped file readers.
#![allow(
    clippy::return_self_not_must_use,
    clippy::must_use_candidate,
    clippy::missing_errors_doc,
    clippy::items_after_statements,
    clippy::missing_panics_doc,
    unsafe_code
)]
//!

use crate::{Error, Result};
use memmap2::Mmap;
use rustpix_core::soa::HitBatch;
use rustpix_tpx::ordering::TimeOrderedStream;
use rustpix_tpx::section::{discover_sections, process_section_into_batch};
use rustpix_tpx::{DetectorConfig, Tpx3Packet};
use std::fs::File;
use std::path::{Path, PathBuf};

/// A memory-mapped file reader.
///
/// Uses memmap2 to efficiently access file contents without
/// loading the entire file into memory.
pub struct MappedFileReader {
    mmap: Mmap,
    path: PathBuf,
}

impl MappedFileReader {
    /// Opens a file for memory-mapped reading.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let file = File::open(&path)?;
        // SAFETY: The file is opened read-only and we assume it is not modified concurrently.
        // This is the standard safety contract for memory mapping.
        let mmap = unsafe { Mmap::map(&file)? };
        Ok(Self {
            mmap,
            path: path.as_ref().to_path_buf(),
        })
    }

    /// Returns the file contents as a byte slice.
    pub fn as_bytes(&self) -> &[u8] {
        &self.mmap
    }

    /// Returns the file size in bytes.
    pub fn len(&self) -> usize {
        self.mmap.len()
    }

    /// Returns true if the file is empty.
    pub fn is_empty(&self) -> bool {
        self.mmap.is_empty()
    }

    /// Returns an iterator over 8-byte chunks.
    pub fn chunks(&self) -> impl Iterator<Item = &[u8]> {
        self.mmap.chunks(8)
    }
}

/// A TPX3 file reader with memory-mapped I/O.
pub struct Tpx3FileReader {
    reader: MappedFileReader,
    config: DetectorConfig,
}

impl Tpx3FileReader {
    /// Opens a TPX3 file for reading with default configuration.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let reader = MappedFileReader::open(path)?;
        Ok(Self {
            reader,
            config: DetectorConfig::default(),
        })
    }

    /// Sets the detector configuration.
    pub fn with_config(mut self, config: DetectorConfig) -> Self {
        self.config = config;
        self
    }

    /// Returns the file size in bytes.
    pub fn file_size(&self) -> usize {
        self.reader.len()
    }

    /// Returns the number of 8-byte packets in the file.
    pub fn packet_count(&self) -> usize {
        self.reader.len() / 8
    }

    /// Reads and parses all hits from the file into a `HitBatch` (`SoA`).
    pub fn read_batch(&self) -> Result<HitBatch> {
        if !self.reader.len().is_multiple_of(8) {
            return Err(Error::InvalidFormat(format!(
                "file size {} is not a multiple of 8 (file: {})",
                self.reader.len(),
                self.reader.path.display()
            )));
        }

        let data = self.reader.as_bytes();

        // Phase 1: Discover sections
        let sections = discover_sections(data);

        // Phase 2: Process sections
        let tdc_correction = self.config.tdc_correction_25ns();
        let config = &self.config;

        use rayon::prelude::*;

        let mut section_batches: Vec<HitBatch> = sections
            .par_iter()
            .map(|section| {
                let mut batch = HitBatch::with_capacity((section.packet_count() * 6) / 10);
                let _ = process_section_into_batch(
                    data,
                    section,
                    tdc_correction,
                    |chip_id, x, y| config.map_chip_to_global(chip_id, x, y),
                    &mut batch,
                );
                batch
            })
            .collect();

        let total_hits = section_batches.iter().map(HitBatch::len).sum();
        let mut all_batch = HitBatch::with_capacity(total_hits);
        for batch in section_batches.drain(..) {
            all_batch.append(&batch);
        }

        // Sort by TOF for deterministic ordering
        all_batch.sort_by_tof();

        Ok(all_batch)
    }

    /// Reads hits using the efficient time-ordered stream.
    ///
    /// This uses a pulse-based K-way merge to produce time-ordered hits
    /// without loading the entire file or performing a global sort.
    pub fn read_batch_time_ordered(&self) -> Result<HitBatch> {
        if !self.reader.len().is_multiple_of(8) {
            return Err(Error::InvalidFormat(format!(
                "file size {} is not a multiple of 8 (file: {})",
                self.reader.len(),
                self.reader.path.display()
            )));
        }

        let data = self.reader.as_bytes();
        let sections = discover_sections(data);

        let stream = TimeOrderedStream::new(data, &sections, &self.config);
        let mut batch = HitBatch::default();
        for pulse_batch in stream {
            batch.append(&pulse_batch);
        }
        Ok(batch)
    }

    /// Returns an iterator over raw packets.
    pub fn iter_packets(&self) -> impl Iterator<Item = Tpx3Packet> + '_ {
        self.reader.as_bytes().chunks_exact(8).map(|chunk| {
            let bytes: [u8; 8] = chunk.try_into().unwrap();
            Tpx3Packet::new(u64::from_le_bytes(bytes))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_mapped_file_reader() {
        let mut file = NamedTempFile::new().unwrap();
        let data: Vec<u8> = (0..64).collect();
        file.write_all(&data).unwrap();
        file.flush().unwrap();

        let reader = MappedFileReader::open(file.path()).unwrap();
        assert_eq!(reader.len(), 64);
        assert!(!reader.is_empty());
        assert_eq!(reader.as_bytes(), &data[..]);
    }

    #[test]
    fn test_tpx3_file_reader_empty() {
        let file = NamedTempFile::new().unwrap();

        let reader = Tpx3FileReader::open(file.path()).unwrap();
        assert_eq!(reader.file_size(), 0);
        assert_eq!(reader.packet_count(), 0);

        let batch = reader.read_batch().unwrap();
        assert!(batch.is_empty());
    }

    #[test]
    fn test_tpx3_file_reader_invalid_size() {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(&[0u8; 7]).unwrap(); // Not a multiple of 8
        file.flush().unwrap();

        let reader = Tpx3FileReader::open(file.path()).unwrap();
        assert!(reader.read_batch().is_err());
    }
}
