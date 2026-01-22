//! Section-aware TPX3 file processing.
#![allow(
    clippy::cast_lossless,
    clippy::items_after_statements,
    clippy::must_use_candidate,
    clippy::missing_panics_doc
)]

use super::packet::Tpx3Packet;

/// A contiguous section of TPX3 data for a single chip.
#[derive(Clone, Debug)]
pub struct Tpx3Section {
    /// Byte offset of section start.
    pub start_offset: usize,
    /// Byte offset of section end.
    pub end_offset: usize,
    /// Chip ID for this section.
    pub chip_id: u8,
    /// TDC state at section start (inherited from previous section).
    pub initial_tdc: Option<u32>,
    /// TDC state at section end (for propagation).
    pub final_tdc: Option<u32>,
}

impl Tpx3Section {
    /// Number of bytes in this section.
    pub fn byte_size(&self) -> usize {
        self.end_offset - self.start_offset
    }

    /// Number of 64-bit packets in this section.
    pub fn packet_count(&self) -> usize {
        self.byte_size() / 8
    }
}

/// Discover sections in a TPX3 file.
///
/// This performs Phase 1 of processing:
/// 1. Scan for TPX3 headers to identify section boundaries
/// 2. Track per-chip TDC state across sections
/// 3. Propagate TDC inheritance between sections
///
/// # Arguments
/// * `data` - Memory-mapped file data
///
/// # Returns
/// Vector of sections with TDC states populated.
pub fn discover_sections(data: &[u8]) -> Vec<Tpx3Section> {
    const PACKET_SIZE: usize = 8;

    if data.len() < PACKET_SIZE {
        return Vec::new();
    }

    let mut sections = Vec::new();
    let mut current_section: Option<Tpx3Section> = None;
    let mut per_chip_tdc: [Option<u32>; 256] = [None; 256]; // Track per-chip TDC

    let num_packets = data.len() / PACKET_SIZE;

    for i in 0..num_packets {
        let offset = i * PACKET_SIZE;
        let raw = u64::from_le_bytes(data[offset..offset + PACKET_SIZE].try_into().unwrap());
        let packet = Tpx3Packet::new(raw);

        if packet.is_header() {
            // Close current section
            if let Some(mut section) = current_section.take() {
                section.end_offset = offset;
                if section.byte_size() > 0 {
                    sections.push(section);
                }
            }

            // Start new section
            let chip_id = packet.chip_id();
            current_section = Some(Tpx3Section {
                start_offset: offset + PACKET_SIZE, // Skip header itself
                end_offset: 0,
                chip_id,
                initial_tdc: per_chip_tdc[chip_id as usize],
                final_tdc: None,
            });
        } else if packet.is_tdc() {
            // Track TDC for current chip
            if let Some(ref mut section) = current_section {
                let tdc_ts = packet.tdc_timestamp();
                section.final_tdc = Some(tdc_ts);
                per_chip_tdc[section.chip_id as usize] = Some(tdc_ts);
            }
        }
    }

    // Close final section
    if let Some(mut section) = current_section {
        section.end_offset = data.len();
        if section.byte_size() > 0 {
            sections.push(section);
        }
    }

    sections
}

/// Process a single section into hits.
///
/// This is designed to be called in parallel for different sections.
pub fn process_section<H: From<(u32, u16, u16, u32, u16, u8)>>(
    data: &[u8],
    section: &Tpx3Section,
    tdc_correction_25ns: u32,
    chip_transform: impl Fn(u8, u16, u16) -> (u16, u16),
) -> Vec<H> {
    use super::hit::{calculate_tof, correct_timestamp_rollover};

    const PACKET_SIZE: usize = 8;

    let section_data = &data[section.start_offset..section.end_offset];
    let num_packets = section_data.len() / PACKET_SIZE;

    // Pre-allocate based on expected hit density (~60% of packets are hits)
    let mut hits = Vec::with_capacity((num_packets * 6) / 10);

    let mut current_tdc = section.initial_tdc;

    for i in 0..num_packets {
        let offset = i * PACKET_SIZE;
        let raw = u64::from_le_bytes(
            section_data[offset..offset + PACKET_SIZE]
                .try_into()
                .unwrap(),
        );
        let packet = Tpx3Packet::new(raw);

        if packet.is_tdc() {
            current_tdc = Some(packet.tdc_timestamp());
        } else if packet.is_hit() {
            // Skip hits until we have a TDC reference
            let Some(tdc_ts) = current_tdc else { continue };

            let (local_x, local_y) = packet.pixel_coordinates();
            let (global_x, global_y) = chip_transform(section.chip_id, local_x, local_y);

            // Calculate timestamp with rollover correction
            let raw_timestamp = packet.timestamp_coarse();
            let timestamp = correct_timestamp_rollover(raw_timestamp, tdc_ts);
            let tof = calculate_tof(timestamp, tdc_ts, tdc_correction_25ns);

            hits.push(H::from((
                tof,
                global_x,
                global_y,
                timestamp,
                packet.tot(),
                section.chip_id,
            )));
        }
    }

    hits
}

