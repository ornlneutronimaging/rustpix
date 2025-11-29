//! rustpix-tpx: TPX3 packet parser, hit types, and file processor.
//!
//! This crate provides TPX3-specific data structures and parsing logic
//! for Timepix3 pixel detector data.
//!
//! See IMPLEMENTATION_PLAN.md Part 3 for detailed specification.
//!
//! # Key Components
//!
//! - [`Tpx3Packet`] - Low-level packet parser with bit field extraction
//! - [`Tpx3Hit`] - Hit data structure with TOF, coordinates, and cluster assignment
//! - `Tpx3Processor` - Section-aware file processor
//!
//! # Processing Pipeline
//!
//! 1. **Phase 1 (Sequential)**: Discover sections, propagate TDC state
//! 2. **Phase 2 (Parallel)**: Process sections into hits
//!
//! See IMPLEMENTATION_PLAN.md Part 3.3-3.4 for algorithm details.

mod hit;
mod packet;
pub mod section;

pub use hit::{calculate_tof, correct_timestamp_rollover, Tpx3Hit};
pub use packet::Tpx3Packet;

// Re-export core types for convenience
pub use rustpix_core::hit::Hit;

use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

/// Affine transformation for chip coordinate mapping.
///
/// Formula:
/// global_x = a * local_x + b * local_y + tx
/// global_y = c * local_x + d * local_y + ty
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChipTransform {
    pub a: i32,
    pub b: i32,
    pub c: i32,
    pub d: i32,
    pub tx: i32,
    pub ty: i32,
}

impl ChipTransform {
    /// Create an identity transform.
    pub fn identity() -> Self {
        Self {
            a: 1,
            b: 0,
            c: 0,
            d: 1,
            tx: 0,
            ty: 0,
        }
    }

    /// Apply transform to local coordinates.
    pub fn apply(&self, x: u16, y: u16) -> (u16, u16) {
        let x = x as i32;
        let y = y as i32;

        let gx = self.a * x + self.b * y + self.tx;
        let gy = self.c * x + self.d * y + self.ty;

        (gx as u16, gy as u16)
    }
}

/// Detector configuration for TPX3 processing.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DetectorConfig {
    /// TDC frequency in Hz (default: 60.0 for SNS).
    pub tdc_frequency_hz: f64,
    /// Enable missing TDC correction.
    pub enable_missing_tdc_correction: bool,
    /// Chip size in pixels (default: 256).
    pub chip_size: u16,
    /// Per-chip affine transforms.
    pub chip_transforms: Vec<ChipTransform>,
}

impl Default for DetectorConfig {
    fn default() -> Self {
        Self::venus_defaults()
    }
}

// Intermediate structs for C++ compatible JSON schema
#[derive(Deserialize)]
struct JsonConfig {
    detector: JsonDetector,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct JsonDetector {
    timing: JsonTiming,
    chip_layout: JsonChipLayout,
    chip_transformations: Option<Vec<JsonChipTransform>>,
}

#[derive(Deserialize)]
#[serde(default)]
struct JsonTiming {
    tdc_frequency_hz: f64,
    enable_missing_tdc_correction: bool,
}

impl Default for JsonTiming {
    fn default() -> Self {
        Self {
            tdc_frequency_hz: 60.0,
            enable_missing_tdc_correction: true,
        }
    }
}

#[derive(Deserialize)]
#[serde(default)]
struct JsonChipLayout {
    chip_size_x: u16,
    chip_size_y: u16,
}

impl Default for JsonChipLayout {
    fn default() -> Self {
        Self {
            chip_size_x: 256,
            chip_size_y: 256,
        }
    }
}

#[derive(Deserialize)]
struct JsonChipTransform {
    chip_id: u8,
    matrix: [[i32; 3]; 2],
}

impl DetectorConfig {
    /// Create VENUS/SNS default configuration.
    ///
    /// Uses specific affine transforms for the 4 chips:
    /// - Chip 0: Translation (258, 0)
    /// - Chip 1: Rotation 180 + Translation (513, 513)
    /// - Chip 2: Rotation 180 + Translation (255, 513)
    /// - Chip 3: Identity (0, 0)
    pub fn venus_defaults() -> Self {
        let transforms = vec![
            // Chip 0: [[1, 0, 258], [0, 1, 0]]
            ChipTransform {
                a: 1,
                b: 0,
                c: 0,
                d: 1,
                tx: 258,
                ty: 0,
            },
            // Chip 1: [[-1, 0, 513], [0, -1, 513]]
            ChipTransform {
                a: -1,
                b: 0,
                c: 0,
                d: -1,
                tx: 513,
                ty: 513,
            },
            // Chip 2: [[-1, 0, 255], [0, -1, 513]]
            ChipTransform {
                a: -1,
                b: 0,
                c: 0,
                d: -1,
                tx: 255,
                ty: 513,
            },
            // Chip 3: [[1, 0, 0], [0, 1, 0]]
            ChipTransform {
                a: 1,
                b: 0,
                c: 0,
                d: 1,
                tx: 0,
                ty: 0,
            },
        ];

        Self {
            tdc_frequency_hz: 60.0,
            enable_missing_tdc_correction: true,
            chip_size: 256,
            chip_transforms: transforms,
        }
    }

