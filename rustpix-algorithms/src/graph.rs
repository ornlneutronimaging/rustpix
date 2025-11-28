//! Graph-based clustering algorithm.
//!
//! Uses a union-find data structure for efficient clustering
//! of spatio-temporally connected hits.

use rustpix_core::{Cluster, ClusteringAlgorithm, ClusteringConfig, Hit, Result};

/// Graph-based clustering using union-find.
///
/// This algorithm builds a graph where edges connect neighboring hits
/// and uses union-find to efficiently identify connected components.
#[derive(Debug, Clone, Default)]
pub struct GraphClustering;

impl GraphClustering {
    /// Creates a new graph-based clustering instance.
    pub fn new() -> Self {
        Self
    }
}

/// Union-Find data structure for connected component detection.
struct UnionFind {
    parent: Vec<usize>,
    rank: Vec<usize>,
}

impl UnionFind {
    fn new(n: usize) -> Self {
        Self {
            parent: (0..n).collect(),
            rank: vec![0; n],
        }
    }

    fn find(&mut self, x: usize) -> usize {
        if self.parent[x] != x {
            self.parent[x] = self.find(self.parent[x]);
        }
        self.parent[x]
    }

    fn union(&mut self, x: usize, y: usize) {
        let px = self.find(x);
        let py = self.find(y);

        if px == py {
            return;
        }

        match self.rank[px].cmp(&self.rank[py]) {
            std::cmp::Ordering::Less => self.parent[px] = py,
            std::cmp::Ordering::Greater => self.parent[py] = px,
            std::cmp::Ordering::Equal => {
                self.parent[py] = px;
                self.rank[px] += 1;
            }
        }
    }
}

impl<H: Hit + Clone> ClusteringAlgorithm<H> for GraphClustering {
    fn cluster(&self, hits: &[H], config: &ClusteringConfig) -> Result<Vec<Cluster<H>>> {
        if hits.is_empty() {
            return Ok(Vec::new());
        }

        let n = hits.len();
        let mut uf = UnionFind::new(n);
        let epsilon_squared = (config.spatial_epsilon * config.spatial_epsilon) as u32;

        // Build edges between neighboring hits
        for i in 0..n {
            for j in (i + 1)..n {
                let hit_i = &hits[i];
                let hit_j = &hits[j];

                // Check spatial proximity
                let dist_sq = hit_i.coord().distance_squared(&hit_j.coord());
                if dist_sq > epsilon_squared {
                    continue;
                }

                // Check temporal proximity
                let time_diff = hit_i.toa().abs_diff(&hit_j.toa());
                if time_diff > config.temporal_epsilon {
                    continue;
                }

                uf.union(i, j);
            }
        }

        // Group hits by their root
        let mut cluster_map: std::collections::HashMap<usize, Vec<usize>> =
            std::collections::HashMap::new();

        for i in 0..n {
            let root = uf.find(i);
            cluster_map.entry(root).or_default().push(i);
        }

        // Convert to clusters and apply filters
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
        "Graph"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustpix_core::HitData;

    #[test]
    fn test_graph_single_cluster() {
        let hits = vec![
            HitData::new(0, 0, 100, 10),
            HitData::new(1, 0, 110, 15),
            HitData::new(1, 1, 105, 12),
        ];

        let algo = GraphClustering::new();
        let config = ClusteringConfig::default().with_spatial_epsilon(2.0);
        let clusters = algo.cluster(&hits, &config).unwrap();

        assert_eq!(clusters.len(), 1);
        assert_eq!(clusters[0].len(), 3);
    }

    #[test]
    fn test_graph_separate_clusters() {
        let hits = vec![
            HitData::new(0, 0, 100, 10),
            HitData::new(1, 0, 110, 15),
            HitData::new(100, 100, 1000, 20),
            HitData::new(101, 100, 1010, 25),
        ];

        let algo = GraphClustering::new();
        let config = ClusteringConfig::default().with_spatial_epsilon(2.0);
        let clusters = algo.cluster(&hits, &config).unwrap();

        assert_eq!(clusters.len(), 2);
    }

    #[test]
    fn test_graph_empty_input() {
        let hits: Vec<HitData> = Vec::new();
        let algo = GraphClustering::new();
        let config = ClusteringConfig::default();
        let clusters = algo.cluster(&hits, &config).unwrap();

        assert!(clusters.is_empty());
    }

    #[test]
    fn test_union_find() {
        let mut uf = UnionFind::new(5);
        uf.union(0, 1);
        uf.union(2, 3);
        uf.union(1, 2);

        assert_eq!(uf.find(0), uf.find(3));
        assert_ne!(uf.find(0), uf.find(4));
    }
}
