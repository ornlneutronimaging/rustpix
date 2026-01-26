//! Memory-mapped file readers.
//!

use crate::{Error, Result};
use memmap2::Mmap;
use rayon::prelude::*;
use rustpix_core::soa::HitBatch;
use rustpix_tpx::ordering::TimeOrderedStream;
use rustpix_tpx::section::{discover_sections, process_section_into_batch};
use rustpix_tpx::{DetectorConfig, Tpx3Packet};
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// A memory-mapped file reader.
///
/// Uses memmap2 to efficiently access file contents without
/// loading the entire file into memory.
pub struct MappedFileReader {
    mmap: Arc<Mmap>,
    path: PathBuf,
}

impl MappedFileReader {
    /// Opens a file for memory-mapped reading.
    ///
    /// # Errors
    /// Returns an error if the file cannot be opened or memory-mapped.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let file = File::open(&path)?;
        // SAFETY: The file is opened read-only and we assume it is not modified concurrently.
        // This is the standard safety contract for memory mapping.
        #[allow(unsafe_code)]
        let mmap = unsafe { Mmap::map(&file)? };
        Ok(Self {
            mmap: Arc::new(mmap),
            path: path.as_ref().to_path_buf(),
        })
    }

    /// Returns the file contents as a byte slice.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.mmap[..]
    }

    /// Returns the file size in bytes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.mmap.len()
    }

    /// Returns true if the file is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.mmap.is_empty()
    }

    /// Returns an iterator over 8-byte chunks.
    pub fn chunks(&self) -> impl Iterator<Item = &[u8]> {
        self.mmap.chunks(8)
    }
}

#[derive(Clone)]
struct SharedMmap(Arc<Mmap>);

impl AsRef<[u8]> for SharedMmap {
    fn as_ref(&self) -> &[u8] {
        &self.0[..]
    }
}

/// Time-ordered stream of hit batches that owns the underlying file mapping.
pub struct TimeOrderedHitStream {
    inner: TimeOrderedStream<SharedMmap>,
}

impl Iterator for TimeOrderedHitStream {
    type Item = HitBatch;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next()
    }
}

/// A pulse-ordered event batch with its TDC timestamp (25ns ticks).
pub struct EventBatch {
    pub tdc_timestamp_25ns: u64,
    pub hits: HitBatch,
}

/// Time-ordered stream of event batches that owns the underlying file mapping.
pub struct TimeOrderedEventStream {
    inner: TimeOrderedStream<SharedMmap>,
}

impl Iterator for TimeOrderedEventStream {
    type Item = EventBatch;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next_pulse_batch().map(|batch| EventBatch {
            tdc_timestamp_25ns: batch.tdc_timestamp,
            hits: batch.hits,
        })
    }
}

/// A TPX3 file reader with memory-mapped I/O.
pub struct Tpx3FileReader {
    reader: MappedFileReader,
    config: DetectorConfig,
}

impl Tpx3FileReader {
    /// Opens a TPX3 file for reading with default configuration.
    ///
    /// # Errors
    /// Returns an error if the file cannot be opened or memory-mapped.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let reader = MappedFileReader::open(path)?;
        Ok(Self {
            reader,
            config: DetectorConfig::default(),
        })
    }

    /// Sets the detector configuration.
    #[must_use]
    pub fn with_config(mut self, config: DetectorConfig) -> Self {
        self.config = config;
        self
    }

    /// Returns the file size in bytes.
    #[must_use]
    pub fn file_size(&self) -> usize {
        self.reader.len()
    }

    /// Returns the number of 8-byte packets in the file.
    #[must_use]
    pub fn packet_count(&self) -> usize {
        self.reader.len() / 8
    }

    /// Reads and parses all hits from the file into a `HitBatch` (`SoA`).
    ///
    /// # Errors
    /// Returns an error if the file size is invalid or the data cannot be parsed.
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
    ///
    /// # Errors
    /// Returns an error if the file size is invalid.
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

    /// Returns a time-ordered stream of hit batches (pulse-merged).
    ///
    /// # Errors
    /// Returns an error if the file size is invalid.
    pub fn stream_time_ordered(&self) -> Result<TimeOrderedHitStream> {
        if !self.reader.len().is_multiple_of(8) {
            return Err(Error::InvalidFormat(format!(
                "file size {} is not a multiple of 8 (file: {})",
                self.reader.len(),
                self.reader.path.display()
            )));
        }

        let sections = discover_sections(self.reader.as_bytes());
        let stream = TimeOrderedStream::new(
            SharedMmap(self.reader.mmap.clone()),
            &sections,
            &self.config,
        );
        Ok(TimeOrderedHitStream { inner: stream })
    }

    /// Returns a time-ordered stream of event batches (pulse-merged with TDC).
    ///
    /// # Errors
    /// Returns an error if the file size is invalid.
    pub fn stream_time_ordered_events(&self) -> Result<TimeOrderedEventStream> {
        if !self.reader.len().is_multiple_of(8) {
            return Err(Error::InvalidFormat(format!(
                "file size {} is not a multiple of 8 (file: {})",
                self.reader.len(),
                self.reader.path.display()
            )));
        }

        let sections = discover_sections(self.reader.as_bytes());
        let stream = TimeOrderedStream::new(
            SharedMmap(self.reader.mmap.clone()),
            &sections,
            &self.config,
        );
        Ok(TimeOrderedEventStream { inner: stream })
    }

    /// Returns an iterator over raw packets.
    ///
    /// # Panics
    /// Panics if a chunk is not exactly 8 bytes. This should be unreachable because
    /// `chunks_exact(8)` guarantees each chunk length.
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
