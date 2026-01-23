//! Neutron extraction traits and configuration.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_lossless,
    clippy::missing_errors_doc,
    clippy::doc_markdown,
    clippy::cast_possible_wrap
)]
//!

use crate::error::ExtractionError;
use crate::neutron::{Neutron, NeutronBatch};

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
            min_tot_threshold: 10,
        }
    }
}

impl ExtractionConfig {
    /// Create VENUS/SNS default configuration.
    #[must_use]
    pub fn venus_defaults() -> Self {
        Self::default()
    }

    /// Set super resolution factor.
    #[must_use]
    pub fn with_super_resolution(mut self, factor: f64) -> Self {
        self.super_resolution_factor = factor;
        self
    }

    /// Set TOT weighting.
    #[must_use]
    pub fn with_weighted_by_tot(mut self, weighted: bool) -> Self {
        self.weighted_by_tot = weighted;
        self
    }

    /// Set minimum TOT threshold.
    #[must_use]
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

    /// Extract neutrons from a HitBatch using SoA layout.
    ///
    /// This implementation is optimized for SoA and uses a single pass over the hits
    /// (O(N) + O(C)) rather than iterating by cluster (O(N*C)).
    fn extract_soa(
        &self,
        batch: &crate::soa::HitBatch,
        num_clusters: usize,
    ) -> Result<Vec<Neutron>, ExtractionError>;
}

/// Simple centroid extraction using TOT-weighted averages.
///
/// 1. Single hit: Return as-is with super-resolution scaling
/// 2. Multi-hit: Compute TOT-weighted centroid
/// 3. Representative TOF: Use TOF from hit with highest TOT
/// 4. Output scaling: Multiply by super_resolution_factor
#[derive(Clone, Debug, Default)]
struct ClusterAccumulator {
    sum_x: f64,
    sum_y: f64,
    raw_sum_x: f64,
    raw_sum_y: f64,
    sum_tot: u64,
    count: u32,
    max_tot: u16,
    rep_tof: u32,
    rep_chip: u8,
}

/// Simple centroid extraction using TOT-weighted averages.
///
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
    #[must_use]
    pub fn new() -> Self {
        Self {
            config: ExtractionConfig::default(),
        }
    }

    /// Create with custom configuration.
    #[must_use]
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

    fn extract_soa(
        &self,
        batch: &crate::soa::HitBatch,
        num_clusters: usize,
    ) -> Result<Vec<Neutron>, ExtractionError> {
        let mut accumulators = vec![ClusterAccumulator::default(); num_clusters];
        let labels = &batch.cluster_id;
        let x_values = &batch.x;
        let y_values = &batch.y;
        let time_of_flight = &batch.tof;
        let time_over_threshold = &batch.tot;
        let chip_ids = &batch.chip_id;

        if self.config.weighted_by_tot {
            accumulate_weighted(
                &mut accumulators,
                labels,
                x_values,
                y_values,
                time_over_threshold,
                time_of_flight,
                chip_ids,
                num_clusters,
                self.config.min_tot_threshold,
            );
            Ok(build_neutrons_weighted(
                accumulators,
                self.config.super_resolution_factor,
            ))
        } else {
            accumulate_unweighted(
                &mut accumulators,
                labels,
                x_values,
                y_values,
                time_over_threshold,
                time_of_flight,
                chip_ids,
                num_clusters,
                self.config.min_tot_threshold,
            );
            Ok(build_neutrons_unweighted(
                accumulators,
                self.config.super_resolution_factor,
            ))
        }
    }
}

impl SimpleCentroidExtraction {
    pub fn extract_soa_batch(
        &self,
        batch: &crate::soa::HitBatch,
        num_clusters: usize,
    ) -> Result<NeutronBatch, ExtractionError> {
        let mut accumulators = vec![ClusterAccumulator::default(); num_clusters];
        let labels = &batch.cluster_id;
        let x_values = &batch.x;
        let y_values = &batch.y;
        let time_of_flight = &batch.tof;
        let time_over_threshold = &batch.tot;
        let chip_ids = &batch.chip_id;

        if self.config.weighted_by_tot {
            accumulate_weighted(
                &mut accumulators,
                labels,
                x_values,
                y_values,
                time_over_threshold,
                time_of_flight,
                chip_ids,
                num_clusters,
                self.config.min_tot_threshold,
            );
            Ok(build_neutron_batch_weighted(
                accumulators,
                self.config.super_resolution_factor,
            ))
        } else {
            accumulate_unweighted(
                &mut accumulators,
                labels,
                x_values,
                y_values,
                time_over_threshold,
                time_of_flight,
                chip_ids,
                num_clusters,
                self.config.min_tot_threshold,
            );
            Ok(build_neutron_batch_unweighted(
                accumulators,
                self.config.super_resolution_factor,
            ))
        }
    }
}

