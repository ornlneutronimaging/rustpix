//! TPX3 packet types and structures.

use crate::{Error, Result, Tpx3Hit};

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// TPX3 packet types as defined in the TPX3 documentation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[repr(u8)]
pub enum PacketType {
    /// Pixel hit data (type 0xB).
    PixelHit = 0xB,
    /// TDC1 rising edge (type 0x6, subtype 0xF).
    Tdc1Rising = 0x6F,
    /// TDC1 falling edge (type 0x6, subtype 0xA).
    Tdc1Falling = 0x6A,
    /// TDC2 rising edge (type 0x6, subtype 0xE).
    Tdc2Rising = 0x6E,
    /// TDC2 falling edge (type 0x6, subtype 0xB).
    Tdc2Falling = 0x6B,
    /// Global time (type 0x4).
    GlobalTime = 0x4,
    /// Control packet (type 0x7).
    Control = 0x7,
}

impl PacketType {
    /// Creates a PacketType from the raw packet header nibble.
    pub fn from_header(header: u8) -> Result<Self> {
        match header {
            0xB => Ok(PacketType::PixelHit),
            0x6 => Ok(PacketType::Tdc1Rising), // Will be refined with subtype
            0x4 => Ok(PacketType::GlobalTime),
            0x7 => Ok(PacketType::Control),
            _ => Err(Error::InvalidPacketType(header)),
        }
    }
}

/// A parsed TPX3 packet.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum Tpx3Packet {
    /// A pixel hit event.
    Hit(Tpx3Hit),
    /// TDC timestamp.
    Tdc {
        /// TDC type.
        tdc_type: TdcType,
        /// Timestamp value.
        timestamp: u64,
    },
    /// Global timestamp for synchronization.
    GlobalTime {
        /// Global timestamp value.
        timestamp: u64,
    },
    /// Control/configuration packet.
    Control {
        /// Control data.
        data: u64,
    },
}

/// TDC (Time-to-Digital Converter) types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum TdcType {
    /// TDC1 rising edge.
    Tdc1Rising,
    /// TDC1 falling edge.
    Tdc1Falling,
    /// TDC2 rising edge.
    Tdc2Rising,
    /// TDC2 falling edge.
    Tdc2Falling,
}

impl Tpx3Packet {
    /// Parses a raw 64-bit packet.
    pub fn parse(raw: u64) -> Result<Self> {
        let header = ((raw >> 60) & 0xF) as u8;

        match header {
            0xB => {
                // Pixel hit packet
                let super_pixel = ((raw >> 52) & 0x3F) as u8;
                let eoc = ((raw >> 58) & 0x3) as u8;
                let spidr_time = ((raw >> 48) & 0xFFFF) as u16;
                let hit = Tpx3Hit::from_raw(raw, super_pixel, eoc, spidr_time);
                Ok(Tpx3Packet::Hit(hit))
            }
            0x6 => {
                // TDC packet
                let subtype = ((raw >> 56) & 0xF) as u8;
                let timestamp = raw & 0x0FFF_FFFF_FFFF_FFFF;
                let tdc_type = match subtype {
                    0xF => TdcType::Tdc1Rising,
                    0xA => TdcType::Tdc1Falling,
                    0xE => TdcType::Tdc2Rising,
                    0xB => TdcType::Tdc2Falling,
                    _ => return Err(Error::InvalidPacketType(subtype)),
                };
                Ok(Tpx3Packet::Tdc {
                    tdc_type,
                    timestamp,
                })
            }
            0x4 => {
                // Global time packet
                let timestamp = raw & 0x0FFF_FFFF_FFFF_FFFF;
                Ok(Tpx3Packet::GlobalTime { timestamp })
            }
            0x7 => {
                // Control packet
                Ok(Tpx3Packet::Control { data: raw })
            }
            _ => Err(Error::InvalidPacketHeader(raw)),
        }
    }

    /// Returns true if this is a hit packet.
    pub fn is_hit(&self) -> bool {
        matches!(self, Tpx3Packet::Hit(_))
    }

    /// Extracts the hit if this is a hit packet.
    pub fn as_hit(&self) -> Option<&Tpx3Hit> {
        match self {
            Tpx3Packet::Hit(hit) => Some(hit),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_packet_type_from_header() {
        assert!(matches!(
            PacketType::from_header(0xB),
            Ok(PacketType::PixelHit)
        ));
        assert!(matches!(
            PacketType::from_header(0x4),
            Ok(PacketType::GlobalTime)
        ));
        assert!(PacketType::from_header(0x0).is_err());
    }

    #[test]
    fn test_packet_is_hit() {
        let hit_packet = Tpx3Packet::Hit(Tpx3Hit::new(0, 0, 0, 0, 0, 0));
        let tdc_packet = Tpx3Packet::Tdc {
            tdc_type: TdcType::Tdc1Rising,
            timestamp: 0,
        };

        assert!(hit_packet.is_hit());
        assert!(!tdc_packet.is_hit());
    }
}
