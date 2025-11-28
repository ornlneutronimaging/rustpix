//! Neutron extraction traits and configuration.
//!
//! See IMPLEMENTATION_PLAN.md Part 2.4 for detailed specification.

use crate::error::ExtractionError;
use crate::hit::Hit;
use crate::neutron::Neutron;

/// Configuration for neutron extraction.
#[derive(Clone, Debug)]
pub struct ExtractionConfig {
    /// Sub-pixel resolution multiplier (default: 8.0).
    pub super_resolution_factor: f64,
    /// Weight centroids by TOT values.
    pub weighted_by_tot: bool,
    /// Minimum TOT threshold (0 = disabled).
    pub min_tot_threshold: u16,
}

impl Default for ExtractionConfig {
    fn default() -> Self {
        Self {
            super_resolution_factor: 8.0,
            weighted_by_tot: true,
            min_tot_threshold: 0,
        }
    }
}

impl ExtractionConfig {
    /// Create VENUS/SNS default configuration.
    pub fn venus_defaults() -> Self {
        Self::default()
    }

    /// Set super resolution factor.
    pub fn with_super_resolution(mut self, factor: f64) -> Self {
        self.super_resolution_factor = factor;
        self
    }

    /// Set TOT weighting.
    pub fn with_weighted_by_tot(mut self, weighted: bool) -> Self {
        self.weighted_by_tot = weighted;
        self
    }

    /// Set minimum TOT threshold.
    pub fn with_min_tot_threshold(mut self, threshold: u16) -> Self {
        self.min_tot_threshold = threshold;
        self
    }
}

/// Trait for neutron extraction algorithms.
///
/// Extracts neutron events from clustered hits by computing centroids.
pub trait NeutronExtraction: Send + Sync {
    /// Algorithm name.
    fn name(&self) -> &'static str;

    /// Configure the extraction.
    fn configure(&mut self, config: ExtractionConfig);

    /// Get current configuration.
    fn config(&self) -> &ExtractionConfig;

    /// Extract neutrons from clustered hits.
    ///
    /// # Arguments
    /// * `hits` - Slice of hits (matching labels array)
    /// * `labels` - Cluster labels from clustering algorithm
    /// * `num_clusters` - Number of clusters found
    ///
    /// # Returns
    /// Vector of extracted neutrons (one per cluster).
    fn extract<H: Hit>(
        &self,
        hits: &[H],
        labels: &[i32],
        num_clusters: usize,
    ) -> Result<Vec<Neutron>, ExtractionError>;
}

/// Simple centroid extraction using TOT-weighted averages.
///
/// Algorithm (see IMPLEMENTATION_PLAN.md Part 4 - Neutron Extraction):
/// 1. Single hit: Return as-is with super-resolution scaling
/// 2. Multi-hit: Compute TOT-weighted centroid
/// 3. Representative TOF: Use TOF from hit with highest TOT
/// 4. Output scaling: Multiply by super_resolution_factor
#[derive(Clone, Debug, Default)]
pub struct SimpleCentroidExtraction {
    config: ExtractionConfig,
}

impl SimpleCentroidExtraction {
    /// Create with default configuration.
    pub fn new() -> Self {
        Self {
            config: ExtractionConfig::default(),
        }
    }

    /// Create with custom configuration.
    pub fn with_config(config: ExtractionConfig) -> Self {
        Self { config }
    }
}