#[allow(clippy::cast_sign_loss)]
#[inline]
fn cluster_index(label: i32, num_clusters: usize) -> Option<usize> {
    if label < 0 {
        return None;
    }
    let idx = label as usize;
    if idx >= num_clusters {
        None
    } else {
        Some(idx)
    }
}

fn accumulate_weighted(
    accumulators: &mut [ClusterAccumulator],
    labels: &[i32],
    x_values: &[u16],
    y_values: &[u16],
    tot_values: &[u16],
    tof_values: &[u32],
    chip_ids: &[u8],
    num_clusters: usize,
    min_tot: u16,
) {
    if min_tot > 0 {
        for i in 0..labels.len() {
            let Some(cluster_idx) = cluster_index(labels[i], num_clusters) else {
                continue;
            };
            let tot = tot_values[i];
            if tot < min_tot {
                continue;
            }

            let acc = &mut accumulators[cluster_idx];
            let x = x_values[i] as f64;
            let y = y_values[i] as f64;
            let weight = tot as f64;

            acc.count += 1;
            acc.sum_tot += tot as u64;
            acc.raw_sum_x += x;
            acc.raw_sum_y += y;
            acc.sum_x += x * weight;
            acc.sum_y += y * weight;

            if tot >= acc.max_tot {
                acc.max_tot = tot;
                acc.rep_tof = tof_values[i];
                acc.rep_chip = chip_ids[i];
            }
        }
    } else {
        for i in 0..labels.len() {
            let Some(cluster_idx) = cluster_index(labels[i], num_clusters) else {
                continue;
            };
            let tot = tot_values[i];
            let acc = &mut accumulators[cluster_idx];
            let x = x_values[i] as f64;
            let y = y_values[i] as f64;
            let weight = tot as f64;

            acc.count += 1;
            acc.sum_tot += tot as u64;
            acc.raw_sum_x += x;
            acc.raw_sum_y += y;
            acc.sum_x += x * weight;
            acc.sum_y += y * weight;

            if tot >= acc.max_tot {
                acc.max_tot = tot;
                acc.rep_tof = tof_values[i];
                acc.rep_chip = chip_ids[i];
            }
        }
    }
}

fn accumulate_unweighted(
    accumulators: &mut [ClusterAccumulator],
    labels: &[i32],
    x_values: &[u16],
    y_values: &[u16],
    tot_values: &[u16],
    tof_values: &[u32],
    chip_ids: &[u8],
    num_clusters: usize,
    min_tot: u16,
) {
    if min_tot > 0 {
        for i in 0..labels.len() {
            let Some(cluster_idx) = cluster_index(labels[i], num_clusters) else {
                continue;
            };
            let tot = tot_values[i];
            if tot < min_tot {
                continue;
            }

            let acc = &mut accumulators[cluster_idx];
            let x = x_values[i] as f64;
            let y = y_values[i] as f64;

            acc.count += 1;
            acc.sum_tot += tot as u64;
            acc.raw_sum_x += x;
            acc.raw_sum_y += y;

            if tot >= acc.max_tot {
                acc.max_tot = tot;
                acc.rep_tof = tof_values[i];
                acc.rep_chip = chip_ids[i];
            }
        }
    } else {
        for i in 0..labels.len() {
            let Some(cluster_idx) = cluster_index(labels[i], num_clusters) else {
                continue;
            };
            let tot = tot_values[i];

            let acc = &mut accumulators[cluster_idx];
            let x = x_values[i] as f64;
            let y = y_values[i] as f64;

            acc.count += 1;
            acc.sum_tot += tot as u64;
            acc.raw_sum_x += x;
            acc.raw_sum_y += y;

            if tot >= acc.max_tot {
                acc.max_tot = tot;
                acc.rep_tof = tof_values[i];
                acc.rep_chip = chip_ids[i];
            }
        }
    }
}

