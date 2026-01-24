//! Section scanner for TPX3 files.
//!
//! Identifies logical sections in the file based on TPX3 headers.

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// A section of the TPX3 file belonging to a specific chip.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Section {
    /// Start offset in bytes (inclusive).
    pub start_offset: usize,
    /// End offset in bytes (exclusive).
    pub end_offset: usize,
    /// Chip ID for this section.
    pub chip_id: u8,
}

/// Scanner for discovering sections in TPX3 data.
pub struct PacketScanner;

impl PacketScanner {
    /// Scans the provided data for TPX3 sections.
    ///
    /// The data should be 8-byte aligned.
    ///
    /// # Arguments
    /// * `data` - The byte slice to scan.
    /// * `is_eof` - Whether this is the final chunk of data.
    ///
    /// # Returns
    /// A tuple `(sections, consumed_bytes)`.
    /// `consumed_bytes` indicates how many bytes can be safely advanced.
    /// Bytes after `consumed_bytes` belong to an incomplete section (unless `is_eof`).
    ///
    /// # Panics
    /// Panics if a chunk is not exactly 8 bytes. This should be unreachable because
    /// `chunks_exact(8)` guarantees each chunk length.
    #[must_use]
    pub fn scan_sections(data: &[u8], is_eof: bool) -> (Vec<Section>, usize) {
        let mut sections = Vec::new();
        let mut current_section_start = 0;
        let mut current_chip_id = 0;
        let mut in_section = false;

        // Track the end of the last fully completed section
        let mut consumed_bytes = 0;

        // Iterate over data in 8-byte chunks
        for (offset, chunk) in data.chunks_exact(8).enumerate() {
            let offset_bytes = offset * 8;
            let packet = u64::from_le_bytes(chunk.try_into().unwrap());

            // Check if this is a TPX3 header: magic "TPX3" (0x33585054) in lower 32 bits
            if (packet & 0xFFFF_FFFF) == 0x3358_5054 {
                let chip_id = ((packet >> 32) & 0xFF) as u8;

                if in_section {
                    // Close previous section
                    sections.push(Section {
                        start_offset: current_section_start,
                        end_offset: offset_bytes,
                        chip_id: current_chip_id,
                    });
                    // Previous section ended here, so we have consumed up to here
                    consumed_bytes = offset_bytes;
                }

                // Start new section
                current_section_start = offset_bytes;
                current_chip_id = chip_id;
                in_section = true;
            }
        }

        if is_eof {
            // fast forward consumed to end
            consumed_bytes = data.len();

            if in_section && data.len() > current_section_start {
                sections.push(Section {
                    start_offset: current_section_start,
                    end_offset: data.len(),
                    chip_id: current_chip_id,
                });
            }
        } else {
            // If we are not at EOF, and we are inside a section,
            // that section is incomplete.
            // Special case: If we haven't found ANY closed sections (consumed_bytes == 0)
            // but we found a start header?
            // Then we return 0 consumed, meaning "need more data".
            // If the buffer is huge and we still return 0, the section is huge.
        }

        (sections, consumed_bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reader::MappedFileReader;
    use std::path::PathBuf;

    #[test]
    fn test_scan_tiny_tpx3() {
        let path = PathBuf::from("../tests/data/tiny.tpx3");
        if !path.exists() {
            eprintln!("Skipping test: tiny.tpx3 not found");
            return;
        }

        let reader = MappedFileReader::open(path.to_str().unwrap()).unwrap();
        let (sections, consumed) = PacketScanner::scan_sections(reader.as_bytes(), true);

        let sections_len = sections.len();
        println!("Found {sections_len} sections");
        println!("Consumed {consumed} bytes");

        assert_eq!(consumed, reader.len()); // Should consume everything at EOF

        for (i, section) in sections.iter().enumerate() {
            let chip_id = section.chip_id;
            let byte_len = section.end_offset - section.start_offset;
            println!("Section {i}: Chip {chip_id}, {byte_len} bytes");
        }

        assert!(!sections.is_empty(), "Should find at least one section");
        // Verify contiguous sections (optional, but good sanity check)
        for i in 0..sections.len() - 1 {
            assert_eq!(
                sections[i].end_offset,
                sections[i + 1].start_offset,
                "Sections should be contiguous"
            );
        }
    }
}
