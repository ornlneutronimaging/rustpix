//! Centroid extraction traits and types.

use crate::{Cluster, Error, Hit, Result, TimeOfArrival};

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Extracted centroid data from a cluster.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Centroid {
    /// Centroid X coordinate (sub-pixel precision).
    pub x: f64,
    /// Centroid Y coordinate (sub-pixel precision).
    pub y: f64,
    /// Time of arrival.
    pub toa: TimeOfArrival,
    /// Total time over threshold (sum of all hits).
    pub tot_sum: u32,
    /// Number of hits in the cluster.
    pub cluster_size: u16,
}

impl Centroid {
    /// Creates a new centroid.
    pub fn new(x: f64, y: f64, toa: u64, tot_sum: u32, cluster_size: u16) -> Self {
        Self {
            x,
            y,
            toa: TimeOfArrival::new(toa),
            tot_sum,
            cluster_size,
        }
    }
}

/// Configuration for centroid extraction.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct ExtractionConfig {
    /// Whether to weight positions by ToT (charge weighting).
    pub tot_weighted: bool,
    /// Whether to compute weighted average ToA.
    pub weighted_toa: bool,
}

impl Default for ExtractionConfig {
    fn default() -> Self {
        Self {
            tot_weighted: true,
            weighted_toa: true,
        }
    }
}

impl ExtractionConfig {
    /// Creates a new extraction configuration with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets ToT weighting.
    pub fn with_tot_weighted(mut self, weighted: bool) -> Self {
        self.tot_weighted = weighted;
        self
    }

    /// Sets ToA weighting.
    pub fn with_weighted_toa(mut self, weighted: bool) -> Self {
        self.weighted_toa = weighted;
        self
    }
}

/// Trait for centroid extraction algorithms.
///
/// Centroid extractors compute the position, time, and intensity
/// of a detection event from a cluster of hits.
pub trait CentroidExtractor<H: Hit>: Send + Sync {
    /// Extracts a centroid from a cluster.
    fn extract(&self, cluster: &Cluster<H>, config: &ExtractionConfig) -> Result<Centroid>;

    /// Extracts centroids from multiple clusters.
    fn extract_all(
        &self,
        clusters: &[Cluster<H>],
        config: &ExtractionConfig,
    ) -> Result<Vec<Centroid>> {
        clusters.iter().map(|c| self.extract(c, config)).collect()
    }
}

/// Simple centroid extractor using weighted averages.
#[derive(Debug, Clone, Default)]
pub struct WeightedCentroidExtractor;

impl WeightedCentroidExtractor {
    /// Creates a new weighted centroid extractor.
    pub fn new() -> Self {
        Self
    }
}

impl<H: Hit> CentroidExtractor<H> for WeightedCentroidExtractor {
    fn extract(&self, cluster: &Cluster<H>, config: &ExtractionConfig) -> Result<Centroid> {
        if cluster.is_empty() {
            return Err(Error::EmptyCluster);
        }

        let mut sum_x = 0.0;
        let mut sum_y = 0.0;
        let mut sum_toa: u64 = 0;
        let mut tot_sum: u32 = 0;
        let mut weight_sum = 0.0;

        for hit in cluster.iter() {
            let weight = if config.tot_weighted {
                hit.tot() as f64
            } else {
                1.0
            };

            sum_x += hit.x() as f64 * weight;
            sum_y += hit.y() as f64 * weight;
            weight_sum += weight;

            if config.weighted_toa {
                sum_toa += (hit.toa_raw() as f64 * weight) as u64;
            } else {
                sum_toa += hit.toa_raw();
            }

            tot_sum += hit.tot() as u32;
        }

        let cluster_size = cluster.len() as u16;
        let centroid_x = sum_x / weight_sum;
        let centroid_y = sum_y / weight_sum;
        let avg_toa = if config.weighted_toa {
            (sum_toa as f64 / weight_sum) as u64
        } else {
            sum_toa / cluster_size as u64
        };

        Ok(Centroid::new(
            centroid_x,
            centroid_y,
            avg_toa,
            tot_sum,
            cluster_size,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::HitData;

    #[test]
    fn test_weighted_centroid_extraction() {
        let mut cluster = Cluster::new();
        cluster.push(HitData::new(0, 0, 100, 10));
        cluster.push(HitData::new(2, 0, 100, 10));

        let extractor = WeightedCentroidExtractor::new();
        let config = ExtractionConfig::new().with_tot_weighted(true);

        let centroid = extractor.extract(&cluster, &config).unwrap();

        // Equal weights, so centroid should be at (1, 0)
        assert!((centroid.x - 1.0).abs() < f64::EPSILON);
        assert!((centroid.y - 0.0).abs() < f64::EPSILON);
        assert_eq!(centroid.tot_sum, 20);
        assert_eq!(centroid.cluster_size, 2);
    }

    #[test]
    fn test_unweighted_centroid_extraction() {
        let mut cluster = Cluster::new();
        cluster.push(HitData::new(0, 0, 100, 30));
        cluster.push(HitData::new(2, 0, 100, 10));

        let extractor = WeightedCentroidExtractor::new();
        let config = ExtractionConfig::new().with_tot_weighted(false);

        let centroid = extractor.extract(&cluster, &config).unwrap();

        // Unweighted, so centroid should be at (1, 0)
        assert!((centroid.x - 1.0).abs() < f64::EPSILON);
        assert!((centroid.y - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_weighted_asymmetric() {
        let mut cluster = Cluster::new();
        cluster.push(HitData::new(0, 0, 100, 30)); // weight 30
        cluster.push(HitData::new(2, 0, 100, 10)); // weight 10

        let extractor = WeightedCentroidExtractor::new();
        let config = ExtractionConfig::new().with_tot_weighted(true);

        let centroid = extractor.extract(&cluster, &config).unwrap();

        // Weighted average: (0*30 + 2*10) / 40 = 0.5
        assert!((centroid.x - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn test_empty_cluster_error() {
        let cluster: Cluster<HitData> = Cluster::new();
        let extractor = WeightedCentroidExtractor::new();
        let config = ExtractionConfig::default();

        let result = extractor.extract(&cluster, &config);
        assert!(result.is_err());
    }
}
