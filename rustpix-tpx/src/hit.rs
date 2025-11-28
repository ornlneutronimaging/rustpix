//! TPX3 hit data type.

use rustpix_core::{Hit, HitData, PixelCoord, TimeOfArrival};

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// TPX3-specific hit data.
///
/// Contains all information from a TPX3 pixel hit event,
/// including raw timing information and derived values.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Tpx3Hit {
    /// Pixel X coordinate (0-255).
    pub x: u16,
    /// Pixel Y coordinate (0-255).
    pub y: u16,
    /// Time of arrival in 1.5625 ns units (coarse + fine time).
    pub toa: u64,
    /// Time over threshold in 25 ns units.
    pub tot: u16,
    /// Fast time of arrival (fine time, 0-15).
    pub ftoa: u8,
    /// Spidertime (global timestamp from TDC).
    pub spidr_time: u16,
}

impl Tpx3Hit {
    /// Creates a new TPX3 hit.
    pub fn new(x: u16, y: u16, toa: u64, tot: u16, ftoa: u8, spidr_time: u16) -> Self {
        Self {
            x,
            y,
            toa,
            tot,
            ftoa,
            spidr_time,
        }
    }

    /// Creates a TPX3 hit from raw packet data.
    ///
    /// The raw packet format is a 64-bit value with:
    /// - bits 44-47: packet type (should be 0xB for pixel hit)
    /// - bits 35-43: ToT (10 bits, but only 9 used, in 25ns units)
    /// - bits 21-34: ToA coarse (14 bits)
    /// - bits 17-20: FToA (4 bits, fine time)
    /// - bits 9-16: SPIDR time (8 bits of 16-bit counter)
    /// - bits 0-8: pixel address part
    pub fn from_raw(raw: u64, super_pixel: u8, eoc: u8, spidr_time: u16) -> Self {
        // Extract fields from raw packet
        let tot = ((raw >> 20) & 0x3FF) as u16;
        let toa_coarse = (raw >> 30) & 0x3FFF;
        let ftoa = ((raw >> 44) & 0xF) as u8;
        let pixel_addr = (raw & 0xFF) as u16;

        // Calculate pixel coordinates
        let dcol = super_pixel * 2 + (pixel_addr >> 2) as u8 % 2;
        let spix = (pixel_addr >> 2) as u8 / 2;
        let pix = pixel_addr & 0x3;

        let x = dcol as u16 * 2 + (pix & 0x1);
        let y = spix as u16 * 4 + (pix >> 1) * 2 + (eoc as u16 & 0x1);

        // Calculate full ToA in 1.5625ns units
        // ToA = (coarse_time * 16 - ftoa) * 1.5625ns
        let toa = toa_coarse * 16 - ftoa as u64;

        Self {
            x,
            y,
            toa,
            tot,
            ftoa,
            spidr_time,
        }
    }

    /// Returns the ToA in nanoseconds.
    pub fn toa_ns(&self) -> f64 {
        self.toa as f64 * 1.5625
    }

    /// Returns the ToT in nanoseconds.
    pub fn tot_ns(&self) -> f64 {
        self.tot as f64 * 25.0
    }

    /// Converts to a generic HitData structure.
    pub fn to_hit_data(&self) -> HitData {
        HitData::new(self.x, self.y, self.toa, self.tot)
    }
}

impl Hit for Tpx3Hit {
    #[inline]
    fn coord(&self) -> PixelCoord {
        PixelCoord::new(self.x, self.y)
    }

    #[inline]
    fn toa(&self) -> TimeOfArrival {
        TimeOfArrival::new(self.toa)
    }

    #[inline]
    fn tot(&self) -> u16 {
        self.tot
    }
}

impl From<Tpx3Hit> for HitData {
    fn from(hit: Tpx3Hit) -> Self {
        hit.to_hit_data()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tpx3_hit_creation() {
        let hit = Tpx3Hit::new(100, 150, 1000000, 50, 5, 1000);
        assert_eq!(hit.x, 100);
        assert_eq!(hit.y, 150);
        assert_eq!(hit.toa, 1000000);
        assert_eq!(hit.tot, 50);
        assert_eq!(hit.ftoa, 5);
        assert_eq!(hit.spidr_time, 1000);
    }

    #[test]
    fn test_tpx3_hit_trait() {
        let hit = Tpx3Hit::new(100, 150, 1000000, 50, 5, 1000);
        assert_eq!(hit.x(), 100);
        assert_eq!(hit.y(), 150);
        assert_eq!(hit.toa_raw(), 1000000);
        assert_eq!(Hit::tot(&hit), 50);
    }

    #[test]
    fn test_tpx3_hit_timing() {
        let hit = Tpx3Hit::new(0, 0, 64, 4, 0, 0);
        // 64 * 1.5625 = 100 ns
        assert!((hit.toa_ns() - 100.0).abs() < 0.001);
        // 4 * 25 = 100 ns
        assert!((hit.tot_ns() - 100.0).abs() < 0.001);
    }

    #[test]
    fn test_conversion_to_hit_data() {
        let tpx3_hit = Tpx3Hit::new(100, 150, 1000000, 50, 5, 1000);
        let hit_data: HitData = tpx3_hit.into();
        assert_eq!(hit_data.coord.x, 100);
        assert_eq!(hit_data.coord.y, 150);
        assert_eq!(hit_data.toa.as_u64(), 1000000);
        assert_eq!(hit_data.tot, 50);
    }
}