fn build_neutrons_weighted(
    accumulators: Vec<ClusterAccumulator>,
    scale: f64,
) -> Vec<Neutron> {
    let mut neutrons = Vec::with_capacity(accumulators.len());
    for acc in accumulators {
        if acc.count == 0 {
            continue;
        }

        let (centroid_x, centroid_y) = if acc.sum_tot > 0 {
            let sum_weight = acc.sum_tot as f64;
            (acc.sum_x / sum_weight, acc.sum_y / sum_weight)
        } else {
            (
                acc.raw_sum_x / acc.count as f64,
                acc.raw_sum_y / acc.count as f64,
            )
        };

        let scaled_x = centroid_x * scale;
        let scaled_y = centroid_y * scale;

        neutrons.push(Neutron::new(
            scaled_x,
            scaled_y,
            acc.rep_tof,
            acc.sum_tot.min(u16::MAX as u64) as u16,
            acc.count as u16,
            acc.rep_chip,
        ));
    }
    neutrons
}

fn build_neutrons_unweighted(
    accumulators: Vec<ClusterAccumulator>,
    scale: f64,
) -> Vec<Neutron> {
    let mut neutrons = Vec::with_capacity(accumulators.len());
    for acc in accumulators {
        if acc.count == 0 {
            continue;
        }

        let centroid_x = acc.raw_sum_x / acc.count as f64;
        let centroid_y = acc.raw_sum_y / acc.count as f64;

        let scaled_x = centroid_x * scale;
        let scaled_y = centroid_y * scale;

        neutrons.push(Neutron::new(
            scaled_x,
            scaled_y,
            acc.rep_tof,
            acc.sum_tot.min(u16::MAX as u64) as u16,
            acc.count as u16,
            acc.rep_chip,
        ));
    }
    neutrons
}

fn build_neutron_batch_weighted(
    accumulators: Vec<ClusterAccumulator>,
    scale: f64,
) -> NeutronBatch {
    let mut batch = NeutronBatch::with_capacity(accumulators.len());
    for acc in accumulators {
        if acc.count == 0 {
            continue;
        }

        let (centroid_x, centroid_y) = if acc.sum_tot > 0 {
            let sum_weight = acc.sum_tot as f64;
            (acc.sum_x / sum_weight, acc.sum_y / sum_weight)
        } else {
            (
                acc.raw_sum_x / acc.count as f64,
                acc.raw_sum_y / acc.count as f64,
            )
        };

        batch.push(Neutron::new(
            centroid_x * scale,
            centroid_y * scale,
            acc.rep_tof,
            acc.sum_tot.min(u16::MAX as u64) as u16,
            acc.count as u16,
            acc.rep_chip,
        ));
    }
    batch
}

fn build_neutron_batch_unweighted(
    accumulators: Vec<ClusterAccumulator>,
    scale: f64,
) -> NeutronBatch {
    let mut batch = NeutronBatch::with_capacity(accumulators.len());
    for acc in accumulators {
        if acc.count == 0 {
            continue;
        }

        let centroid_x = acc.raw_sum_x / acc.count as f64;
        let centroid_y = acc.raw_sum_y / acc.count as f64;

        batch.push(Neutron::new(
            centroid_x * scale,
            centroid_y * scale,
            acc.rep_tof,
            acc.sum_tot.min(u16::MAX as u64) as u16,
            acc.count as u16,
            acc.rep_chip,
        ));
    }
    batch
}

#[cfg(test)]
mod tests {
    #![allow(clippy::float_cmp)]
    use super::*;
    use crate::soa::HitBatch;

    fn make_batch(hits: &[(u32, u16, u16, u32, u16, u8, i32)]) -> HitBatch {
        let mut batch = HitBatch::with_capacity(hits.len());
        for (i, (tof, x, y, timestamp, tot, chip_id, cluster_id)) in hits.iter().enumerate() {
            batch.push(*x, *y, *tof, *tot, *timestamp, *chip_id);
            batch.cluster_id[i] = *cluster_id;
        }
        batch
    }