/// Process a single section into a HitBatch (SoA).
pub fn process_section_into_batch(
    data: &[u8],
    section: &Tpx3Section,
    tdc_correction_25ns: u32,
    chip_transform: impl Fn(u8, u16, u16) -> (u16, u16),
    batch: &mut rustpix_core::soa::HitBatch,
) -> Option<u32> {
    use super::hit::{calculate_tof, correct_timestamp_rollover};

    const PACKET_SIZE: usize = 8;

    let section_data = &data[section.start_offset..section.end_offset];
    let num_packets = section_data.len() / PACKET_SIZE;

    let mut current_tdc = section.initial_tdc;

    for i in 0..num_packets {
        let offset = i * PACKET_SIZE;
        let raw = u64::from_le_bytes(
            section_data[offset..offset + PACKET_SIZE]
                .try_into()
                .unwrap(),
        );
        let packet = Tpx3Packet::new(raw);

        if packet.is_tdc() {
            current_tdc = Some(packet.tdc_timestamp());
        } else if packet.is_hit() {
            // Skip hits until we have a TDC reference
            let Some(tdc_ts) = current_tdc else { continue };

            let (local_x, local_y) = packet.pixel_coordinates();
            let (global_x, global_y) = chip_transform(section.chip_id, local_x, local_y);

            // Calculate timestamp with rollover correction
            let raw_timestamp = packet.timestamp_coarse();
            let timestamp = correct_timestamp_rollover(raw_timestamp, tdc_ts);
            let tof = calculate_tof(timestamp, tdc_ts, tdc_correction_25ns);

            batch.push(
                global_x,
                global_y,
                tof,
                packet.tot(),
                timestamp,
                section.chip_id,
            );
        }
    }

    current_tdc
}

