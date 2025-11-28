//! Grid-based clustering algorithm.
//!
//! Uses spatial indexing to efficiently cluster hits by dividing
//! the detector into grid cells.

use crate::SpatialIndex;
use rayon::prelude::*;
use rustpix_core::{Cluster, ClusteringAlgorithm, ClusteringConfig, Hit, Result};

/// Grid-based clustering with spatial indexing.
///
/// This algorithm divides the detector into grid cells and clusters
/// hits within and across neighboring cells. It's optimized for
/// parallel processing of large datasets.
#[derive(Debug, Clone)]
pub struct GridClustering {
    /// Cell size for the grid (in pixels).
    cell_size: u16,
    /// Whether to use parallel processing.
    parallel: bool,
}

impl Default for GridClustering {
    fn default() -> Self {
        Self {
            cell_size: 16,
            parallel: true,
        }
    }
}

impl GridClustering {
    /// Creates a new grid-based clustering instance.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a grid-based clustering instance with custom cell size.
    pub fn with_cell_size(cell_size: u16) -> Self {
        Self {
            cell_size,
            ..Default::default()
        }
    }

    /// Sets whether to use parallel processing.
    pub fn with_parallel(mut self, parallel: bool) -> Self {
        self.parallel = parallel;
        self
    }
}

impl<H: Hit + Clone + Send + Sync> ClusteringAlgorithm<H> for GridClustering {
    fn cluster(&self, hits: &[H], config: &ClusteringConfig) -> Result<Vec<Cluster<H>>> {
        if hits.is_empty() {
            return Ok(Vec::new());
        }

        // Build spatial index
        let mut spatial_index = SpatialIndex::new(self.cell_size);
        spatial_index.build(hits);

        let n = hits.len();
        let epsilon_squared = (config.spatial_epsilon * config.spatial_epsilon) as u32;

        // Use union-find for merging
        let parent: Vec<std::sync::atomic::AtomicUsize> =
            (0..n).map(std::sync::atomic::AtomicUsize::new).collect();

        let find = |x: usize| -> usize {
            let mut current = x;
            loop {
                let p = parent[current].load(std::sync::atomic::Ordering::Relaxed);
                if p == current {
                    break current;
                }
                current = p;
            }
        };

        let union = |x: usize, y: usize| {
            let mut px = find(x);
            let mut py = find(y);
            while px != py {
                if px < py {
                    std::mem::swap(&mut px, &mut py);
                }
                match parent[px].compare_exchange(
                    px,
                    py,
                    std::sync::atomic::Ordering::Relaxed,
                    std::sync::atomic::Ordering::Relaxed,
                ) {
                    Ok(_) => break,
                    Err(_) => {
                        px = find(px);
                        py = find(py);
                    }
                }
            }
        };

        // Process hits
        if self.parallel {
            (0..n).into_par_iter().for_each(|i| {
                let hit_i = &hits[i];
                let neighbors = spatial_index.find_neighbors(hit_i.coord());

                for j in neighbors {
                    if j <= i {
                        continue;
                    }

                    let hit_j = &hits[j];

                    let dist_sq = hit_i.coord().distance_squared(&hit_j.coord());
                    if dist_sq > epsilon_squared {
                        continue;
                    }

                    let time_diff = hit_i.toa().abs_diff(&hit_j.toa());
                    if time_diff > config.temporal_epsilon {
                        continue;
                    }

                    union(i, j);
                }
            });
        } else {
            for i in 0..n {
                let hit_i = &hits[i];
                let neighbors = spatial_index.find_neighbors(hit_i.coord());

                for j in neighbors {
                    if j <= i {
                        continue;
                    }

                    let hit_j = &hits[j];

                    let dist_sq = hit_i.coord().distance_squared(&hit_j.coord());
                    if dist_sq > epsilon_squared {
                        continue;
                    }

                    let time_diff = hit_i.toa().abs_diff(&hit_j.toa());
                    if time_diff > config.temporal_epsilon {
                        continue;
                    }

                    union(i, j);
                }
            }
        }

        // Collect clusters
        let mut cluster_map: std::collections::HashMap<usize, Vec<usize>> =
            std::collections::HashMap::new();

        for i in 0..n {
            let root = find(i);
            cluster_map.entry(root).or_default().push(i);
        }

        let clusters: Vec<Cluster<H>> = cluster_map
            .into_values()
            .filter(|indices| {
                let size = indices.len();
                size >= config.min_cluster_size
                    && config.max_cluster_size.is_none_or(|max| size <= max)
            })
            .map(|indices| {
                indices
                    .into_iter()
                    .map(|i| hits[i].clone())
                    .collect::<Cluster<H>>()
            })
            .collect();

        Ok(clusters)
    }

    fn name(&self) -> &'static str {
        "Grid"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustpix_core::HitData;

    #[test]
    fn test_grid_single_cluster() {
        let hits = vec![
            HitData::new(0, 0, 100, 10),
            HitData::new(1, 0, 110, 15),
            HitData::new(1, 1, 105, 12),
        ];

        let algo = GridClustering::new().with_parallel(false);
        let config = ClusteringConfig::default().with_spatial_epsilon(2.0);
        let clusters = algo.cluster(&hits, &config).unwrap();

        assert_eq!(clusters.len(), 1);
        assert_eq!(clusters[0].len(), 3);
    }

    #[test]
    fn test_grid_separate_clusters() {
        let hits = vec![
            HitData::new(0, 0, 100, 10),
            HitData::new(1, 0, 110, 15),
            HitData::new(100, 100, 1000, 20),
            HitData::new(101, 100, 1010, 25),
        ];

        let algo = GridClustering::new().with_parallel(false);
        let config = ClusteringConfig::default().with_spatial_epsilon(2.0);
        let clusters = algo.cluster(&hits, &config).unwrap();

        assert_eq!(clusters.len(), 2);
    }

    #[test]
    fn test_grid_parallel() {
        let hits: Vec<HitData> = (0..1000)
            .map(|i| HitData::new((i % 256) as u16, (i / 256) as u16, i as u64, 10))
            .collect();

        let algo = GridClustering::new().with_parallel(true);
        let config = ClusteringConfig::default().with_spatial_epsilon(2.0);
        let result = algo.cluster(&hits, &config);

        assert!(result.is_ok());
    }

    #[test]
    fn test_grid_empty_input() {
        let hits: Vec<HitData> = Vec::new();
        let algo = GridClustering::new();
        let config = ClusteringConfig::default();
        let clusters = algo.cluster(&hits, &config).unwrap();

        assert!(clusters.is_empty());
    }
}