    #[test]
    fn test_single_hit_extraction() {
        let batch = make_batch(&[(1000, 100, 200, 500, 50, 0, 0)]);

        let extractor = SimpleCentroidExtraction::new();
        let neutrons = extractor.extract_soa(&batch, 1).unwrap();

        assert_eq!(neutrons.len(), 1);
        assert_eq!(neutrons[0].x, 800.0); // 100 * 8
        assert_eq!(neutrons[0].y, 1600.0); // 200 * 8
        assert_eq!(neutrons[0].tof, 1000);
        assert_eq!(neutrons[0].n_hits, 1);
    }

    #[test]
    fn test_weighted_centroid() {
        let batch = make_batch(&[
            (1000, 0, 0, 500, 30, 0, 0), // weight 30
            (1000, 2, 0, 500, 10, 0, 0), // weight 10
        ]);

        let extractor = SimpleCentroidExtraction::new();
        let neutrons = extractor.extract_soa(&batch, 1).unwrap();

        assert_eq!(neutrons.len(), 1);
        // Weighted: (0*30 + 2*10) / 40 = 0.5, scaled by 8 = 4.0
        assert!((neutrons[0].x - 4.0).abs() < 0.01);
        assert_eq!(neutrons[0].n_hits, 2);
        assert_eq!(neutrons[0].tot, 40);
    }

    #[test]
    fn test_multiple_clusters() {
        let batch = make_batch(&[
            (1000, 10, 10, 500, 50, 0, 0),
            (1000, 11, 10, 500, 50, 0, 0),
            (2000, 100, 100, 500, 50, 1, 1),
        ]);

        let extractor = SimpleCentroidExtraction::new();
        let neutrons = extractor.extract_soa(&batch, 2).unwrap();

        assert_eq!(neutrons.len(), 2);
        assert_eq!(neutrons[0].n_hits, 2);
        assert_eq!(neutrons[1].n_hits, 1);
    }

    #[test]
    fn test_tot_threshold_filters_low_tot_hits() {
        // Create hits with varying TOT values
        let batch = make_batch(&[
            (1000, 0, 0, 500, 5, 0, 0),   // TOT=5, below threshold
            (1000, 10, 0, 500, 15, 0, 0), // TOT=15, above threshold
            (1000, 20, 0, 500, 20, 0, 0), // TOT=20, above threshold
        ]);

        // Default threshold is 10
        let extractor = SimpleCentroidExtraction::new();
        let neutrons = extractor.extract_soa(&batch, 1).unwrap();

        assert_eq!(neutrons.len(), 1);
        // Only hits with TOT >= 10 should be included (2 hits)
        assert_eq!(neutrons[0].n_hits, 2);
        // TOT sum should be 15 + 20 = 35 (not including the TOT=5 hit)
        assert_eq!(neutrons[0].tot, 35);
        // Centroid should be weighted by (10, 0) with TOT=15 and (20, 0) with TOT=20
        // weighted_x = (10*15 + 20*20) / (15 + 20) = (150 + 400) / 35 = 15.71
        // scaled by 8 = 125.71
        assert!((neutrons[0].x - 125.71).abs() < 0.1);
    }

    #[test]
    fn test_tot_threshold_skips_empty_clusters_after_filtering() {
        // All hits in cluster have TOT below threshold
        let batch = make_batch(&[
            (1000, 0, 0, 500, 5, 0, 0), // TOT=5, below default threshold of 10
            (1000, 1, 0, 500, 8, 0, 0), // TOT=8, below threshold
        ]);

        let extractor = SimpleCentroidExtraction::new();
        let neutrons = extractor.extract_soa(&batch, 1).unwrap();

        // Cluster should be skipped because all hits are filtered out
        assert_eq!(neutrons.len(), 0);
    }

