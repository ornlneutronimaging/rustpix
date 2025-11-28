//! DBSCAN clustering algorithm.
//!
//! Density-Based Spatial Clustering of Applications with Noise.
//! Extended for spatio-temporal data.

use crate::SpatialIndex;
use rustpix_core::{Cluster, ClusteringAlgorithm, ClusteringConfig, Hit, Result};

/// DBSCAN clustering algorithm with spatial indexing.
///
/// This implementation uses a spatial index for efficient neighbor
/// queries, making it suitable for large datasets.
#[derive(Debug, Clone)]
pub struct DbscanClustering {
    /// Minimum number of points to form a core point.
    min_points: usize,
}

impl Default for DbscanClustering {
    fn default() -> Self {
        Self { min_points: 2 }
    }
}

impl DbscanClustering {
    /// Creates a new DBSCAN clustering instance.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a DBSCAN instance with a custom min_points value.
    pub fn with_min_points(min_points: usize) -> Self {
        Self { min_points }
    }
}

impl<H: Hit + Clone> ClusteringAlgorithm<H> for DbscanClustering {
    fn cluster(&self, hits: &[H], config: &ClusteringConfig) -> Result<Vec<Cluster<H>>> {
        if hits.is_empty() {
            return Ok(Vec::new());
        }

        let n = hits.len();
        let mut labels: Vec<Option<usize>> = vec![None; n];
        let mut cluster_id = 0;

        // Build spatial index for efficient neighbor queries
        let cell_size = (config.spatial_epsilon.ceil() as u16).max(1);
        let mut spatial_index = SpatialIndex::new(cell_size);
        spatial_index.build(hits);

        for i in 0..n {
            if labels[i].is_some() {
                continue;
            }

            // Find neighbors
            let neighbors = self.region_query(hits, i, &spatial_index, config);

            if neighbors.len() < self.min_points {
                // Noise point (for now, will be classified as singleton cluster)
                continue;
            }

            // Start a new cluster
            labels[i] = Some(cluster_id);
            let mut seeds: Vec<usize> = neighbors.into_iter().filter(|&j| j != i).collect();

            while let Some(q) = seeds.pop() {
                if labels[q].is_some() {
                    continue;
                }

                labels[q] = Some(cluster_id);

                let q_neighbors = self.region_query(hits, q, &spatial_index, config);
                if q_neighbors.len() >= self.min_points {
                    for neighbor in q_neighbors {
                        if labels[neighbor].is_none() && !seeds.contains(&neighbor) {
                            seeds.push(neighbor);
                        }
                    }
                }
            }

            cluster_id += 1;
        }

        // Collect clusters
        let mut clusters: Vec<Cluster<H>> = (0..cluster_id).map(|_| Cluster::new()).collect();

        for (i, label) in labels.iter().enumerate() {
            if let Some(cluster_idx) = label {
                clusters[*cluster_idx].push(hits[i].clone());
            }
        }

        // Also create singleton clusters for noise points if min_cluster_size is 1
        if config.min_cluster_size <= 1 {
            for (i, label) in labels.iter().enumerate() {
                if label.is_none() {
                    let mut cluster = Cluster::new();
                    cluster.push(hits[i].clone());
                    clusters.push(cluster);
                }
            }
        }

        // Apply cluster size filters
        let clusters: Vec<Cluster<H>> = clusters
            .into_iter()
            .filter(|c| {
                let size = c.len();
                size >= config.min_cluster_size
                    && config.max_cluster_size.is_none_or(|max| size <= max)
            })
            .collect();

        Ok(clusters)
    }

    fn name(&self) -> &'static str {
        "DBSCAN"
    }
}

impl DbscanClustering {
    /// Finds all neighbors of a point within the epsilon neighborhood.
    fn region_query<H: Hit>(
        &self,
        hits: &[H],
        point_idx: usize,
        spatial_index: &SpatialIndex,
        config: &ClusteringConfig,
    ) -> Vec<usize> {
        let point = &hits[point_idx];
        let candidates = spatial_index.find_neighbors(point.coord());

        let epsilon_squared = (config.spatial_epsilon * config.spatial_epsilon) as u32;

        candidates
            .into_iter()
            .filter(|&idx| {
                let candidate = &hits[idx];
                let dist_sq = point.coord().distance_squared(&candidate.coord());
                let time_diff = point.toa().abs_diff(&candidate.toa());

                dist_sq <= epsilon_squared && time_diff <= config.temporal_epsilon
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustpix_core::HitData;

    #[test]
    fn test_dbscan_single_cluster() {
        let hits = vec![
            HitData::new(0, 0, 100, 10),
            HitData::new(1, 0, 110, 15),
            HitData::new(1, 1, 105, 12),
            HitData::new(0, 1, 108, 11),
        ];

        let algo = DbscanClustering::new();
        let config = ClusteringConfig::default().with_spatial_epsilon(2.0);
        let clusters = algo.cluster(&hits, &config).unwrap();

        assert_eq!(clusters.len(), 1);
        assert_eq!(clusters[0].len(), 4);
    }

    #[test]
    fn test_dbscan_separate_clusters() {
        let hits = vec![
            HitData::new(0, 0, 100, 10),
            HitData::new(1, 0, 110, 15),
            HitData::new(100, 100, 1000, 20),
            HitData::new(101, 100, 1010, 25),
        ];

        let algo = DbscanClustering::new();
        let config = ClusteringConfig::default().with_spatial_epsilon(2.0);
        let clusters = algo.cluster(&hits, &config).unwrap();

        assert_eq!(clusters.len(), 2);
    }

    #[test]
    fn test_dbscan_empty_input() {
        let hits: Vec<HitData> = Vec::new();
        let algo = DbscanClustering::new();
        let config = ClusteringConfig::default();
        let clusters = algo.cluster(&hits, &config).unwrap();

        assert!(clusters.is_empty());
    }
}