impl NeutronExtraction for SimpleCentroidExtraction {
    fn name(&self) -> &'static str {
        "SimpleCentroid"
    }

    fn configure(&mut self, config: ExtractionConfig) {
        self.config = config;
    }

    fn config(&self) -> &ExtractionConfig {
        &self.config
    }

    fn extract<H: Hit>(
        &self,
        hits: &[H],
        labels: &[i32],
        num_clusters: usize,
    ) -> Result<Vec<Neutron>, ExtractionError> {
        if hits.len() != labels.len() {
            return Err(ExtractionError::LabelMismatch {
                hits: hits.len(),
                labels: labels.len(),
            });
        }

        let mut neutrons = Vec::with_capacity(num_clusters);

        // Process each cluster
        for cluster_id in 0..num_clusters as i32 {
            let cluster_indices: Vec<usize> = labels
                .iter()
                .enumerate()
                .filter(|(_, &label)| label == cluster_id)
                .map(|(idx, _)| idx)
                .collect();

            if cluster_indices.is_empty() {
                continue;
            }

            // Find hit with maximum TOT (for representative TOF)
            let max_tot_idx = cluster_indices
                .iter()
                .max_by_key(|&&idx| hits[idx].tot())
                .copied()
                .unwrap();

            let representative_tof = hits[max_tot_idx].tof();
            let representative_chip = hits[max_tot_idx].chip_id();

            // Calculate centroid
            let (centroid_x, centroid_y, total_tot) = if self.config.weighted_by_tot {
                // TOT-weighted centroid
                let mut sum_x = 0.0;
                let mut sum_y = 0.0;
                let mut sum_weight = 0.0;
                let mut sum_tot = 0u32;

                for &idx in &cluster_indices {
                    let hit = &hits[idx];
                    let weight = hit.tot() as f64;
                    sum_x += hit.x() as f64 * weight;
                    sum_y += hit.y() as f64 * weight;
                    sum_weight += weight;
                    sum_tot += hit.tot() as u32;
                }

                (
                    sum_x / sum_weight,
                    sum_y / sum_weight,
                    sum_tot.min(u16::MAX as u32) as u16,
                )
            } else {
                // Simple arithmetic mean
                let mut sum_x = 0.0;
                let mut sum_y = 0.0;
                let mut sum_tot = 0u32;
                let n = cluster_indices.len() as f64;

                for &idx in &cluster_indices {
                    let hit = &hits[idx];
                    sum_x += hit.x() as f64;
                    sum_y += hit.y() as f64;
                    sum_tot += hit.tot() as u32;
                }

                (
                    sum_x / n,
                    sum_y / n,
                    sum_tot.min(u16::MAX as u32) as u16,
                )
            };

            // Apply super-resolution scaling
            let scaled_x = centroid_x * self.config.super_resolution_factor;
            let scaled_y = centroid_y * self.config.super_resolution_factor;

            neutrons.push(Neutron::new(
                scaled_x,
                scaled_y,
                representative_tof,
                total_tot,
                cluster_indices.len() as u16,
                representative_chip,
            ));
        }

        Ok(neutrons)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hit::GenericHit;

    #[test]
    fn test_single_hit_extraction() {
        let hits = vec![GenericHit::new(1000, 100, 200, 500, 50, 0)];
        let labels = vec![0];

        let extractor = SimpleCentroidExtraction::new();
        let neutrons = extractor.extract(&hits, &labels, 1).unwrap();

        assert_eq!(neutrons.len(), 1);
        assert_eq!(neutrons[0].x, 800.0); // 100 * 8
        assert_eq!(neutrons[0].y, 1600.0); // 200 * 8
        assert_eq!(neutrons[0].tof, 1000);
        assert_eq!(neutrons[0].n_hits, 1);
    }

    #[test]
    fn test_weighted_centroid() {
        let hits = vec![
            GenericHit::new(1000, 0, 0, 500, 30, 0),  // weight 30
            GenericHit::new(1000, 2, 0, 500, 10, 0),  // weight 10
        ];
        let labels = vec![0, 0];

        let extractor = SimpleCentroidExtraction::new();
        let neutrons = extractor.extract(&hits, &labels, 1).unwrap();

        assert_eq!(neutrons.len(), 1);
        // Weighted: (0*30 + 2*10) / 40 = 0.5, scaled by 8 = 4.0
        assert!((neutrons[0].x - 4.0).abs() < 0.01);
        assert_eq!(neutrons[0].n_hits, 2);
        assert_eq!(neutrons[0].tot, 40);
    }

    #[test]
    fn test_multiple_clusters() {
        let hits = vec![
            GenericHit::new(1000, 10, 10, 500, 50, 0),
            GenericHit::new(1000, 11, 10, 500, 50, 0),
            GenericHit::new(2000, 100, 100, 500, 50, 1),
        ];
        let labels = vec![0, 0, 1];

        let extractor = SimpleCentroidExtraction::new();
        let neutrons = extractor.extract(&hits, &labels, 2).unwrap();

        assert_eq!(neutrons.len(), 2);
        assert_eq!(neutrons[0].n_hits, 2);
        assert_eq!(neutrons[1].n_hits, 1);
    }

    #[test]
    fn test_label_mismatch_error() {
        let hits = vec![GenericHit::new(1000, 100, 200, 500, 50, 0)];
        let labels = vec![0, 0]; // Wrong length

        let extractor = SimpleCentroidExtraction::new();
        let result = extractor.extract(&hits, &labels, 1);

        assert!(result.is_err());
    }
}
