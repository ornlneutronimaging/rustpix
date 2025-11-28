//! TPX3 file parser.

use crate::{Error, Result, Tpx3Hit, Tpx3Packet};
use rayon::prelude::*;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Configuration for the TPX3 parser.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Tpx3ParserConfig {
    /// Whether to filter out non-hit packets.
    pub hits_only: bool,
    /// Whether to use parallel parsing.
    pub parallel: bool,
    /// Chunk size for parallel processing.
    pub chunk_size: usize,
}

impl Default for Tpx3ParserConfig {
    fn default() -> Self {
        Self {
            hits_only: true,
            parallel: true,
            chunk_size: 1024 * 1024, // 1M packets per chunk
        }
    }
}

impl Tpx3ParserConfig {
    /// Creates a new parser configuration with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets whether to filter hits only.
    pub fn with_hits_only(mut self, hits_only: bool) -> Self {
        self.hits_only = hits_only;
        self
    }

    /// Sets whether to use parallel parsing.
    pub fn with_parallel(mut self, parallel: bool) -> Self {
        self.parallel = parallel;
        self
    }

    /// Sets the chunk size for parallel processing.
    pub fn with_chunk_size(mut self, size: usize) -> Self {
        self.chunk_size = size;
        self
    }
}

/// TPX3 data parser.
#[derive(Debug, Clone, Default)]
pub struct Tpx3Parser {
    config: Tpx3ParserConfig,
}

impl Tpx3Parser {
    /// Creates a new parser with default configuration.
    pub fn new() -> Self {
        Self {
            config: Tpx3ParserConfig::default(),
        }
    }

    /// Creates a new parser with the given configuration.
    pub fn with_config(config: Tpx3ParserConfig) -> Self {
        Self { config }
    }

    /// Parses raw bytes as TPX3 packets.
    ///
    /// The data must be a slice of 8-byte packets in little-endian format.
    pub fn parse_bytes(&self, data: &[u8]) -> Result<Vec<Tpx3Packet>> {
        if !data.len().is_multiple_of(8) {
            return Err(Error::ParseError(format!(
                "data length {} is not a multiple of 8",
                data.len()
            )));
        }

        let raw_packets: Vec<u64> = data
            .chunks_exact(8)
            .map(|chunk| u64::from_le_bytes(chunk.try_into().unwrap()))
            .collect();

        self.parse_raw(&raw_packets)
    }

    /// Parses raw 64-bit packets.
    pub fn parse_raw(&self, raw_packets: &[u64]) -> Result<Vec<Tpx3Packet>> {
        if self.config.parallel && raw_packets.len() > self.config.chunk_size {
            self.parse_parallel(raw_packets)
        } else {
            self.parse_sequential(raw_packets)
        }
    }

    /// Parses packets sequentially.
    fn parse_sequential(&self, raw_packets: &[u64]) -> Result<Vec<Tpx3Packet>> {
        let mut packets = Vec::with_capacity(raw_packets.len());

        for &raw in raw_packets {
            match Tpx3Packet::parse(raw) {
                Ok(packet) => {
                    if !self.config.hits_only || packet.is_hit() {
                        packets.push(packet);
                    }
                }
                Err(_) if self.config.hits_only => {
                    // Skip invalid packets in hits_only mode
                    continue;
                }
                Err(e) => return Err(e),
            }
        }

        Ok(packets)
    }

    /// Parses packets in parallel using rayon.
    fn parse_parallel(&self, raw_packets: &[u64]) -> Result<Vec<Tpx3Packet>> {
        let results: Vec<Option<Tpx3Packet>> = raw_packets
            .par_iter()
            .map(|&raw| {
                match Tpx3Packet::parse(raw) {
                    Ok(packet) => {
                        if !self.config.hits_only || packet.is_hit() {
                            Some(packet)
                        } else {
                            None
                        }
                    }
                    Err(_) if self.config.hits_only => None,
                    Err(_) => None, // In parallel mode, skip errors
                }
            })
            .collect();

        Ok(results.into_iter().flatten().collect())
    }

    /// Parses raw packets and extracts only hit events.
    pub fn parse_hits(&self, raw_packets: &[u64]) -> Result<Vec<Tpx3Hit>> {
        let packets = self.parse_raw(raw_packets)?;
        Ok(packets
            .into_iter()
            .filter_map(|p| match p {
                Tpx3Packet::Hit(hit) => Some(hit),
                _ => None,
            })
            .collect())
    }

    /// Parses bytes and extracts only hit events.
    pub fn parse_hits_from_bytes(&self, data: &[u8]) -> Result<Vec<Tpx3Hit>> {
        let packets = self.parse_bytes(data)?;
        Ok(packets
            .into_iter()
            .filter_map(|p| match p {
                Tpx3Packet::Hit(hit) => Some(hit),
                _ => None,
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parser_config() {
        let config = Tpx3ParserConfig::new()
            .with_hits_only(false)
            .with_parallel(false)
            .with_chunk_size(1000);

        assert!(!config.hits_only);
        assert!(!config.parallel);
        assert_eq!(config.chunk_size, 1000);
    }

    #[test]
    fn test_parser_invalid_length() {
        let parser = Tpx3Parser::new();
        let data = [0u8; 7]; // Not a multiple of 8
        assert!(parser.parse_bytes(&data).is_err());
    }

    #[test]
    fn test_parser_empty_data() {
        let parser = Tpx3Parser::new();
        let data: [u8; 0] = [];
        let result = parser.parse_bytes(&data).unwrap();
        assert!(result.is_empty());
    }
}