    /// Load configuration from a JSON file (C++ compatible schema).
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, Box<dyn std::error::Error>> {
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        let json_config: JsonConfig = serde_json::from_reader(reader)?;
        Ok(Self::from_json_config(json_config))
    }

    /// Load configuration from a JSON string (C++ compatible schema).
    pub fn from_json(json: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let json_config: JsonConfig = serde_json::from_str(json)?;
        Ok(Self::from_json_config(json_config))
    }

    fn from_json_config(config: JsonConfig) -> Self {
        let detector = config.detector;

        // Use VENUS defaults if no transformations specified (like C++)
        let transforms = match detector.chip_transformations {
            Some(transforms) if !transforms.is_empty() => {
                // Find max chip ID to size the vector
                let max_chip_id = transforms.iter().map(|t| t.chip_id).max().unwrap_or(0);

                let mut t_vec = vec![ChipTransform::identity(); (max_chip_id + 1) as usize];

                for t in transforms {
                    let matrix = t.matrix;
                    // C++ matrix: [[a, b, tx], [c, d, ty]]
                    t_vec[t.chip_id as usize] = ChipTransform {
                        a: matrix[0][0],
                        b: matrix[0][1],
                        tx: matrix[0][2],
                        c: matrix[1][0],
                        d: matrix[1][1],
                        ty: matrix[1][2],
                    };
                }
                t_vec
            }
            _ => {
                // Fall back to VENUS defaults
                Self::venus_defaults().chip_transforms
            }
        };

        Self {
            tdc_frequency_hz: detector.timing.tdc_frequency_hz,
            enable_missing_tdc_correction: detector.timing.enable_missing_tdc_correction,
            chip_size: detector.chip_layout.chip_size_x,
            chip_transforms: transforms,
        }
    }

    /// TDC period in seconds.
    pub fn tdc_period_seconds(&self) -> f64 {
        1.0 / self.tdc_frequency_hz
    }

    /// TDC correction value in 25ns units.
    pub fn tdc_correction_25ns(&self) -> u32 {
        (self.tdc_period_seconds() / 25e-9).round() as u32
    }

