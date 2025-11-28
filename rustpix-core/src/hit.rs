//! Hit traits and types for pixel detector data.

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Pixel coordinate on the detector.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct PixelCoord {
    /// X coordinate (column).
    pub x: u16,
    /// Y coordinate (row).
    pub y: u16,
}

impl PixelCoord {
    /// Creates a new pixel coordinate.
    #[inline]
    pub fn new(x: u16, y: u16) -> Self {
        Self { x, y }
    }

    /// Computes the squared Euclidean distance to another coordinate.
    #[inline]
    pub fn distance_squared(&self, other: &Self) -> u32 {
        let dx = (self.x as i32 - other.x as i32).unsigned_abs();
        let dy = (self.y as i32 - other.y as i32).unsigned_abs();
        dx * dx + dy * dy
    }

    /// Checks if this coordinate is adjacent to another (8-connectivity).
    #[inline]
    pub fn is_adjacent(&self, other: &Self) -> bool {
        let dx = (self.x as i32 - other.x as i32).abs();
        let dy = (self.y as i32 - other.y as i32).abs();
        dx <= 1 && dy <= 1 && (dx != 0 || dy != 0)
    }
}

/// Time of arrival in detector units (typically picoseconds or nanoseconds).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct TimeOfArrival(pub u64);

impl TimeOfArrival {
    /// Creates a new time of arrival.
    #[inline]
    pub fn new(toa: u64) -> Self {
        Self(toa)
    }

    /// Returns the raw time value.
    #[inline]
    pub fn as_u64(&self) -> u64 {
        self.0
    }

    /// Computes the absolute time difference.
    #[inline]
    pub fn abs_diff(&self, other: &Self) -> u64 {
        self.0.abs_diff(other.0)
    }
}

/// Core data structure for a single hit event.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct HitData {
    /// Pixel coordinate.
    pub coord: PixelCoord,
    /// Time of arrival.
    pub toa: TimeOfArrival,
    /// Time over threshold (proportional to charge/energy).
    pub tot: u16,
}

impl HitData {
    /// Creates a new hit data structure.
    #[inline]
    pub fn new(x: u16, y: u16, toa: u64, tot: u16) -> Self {
        Self {
            coord: PixelCoord::new(x, y),
            toa: TimeOfArrival::new(toa),
            tot,
        }
    }
}

/// Trait for hit data from pixel detectors.
///
/// This trait provides a common interface for different detector types
/// (TPX3, TPX4, etc.) to expose their hit data in a uniform way.
pub trait Hit: Send + Sync {
    /// Returns the pixel coordinate of the hit.
    fn coord(&self) -> PixelCoord;

    /// Returns the time of arrival.
    fn toa(&self) -> TimeOfArrival;

    /// Returns the time over threshold (charge proxy).
    fn tot(&self) -> u16;

    /// Returns the x coordinate.
    #[inline]
    fn x(&self) -> u16 {
        self.coord().x
    }

    /// Returns the y coordinate.
    #[inline]
    fn y(&self) -> u16 {
        self.coord().y
    }

    /// Returns the raw time of arrival value.
    #[inline]
    fn toa_raw(&self) -> u64 {
        self.toa().as_u64()
    }
}

impl Hit for HitData {
    #[inline]
    fn coord(&self) -> PixelCoord {
        self.coord
    }

    #[inline]
    fn toa(&self) -> TimeOfArrival {
        self.toa
    }

    #[inline]
    fn tot(&self) -> u16 {
        self.tot
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pixel_coord_distance() {
        let p1 = PixelCoord::new(0, 0);
        let p2 = PixelCoord::new(3, 4);
        assert_eq!(p1.distance_squared(&p2), 25);
    }

    #[test]
    fn test_pixel_coord_adjacency() {
        let center = PixelCoord::new(5, 5);

        // Adjacent pixels
        assert!(center.is_adjacent(&PixelCoord::new(4, 4)));
        assert!(center.is_adjacent(&PixelCoord::new(5, 4)));
        assert!(center.is_adjacent(&PixelCoord::new(6, 6)));

        // Same pixel
        assert!(!center.is_adjacent(&center));

        // Non-adjacent pixels
        assert!(!center.is_adjacent(&PixelCoord::new(7, 5)));
        assert!(!center.is_adjacent(&PixelCoord::new(5, 7)));
    }

    #[test]
    fn test_time_of_arrival() {
        let t1 = TimeOfArrival::new(1000);
        let t2 = TimeOfArrival::new(1500);
        assert_eq!(t1.abs_diff(&t2), 500);
        assert_eq!(t2.abs_diff(&t1), 500);
    }

    #[test]
    fn test_hit_data() {
        let hit = HitData::new(10, 20, 1000, 50);
        assert_eq!(hit.x(), 10);
        assert_eq!(hit.y(), 20);
        assert_eq!(hit.toa_raw(), 1000);
        assert_eq!(hit.tot(), 50);
    }
}