/// Scans a section to find the final TDC timestamp.
/// Used for state propagation before full processing.
pub fn scan_section_tdc(data: &[u8], section: &Tpx3Section) -> Option<u32> {
    let section_data = &data[section.start_offset..section.end_offset];
    const PACKET_SIZE: usize = 8;
    let mut final_tdc = section.initial_tdc;

    for chunk in section_data.chunks_exact(PACKET_SIZE) {
        let raw = u64::from_le_bytes(chunk.try_into().unwrap());
        if ((raw >> 56) & 0xFF) == 0x6F {
            final_tdc = Some(((raw >> 12) & 0x3FFF_FFFF) as u32);
        }
    }
    final_tdc
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hit::Tpx3Hit;

    // Helper to create a TPX3 header packet
    fn make_header(chip_id: u8) -> u64 {
        Tpx3Packet::TPX3_HEADER_MAGIC | ((chip_id as u64) << 32)
    }

    // Helper to create a TDC packet
    fn make_tdc(timestamp: u32) -> u64 {
        0x6F00_0000_0000_0000 | ((timestamp as u64) << 12)
    }

    // Helper to create a Hit packet
    fn make_hit(toa: u16, tot: u16, addr: u16) -> u64 {
        0xB000_0000_0000_0000 | ((toa as u64) << 30) | ((tot as u64) << 20) | ((addr as u64) << 44)
    }

    #[test]
    fn test_discover_sections_single_chip() {
        let mut data = Vec::new();

        // Section 1: Chip 0
        data.extend_from_slice(&make_header(0).to_le_bytes());
        data.extend_from_slice(&make_tdc(1000).to_le_bytes());
        data.extend_from_slice(&make_hit(100, 10, 0).to_le_bytes());

        // Section 2: Chip 0 (should inherit TDC)
        data.extend_from_slice(&make_header(0).to_le_bytes());
        data.extend_from_slice(&make_hit(200, 10, 0).to_le_bytes());

        let sections = discover_sections(&data);

        assert_eq!(sections.len(), 2);

        // Check Section 1
        assert_eq!(sections[0].chip_id, 0);
        assert_eq!(sections[0].initial_tdc, None);
        assert_eq!(sections[0].final_tdc, Some(1000));

        // Check Section 2
        assert_eq!(sections[1].chip_id, 0);
        assert_eq!(sections[1].initial_tdc, Some(1000)); // Inherited!
    }

    #[test]
    fn test_discover_sections_multi_chip() {
        let mut data = Vec::new();

        // Section 1: Chip 0
        data.extend_from_slice(&make_header(0).to_le_bytes());
        data.extend_from_slice(&make_tdc(1000).to_le_bytes());

        // Section 2: Chip 1
        data.extend_from_slice(&make_header(1).to_le_bytes());
        data.extend_from_slice(&make_tdc(2000).to_le_bytes());

        // Section 3: Chip 0 (should inherit from Section 1)
        data.extend_from_slice(&make_header(0).to_le_bytes());
        data.extend_from_slice(&make_hit(300, 10, 0).to_le_bytes()); // Add hit so section isn't empty

        let sections = discover_sections(&data);

        assert_eq!(sections.len(), 3);

        assert_eq!(sections[0].chip_id, 0);
        assert_eq!(sections[0].final_tdc, Some(1000));

        assert_eq!(sections[1].chip_id, 1);
        assert_eq!(sections[1].final_tdc, Some(2000));

        assert_eq!(sections[2].chip_id, 0);
        assert_eq!(sections[2].initial_tdc, Some(1000)); // Inherited from Chip 0
    }

    #[test]
    fn test_process_section() {
        let mut data = Vec::new();

        // Header (skipped by process_section logic, but needed for offset calculation in test setup)
        // In reality, discover_sections gives us offsets that skip the header.
        // Let's construct data that matches what process_section expects (body only)

        let tdc_val = 1000;
        data.extend_from_slice(&make_tdc(tdc_val).to_le_bytes());

        // Hit: ToA=1100 (raw), ToT=10, Addr=0
        // Timestamp = 1100 << 4 = 17600
        // TOF = 17600 - 1000 = 16600
        data.extend_from_slice(&make_hit(1100, 10, 0).to_le_bytes());

        let section = Tpx3Section {
            start_offset: 0,
            end_offset: data.len(),
            chip_id: 0,
            initial_tdc: None,
            final_tdc: None,
        };

        let hits: Vec<Tpx3Hit> = process_section(
            &data,
            &section,
            1_000_000,        // Large correction
            |_, x, y| (x, y), // Identity transform
        );

        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].tot, 10);
        // Verify TOF calculation roughly
        // ToA 1100 -> timestamp 1100 (SPIDR=0). TDC 1000. Diff 100.
        assert_eq!(hits[0].tof, 100);
    }

    #[test]
    fn test_process_section_into_batch() {
        use rustpix_core::soa::HitBatch;

        let mut data = Vec::new();
        // Header

        let tdc_val = 1000;
        data.extend_from_slice(&make_tdc(tdc_val).to_le_bytes());

        // Hit: ToA=1100, ToT=10, Addr=0
        data.extend_from_slice(&make_hit(1100, 10, 0).to_le_bytes());

        let section = Tpx3Section {
            start_offset: 0,
            end_offset: data.len(),
            chip_id: 0,
            initial_tdc: None,
            final_tdc: None,
        };

        let mut batch = HitBatch::default();
        let end_tdc =
            process_section_into_batch(&data, &section, 1_000_000, |_, x, y| (x, y), &mut batch);

        assert_eq!(end_tdc, Some(1000));

        assert_eq!(batch.len(), 1);
        assert_eq!(batch.tot[0], 10);
        assert_eq!(batch.tof[0], 100);
        assert_eq!(batch.chip_id[0], 0);
    }
}
