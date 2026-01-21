//! TPX3 packet parsing.
//!

/// TPX3 packet wrapper providing efficient field extraction.
///
/// Packet format (64-bit):
/// - Hit packets (ID 0xB*):
///   - Bits 0-15: SPIDR time
///   - Bits 16-19: Fine ToA (4-bit)
///   - Bits 20-29: ToT (10-bit)
///   - Bits 30-43: ToA (14-bit)
///   - Bits 44-59: Pixel address (16-bit)
///   - Bits 60-63: Packet type ID
///
/// - TDC packets (ID 0x6F):
///   - Bits 12-41: 30-bit TDC timestamp
///   - Bits 56-63: Packet type ID
#[derive(Clone, Copy, Debug)]
pub struct Tpx3Packet(u64);

impl Tpx3Packet {
    /// TPX3 header magic number ("TPX3" in little-endian).
    pub const TPX3_HEADER_MAGIC: u64 = 0x33585054;

    /// Create from raw 64-bit value.
    #[inline]
    pub const fn new(raw: u64) -> Self {
        Self(raw)
    }

    /// Get raw packet value.
    #[inline]
    pub const fn raw(&self) -> u64 {
        self.0
    }

    /// Check if this is a TPX3 header packet.
    #[inline]
    pub const fn is_header(&self) -> bool {
        (self.0 & 0xFFFFFFFF) == Self::TPX3_HEADER_MAGIC
    }

    /// Check if this is a TDC packet (ID 0x6F).
    #[inline]
    pub const fn is_tdc(&self) -> bool {
        (self.0 >> 56) & 0xFF == 0x6F
    }

    /// Check if this is a hit packet (ID 0xB*).
    #[inline]
    pub const fn is_hit(&self) -> bool {
        (self.0 >> 60) & 0xF == 0xB
    }

    /// Get packet type identifier.
    #[inline]
    pub const fn packet_type(&self) -> u8 {
        ((self.0 >> 56) & 0xFF) as u8
    }

    /// Get chip ID from header packet (bits 32-39).
    #[inline]
    pub const fn chip_id(&self) -> u8 {
        ((self.0 >> 32) & 0xFF) as u8
    }

    /// Get 16-bit pixel address from hit packet.
    #[inline]
    pub const fn pixel_address(&self) -> u16 {
        ((self.0 >> 44) & 0xFFFF) as u16
    }

    /// Get 14-bit Time of Arrival.
    #[inline]
    pub const fn toa(&self) -> u16 {
        ((self.0 >> 30) & 0x3FFF) as u16
    }

    /// Get 10-bit Time over Threshold.
    #[inline]
    pub const fn tot(&self) -> u16 {
        ((self.0 >> 20) & 0x3FF) as u16
    }

    /// Get 4-bit fine ToA.
    #[inline]
    pub const fn fine_toa(&self) -> u8 {
        ((self.0 >> 16) & 0xF) as u8
    }

    /// Get SPIDR time (16-bit).
    #[inline]
    pub const fn spidr_time(&self) -> u16 {
        (self.0 & 0xFFFF) as u16
    }

    /// Get 30-bit TDC timestamp from TDC packet.
    #[inline]
    pub const fn tdc_timestamp(&self) -> u32 {
        ((self.0 >> 12) & 0x3FFFFFFF) as u32
    }

    /// Decode pixel address to local (x, y) coordinates.
    ///
    /// - dcol = (addr >> 8) & 0xFE
    /// - spix = (addr >> 1) & 0xFC
    /// - pix = addr & 0x7
    /// - x = dcol + (pix >> 2)
    /// - y = spix + (pix & 0x3)
    #[inline]
    pub const fn pixel_coordinates(&self) -> (u16, u16) {
        let addr = self.pixel_address();
        let dcol = (addr & 0xFE00) >> 8;
        let spix = (addr & 0x1F8) >> 1;
        let pix = addr & 0x7;
        let x = dcol + (pix >> 2);
        let y = spix + (pix & 0x3);
        (x, y)
    }
}

impl From<u64> for Tpx3Packet {
    fn from(raw: u64) -> Self {
        Self::new(raw)
    }
}

impl Tpx3Packet {
    /// Create from 8-byte array (little-endian).
    #[inline]
    pub fn from_bytes(bytes: [u8; 8]) -> Self {
        Self::new(u64::from_le_bytes(bytes))
    }

    /// Alias for is_hit - checks if this is pixel data.
    #[inline]
    pub const fn is_pixel_data(&self) -> bool {
        self.is_hit()
    }

    /// Calculate coarse timestamp in 25ns units from SPIDR time and ToA.
    ///
    /// Formula: (spidr_time << 14) | toa
    /// Matches C++ reference implementation.
    #[inline]
    pub fn timestamp_coarse(&self) -> u32 {
        let spidr = self.spidr_time() as u32;
        let toa = self.toa() as u32;

        // Combine SPIDR time and ToA to get 25ns timestamp
        (spidr << 14) | toa
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_header_detection() {
        let header = Tpx3Packet::new(0x33585054);
        assert!(header.is_header());

        let non_header = Tpx3Packet::new(0x12345678);
        assert!(!non_header.is_header());
    }

    #[test]
    fn test_tdc_detection() {
        let tdc = Tpx3Packet::new(0x6F00_0000_0000_0000);
        assert!(tdc.is_tdc());
        assert!(!tdc.is_hit());
    }

    #[test]
    fn test_hit_detection() {
        let hit = Tpx3Packet::new(0xB000_0000_0000_0000);
        assert!(hit.is_hit());
        assert!(!hit.is_tdc());
    }

    #[test]
    fn test_pixel_coordinate_decode() {
        // Test with a known address pattern
        // For addr = 0, we expect x=0, y=0
        let packet = Tpx3Packet::new(0xB000_0000_0000_0000);
        let (x, y) = packet.pixel_coordinates();
        assert_eq!(x, 0);
        assert_eq!(y, 0);
    }

    #[test]
    fn test_tdc_timestamp_extraction() {
        // TDC packet with timestamp value
        let tdc = Tpx3Packet::new(0x6F00_0001_2345_6000);
        let ts = tdc.tdc_timestamp();
        // bits 12-41 should be extracted
        assert!(ts > 0);
    }
}
