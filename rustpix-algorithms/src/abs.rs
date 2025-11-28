//! ABS (Adjacency-Based Search) clustering algorithm.
//!
//! This algorithm clusters hits based on spatial and temporal adjacency.
//! Hits are considered neighbors if they are within the spatial and
//! temporal epsilon thresholds.

use rustpix_core::{Cluster, ClusteringAlgorithm, ClusteringConfig, Hit, Result};

/// ABS clustering algorithm.
///
/// Clusters hits based on 8-connectivity in space and temporal proximity.
/// This is a fast algorithm suitable for well-separated events.
#[derive(Debug, Clone, Default)]
pub struct AbsClustering;

impl AbsClustering {
    /// Creates a new ABS clustering instance.
    pub fn new() -> Self {
        Self
    }
}

impl<H: Hit + Clone> ClusteringAlgorithm<H> for AbsClustering {
    fn cluster(&self, hits: &[H], config: &ClusteringConfig) -> Result<Vec<Cluster<H>>> {
        if hits.is_empty() {
            return Ok(Vec::new());
        }

        let n = hits.len();
        let mut visited = vec![false; n];
        let mut clusters = Vec::new();

        for i in 0..n {
            if visited[i] {
                continue;
            }

            visited[i] = true;
            let mut cluster = Cluster::new();
            cluster.push(hits[i].clone());

            // Find all connected hits using a stack-based approach
            let mut stack = vec![i];

            while let Some(current) = stack.pop() {
                let current_hit = &hits[current];

                for j in 0..n {
                    if visited[j] {
                        continue;
                    }

                    let candidate = &hits[j];

                    // Check spatial adjacency (8-connectivity)
                    if !current_hit.coord().is_adjacent(&candidate.coord()) {
                        continue;
                    }

                    // Check temporal proximity
                    let time_diff = current_hit.toa().abs_diff(&candidate.toa());
                    if time_diff > config.temporal_epsilon {
                        continue;
                    }

                    visited[j] = true;
                    cluster.push(candidate.clone());
                    stack.push(j);
                }
            }

            // Apply cluster size filters
            if cluster.len() >= config.min_cluster_size {
                if let Some(max_size) = config.max_cluster_size {
                    if cluster.len() <= max_size {
                        clusters.push(cluster);
                    }
                } else {
                    clusters.push(cluster);
                }
            }
        }

        Ok(clusters)
    }

    fn name(&self) -> &'static str {
        "ABS"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustpix_core::HitData;

    #[test]
    fn test_abs_single_cluster() {
        let hits = vec![
            HitData::new(0, 0, 100, 10),
            HitData::new(1, 0, 110, 15),
            HitData::new(1, 1, 105, 12),
        ];

        let algo = AbsClustering::new();
        let config = ClusteringConfig::default();
        let clusters = algo.cluster(&hits, &config).unwrap();

        assert_eq!(clusters.len(), 1);
        assert_eq!(clusters[0].len(), 3);
    }

    #[test]
    fn test_abs_separate_clusters() {
        let hits = vec![
            HitData::new(0, 0, 100, 10),
            HitData::new(1, 0, 110, 15),
            HitData::new(100, 100, 1000, 20), // Far away spatially and temporally
        ];

        let algo = AbsClustering::new();
        let config = ClusteringConfig::default();
        let clusters = algo.cluster(&hits, &config).unwrap();

        assert_eq!(clusters.len(), 2);
    }

    #[test]
    fn test_abs_temporal_separation() {
        let hits = vec![
            HitData::new(0, 0, 100, 10),
            HitData::new(1, 0, 10000, 15), // Same spatial position but far in time
        ];

        let algo = AbsClustering::new();
        let config = ClusteringConfig::default().with_temporal_epsilon(500);
        let clusters = algo.cluster(&hits, &config).unwrap();

        assert_eq!(clusters.len(), 2);
    }

    #[test]
    fn test_abs_min_cluster_size() {
        let hits = vec![
            HitData::new(0, 0, 100, 10),
            HitData::new(100, 100, 1000, 20),
        ];

        let algo = AbsClustering::new();
        let config = ClusteringConfig::default().with_min_cluster_size(2);
        let clusters = algo.cluster(&hits, &config).unwrap();

        assert!(clusters.is_empty()); // Both hits are isolated
    }

    #[test]
    fn test_abs_empty_input() {
        let hits: Vec<HitData> = Vec::new();
        let algo = AbsClustering::new();
        let config = ClusteringConfig::default();
        let clusters = algo.cluster(&hits, &config).unwrap();

        assert!(clusters.is_empty());
    }
}