    #[test]
    fn test_tot_threshold_disabled_when_zero() {
        let batch = make_batch(&[
            (1000, 0, 0, 500, 5, 0, 0),  // TOT=5
            (1000, 10, 0, 500, 3, 0, 0), // TOT=3
        ]);

        // Disable TOT filtering by setting threshold to 0
        let mut extractor = SimpleCentroidExtraction::new();
        extractor.configure(ExtractionConfig::default().with_min_tot_threshold(0));

        let neutrons = extractor.extract_soa(&batch, 1).unwrap();

        // All hits should be included
        assert_eq!(neutrons.len(), 1);
        assert_eq!(neutrons[0].n_hits, 2);
        assert_eq!(neutrons[0].tot, 8); // 5 + 3
    }

    #[test]
    fn test_representative_tof_from_max_tot_after_filtering() {
        // Verify that representative TOF is selected from remaining hits after filtering
        let batch = make_batch(&[
            (1000, 0, 0, 500, 5, 0, 0),   // TOT=5, filtered out, TOF=1000
            (2000, 10, 0, 500, 15, 0, 0), // TOT=15, kept, TOF=2000
            (3000, 20, 0, 500, 25, 0, 0), // TOT=25 (max), kept, TOF=3000
        ]);

        let extractor = SimpleCentroidExtraction::new();
        let neutrons = extractor.extract_soa(&batch, 1).unwrap();

        assert_eq!(neutrons.len(), 1);
        // Representative TOF should be from the hit with max TOT (25), which is 3000
        assert_eq!(neutrons[0].tof, 3000);
        // Verify it's not using the filtered hit's TOF
        assert_ne!(neutrons[0].tof, 1000);
    }

    #[test]
    fn test_zero_tot_weighted_centroid() {
        // All hits have TOT = 0, which would cause divide-by-zero without the fix
        let batch = make_batch(&[
            (1000, 10, 20, 500, 0, 0, 0), // TOT = 0
            (1000, 30, 40, 500, 0, 0, 0), // TOT = 0
        ]);

        // Disable TOT filtering so zero-TOT hits aren't filtered out
        let mut extractor = SimpleCentroidExtraction::new();
        extractor.configure(ExtractionConfig::default().with_min_tot_threshold(0));

        let neutrons = extractor.extract_soa(&batch, 1).unwrap();

        assert_eq!(neutrons.len(), 1);
        // Should fall back to arithmetic mean: (10+30)/2 = 20, (20+40)/2 = 30
        // Scaled by 8: 160, 240
        assert!((neutrons[0].x - 160.0).abs() < 0.01);
        assert!((neutrons[0].y - 240.0).abs() < 0.01);
        assert_eq!(neutrons[0].tot, 0);
        assert_eq!(neutrons[0].n_hits, 2);
        // Verify no NaN
        assert!(!neutrons[0].x.is_nan());
        assert!(!neutrons[0].y.is_nan());
    }

    #[test]
    fn test_extract_soa_expected_values() {
        let mut batch = HitBatch::with_capacity(3);
        // Cluster 0: weighted centroid, max TOT hit provides TOF/chip_id
        batch.push(10, 10, 1000, 20, 500, 0);
        batch.push(20, 10, 1500, 10, 500, 0);
        // Cluster 1: single hit
        batch.push(5, 7, 2000, 15, 500, 1);

        batch.cluster_id[0] = 0;
        batch.cluster_id[1] = 0;
        batch.cluster_id[2] = 1;

        let extractor = SimpleCentroidExtraction::new();
        let neutrons = extractor.extract_soa(&batch, 2).unwrap();

        assert_eq!(neutrons.len(), 2);

        let n0 = &neutrons[0];
        let expected_x = (10.0 * 20.0 + 20.0 * 10.0) / 30.0 * 8.0;
        let expected_y = 10.0 * 8.0;
        assert!((n0.x - expected_x).abs() < 1e-6);
        assert!((n0.y - expected_y).abs() < 1e-6);
        assert_eq!(n0.tof, 1000);
        assert_eq!(n0.tot, 30);
        assert_eq!(n0.n_hits, 2);
        assert_eq!(n0.chip_id, 0);

        let n1 = &neutrons[1];
        assert!((n1.x - 40.0).abs() < 1e-6);
        assert!((n1.y - 56.0).abs() < 1e-6);
        assert_eq!(n1.tof, 2000);
        assert_eq!(n1.tot, 15);
        assert_eq!(n1.n_hits, 1);
        assert_eq!(n1.chip_id, 1);
    }
}