    /// Map local chip coordinates to global detector coordinates.
    ///
    /// Uses the configured affine transform for the given chip ID.
    /// If chip ID is out of bounds, returns local coordinates as-is (identity).
    pub fn map_chip_to_global(&self, chip_id: u8, x: u16, y: u16) -> (u16, u16) {
        if let Some(transform) = self.chip_transforms.get(chip_id as usize) {
            transform.apply(x, y)
        } else {
            (x, y)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_venus_defaults() {
        let config = DetectorConfig::venus_defaults();
        assert_eq!(config.tdc_frequency_hz, 60.0);
        assert!(config.enable_missing_tdc_correction);
        assert_eq!(config.chip_transforms.len(), 4);
    }

    #[test]
    fn test_tdc_correction() {
        let config = DetectorConfig::venus_defaults();
        // 1/60 Hz = 16.67ms, in 25ns units = 666,667
        let correction = config.tdc_correction_25ns();
        assert!(correction > 600_000 && correction < 700_000);
    }

    #[test]
    fn test_venus_chip_mappings() {
        let config = DetectorConfig::venus_defaults();

        // Chip 0: local (100, 100) -> global (358, 100)
        // x = 1*100 + 0*100 + 258 = 358
        // y = 0*100 + 1*100 + 0 = 100
        let (gx, gy) = config.map_chip_to_global(0, 100, 100);
        assert_eq!((gx, gy), (358, 100));

        // Chip 1: local (100, 100) -> global (413, 413)
        // x = -1*100 + 0*100 + 513 = 413
        // y = 0*100 + -1*100 + 513 = 413
        let (gx, gy) = config.map_chip_to_global(1, 100, 100);
        assert_eq!((gx, gy), (413, 413));

        // Chip 2: local (100, 100) -> global (155, 413)
        // x = -1*100 + 0*100 + 255 = 155
        // y = 0*100 + -1*100 + 513 = 413
        let (gx, gy) = config.map_chip_to_global(2, 100, 100);
        assert_eq!((gx, gy), (155, 413));

        // Chip 3: local (100, 100) -> global (100, 100)
        // x = 1*100 + 0*100 + 0 = 100
        // y = 0*100 + 1*100 + 0 = 100
        let (gx, gy) = config.map_chip_to_global(3, 100, 100);
        assert_eq!((gx, gy), (100, 100));
    }
    #[test]
    fn test_json_loading() {
        let json = r#"{
            "detector": {
                "timing": {
                    "tdc_frequency_hz": 14.0,
                    "enable_missing_tdc_correction": false
                },
                "chip_layout": {
                    "chip_size_x": 256,
                    "chip_size_y": 256
                },
                "chip_transformations": [
                    {
                        "chip_id": 0,
                        "matrix": [[1, 0, 100], [0, 1, 200]]
                    },
                    {
                        "chip_id": 1,
                        "matrix": [[-1, 0, 300], [0, -1, 400]]
                    }
                ]
            }
        }"#;

        let config = DetectorConfig::from_json(json).expect("Failed to parse JSON");

        assert_eq!(config.tdc_frequency_hz, 14.0);
        assert!(!config.enable_missing_tdc_correction);
        assert_eq!(config.chip_size, 256);
        assert_eq!(config.chip_transforms.len(), 2);

        // Check Chip 0: Identity + Translation (100, 200)
        let (gx, gy) = config.map_chip_to_global(0, 10, 20);
        // x = 1*10 + 0*20 + 100 = 110
        // y = 0*10 + 1*20 + 200 = 220
        assert_eq!((gx, gy), (110, 220));

        // Check Chip 1: Rotation 180 + Translation (300, 400)
        let (gx, gy) = config.map_chip_to_global(1, 10, 20);
        // x = -1*10 + 0*20 + 300 = 290
        // y = 0*10 + -1*20 + 400 = 380
        assert_eq!((gx, gy), (290, 380));
    }
    #[test]
    fn test_json_partial_config_frequency_only() {
        // User only wants to change frequency (common ESS use case)
        let json = r#"{
            "detector": {
                "timing": {
                    "tdc_frequency_hz": 14.0
                }
            }
        }"#;

        let config = DetectorConfig::from_json(json).expect("Should parse partial config");

        assert_eq!(config.tdc_frequency_hz, 14.0); // Changed
        assert!(config.enable_missing_tdc_correction); // Default: true
        assert_eq!(config.chip_size, 256); // Default
        assert_eq!(config.chip_transforms.len(), 4); // VENUS defaults
    }

    #[test]
    fn test_json_empty_detector() {
        // Minimal config - just use all defaults
        let json = r#"{ "detector": {} }"#;

        let config = DetectorConfig::from_json(json).expect("Should parse minimal config");

        assert_eq!(config.tdc_frequency_hz, 60.0); // VENUS default
        assert_eq!(config.chip_transforms.len(), 4); // VENUS defaults
    }

    #[test]
    fn test_json_custom_transforms_only() {
        // User only specifies chip transforms (detector swap)
        let json = r#"{
            "detector": {
                "chip_transformations": [
                    {"chip_id": 0, "matrix": [[1, 0, 260], [0, 1, 0]]}
                ]
            }
        }"#;

        let config = DetectorConfig::from_json(json).expect("Should parse");

        assert_eq!(config.tdc_frequency_hz, 60.0); // Default
        assert_eq!(config.chip_transforms[0].tx, 260); // Custom
    }
}
