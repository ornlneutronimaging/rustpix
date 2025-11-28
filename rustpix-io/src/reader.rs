//! Memory-mapped file readers.

use crate::{Error, Result};
use memmap2::Mmap;
use rustpix_tpx::{Tpx3Hit, Tpx3Parser, Tpx3ParserConfig};
use std::fs::File;
use std::path::Path;

/// A memory-mapped file reader.
///
/// Uses memmap2 to efficiently access file contents without
/// loading the entire file into memory.
pub struct MappedFileReader {
    mmap: Mmap,
}

impl MappedFileReader {
    /// Opens a file for memory-mapped reading.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let file = File::open(path)?;
        let mmap = unsafe { Mmap::map(&file)? };
        Ok(Self { mmap })
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
///
/// Efficiently reads TPX3 data files and parses hit events.
pub struct Tpx3FileReader {
    reader: MappedFileReader,
    parser: Tpx3Parser,
}

impl Tpx3FileReader {
    /// Opens a TPX3 file for reading.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let reader = MappedFileReader::open(path)?;
        let parser = Tpx3Parser::new();
        Ok(Self { reader, parser })
    }

    /// Opens a TPX3 file with custom parser configuration.
    pub fn open_with_config<P: AsRef<Path>>(path: P, config: Tpx3ParserConfig) -> Result<Self> {
        let reader = MappedFileReader::open(path)?;
        let parser = Tpx3Parser::with_config(config);
        Ok(Self { reader, parser })
    }

    /// Returns the file size in bytes.
    pub fn file_size(&self) -> usize {
        self.reader.len()
    }

    /// Returns the number of 8-byte packets in the file.
    pub fn packet_count(&self) -> usize {
        self.reader.len() / 8
    }

    /// Reads and parses all hits from the file.
    pub fn read_hits(&self) -> Result<Vec<Tpx3Hit>> {
        if !self.reader.len().is_multiple_of(8) {
            return Err(Error::InvalidFormat(format!(
                "file size {} is not a multiple of 8",
                self.reader.len()
            )));
        }

        let hits = self.parser.parse_hits_from_bytes(self.reader.as_bytes())?;
        Ok(hits)
    }

    /// Reads hits from a specific byte range.
    pub fn read_hits_range(&self, start: usize, end: usize) -> Result<Vec<Tpx3Hit>> {
        if !start.is_multiple_of(8) || !end.is_multiple_of(8) {
            return Err(Error::InvalidFormat(
                "range boundaries must be multiples of 8".into(),
            ));
        }

        if end > self.reader.len() {
            return Err(Error::InvalidFormat("range exceeds file size".into()));
        }

        let data = &self.reader.as_bytes()[start..end];
        let hits = self.parser.parse_hits_from_bytes(data)?;
        Ok(hits)
    }

    /// Returns an iterator over batches of hits.
    pub fn iter_batches(
        &self,
        batch_size: usize,
    ) -> impl Iterator<Item = Result<Vec<Tpx3Hit>>> + '_ {
        let packet_size = 8;
        let batch_bytes = batch_size * packet_size;
        let data = self.reader.as_bytes();

        data.chunks(batch_bytes).map(move |chunk| {
            self.parser
                .parse_hits_from_bytes(chunk)
                .map_err(Error::from)
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

        let hits = reader.read_hits().unwrap();
        assert!(hits.is_empty());
    }

    #[test]
    fn test_tpx3_file_reader_invalid_size() {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(&[0u8; 7]).unwrap(); // Not a multiple of 8
        file.flush().unwrap();

        let reader = Tpx3FileReader::open(file.path()).unwrap();
        assert!(reader.read_hits().is_err());
    }
}
