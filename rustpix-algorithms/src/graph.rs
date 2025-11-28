//! Graph-based clustering algorithm.
//!
//! Uses a union-find data structure for efficient clustering
//! of spatio-temporally connected hits.
//! See IMPLEMENTATION_PLAN.md Part 4.3 for detailed specification.

use rustpix_core::clustering::{
    ClusteringConfig, ClusteringError, ClusteringState, ClusteringStatistics, HitClustering,
};
use rustpix_core::hit::Hit;

/// Graph clustering configuration.
#[derive(Clone, Debug)]
pub struct GraphConfig {
    /// Spatial radius for edge creation (pixels).
    pub radius: f64,
    /// Temporal correlation window (nanoseconds).
    pub temporal_window_ns: f64,
    /// Minimum cluster size to keep.
    pub min_cluster_size: u16,
}

impl Default for GraphConfig {
    fn default() -> Self {
        Self {
            radius: 5.0,
            temporal_window_ns: 75.0,
            min_cluster_size: 1,
        }
    }
}

/// Graph clustering state.
pub struct GraphState {
    hits_processed: usize,
    clusters_found: usize,
    edges_created: usize,
}

impl Default for GraphState {
    fn default() -> Self {
        Self {
            hits_processed: 0,
            clusters_found: 0,
            edges_created: 0,
        }
    }
}

impl ClusteringState for GraphState {
    fn reset(&mut self) {
        self.hits_processed = 0;
        self.clusters_found = 0;
        self.edges_created = 0;
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

    fn union(&mut self, x: usize, y: usize) -> bool {
        let px = self.find(x);
        let py = self.find(y);

        if px == py {
            return false;
        }

        match self.rank[px].cmp(&self.rank[py]) {
            std::cmp::Ordering::Less => self.parent[px] = py,
            std::cmp::Ordering::Greater => self.parent[py] = px,
            std::cmp::Ordering::Equal => {
                self.parent[py] = px;
                self.rank[px] += 1;
            }
        }
        true
    }
}

/// Graph-based clustering using union-find.
///
/// TODO: Full implementation in IMPLEMENTATION_PLAN.md Part 4.3
pub struct GraphClustering {
    config: GraphConfig,
    generic_config: ClusteringConfig,
}

impl GraphClustering {
    /// Create with custom configuration.
    pub fn new(config: GraphConfig) -> Self {
        let generic_config = ClusteringConfig {
            radius: config.radius,
            temporal_window_ns: config.temporal_window_ns,
            min_cluster_size: config.min_cluster_size,
            max_cluster_size: None,
        };
        Self {
            config,
            generic_config,
        }
    }
}

impl Default for GraphClustering {
    fn default() -> Self {
        Self::new(GraphConfig::default())
    }
}

impl HitClustering for GraphClustering {
    type State = GraphState;

    fn name(&self) -> &'static str {
        "Graph"
    }

    fn create_state(&self) -> Self::State {
        GraphState::default()
    }

    fn configure(&mut self, config: &ClusteringConfig) {
        self.config.radius = config.radius;
        self.config.temporal_window_ns = config.temporal_window_ns;
        self.generic_config = config.clone();
    }

    fn config(&self) -> &ClusteringConfig {
        &self.generic_config
    }

    fn cluster<H: Hit>(
        &self,
        hits: &[H],
        state: &mut Self::State,
        labels: &mut [i32],
    ) -> Result<usize, ClusteringError> {
        // TODO: Implement graph clustering algorithm
        // See IMPLEMENTATION_PLAN.md Part 4.3 for full specification
        //
        // Algorithm outline:
        // 1. Build union-find structure
        // 2. For each pair of hits within spatial/temporal bounds:
        //    - Union them in the structure
        // 3. Collect connected components
        // 4. Assign cluster labels

        if hits.is_empty() {
            return Ok(0);
        }

        let n = hits.len();
        let mut uf = UnionFind::new(n);
        let epsilon_sq = (self.config.radius * self.config.radius) as i32;
        let window_tof = (self.config.temporal_window_ns / 25.0).ceil() as u32;
        let mut edges = 0;

        // Build edges between neighboring hits
        for i in 0..n {
            for j in (i + 1)..n {
                let hi = &hits[i];
                let hj = &hits[j];

                // Check spatial proximity
                let dx = hi.x() as i32 - hj.x() as i32;
                let dy = hi.y() as i32 - hj.y() as i32;
                let dist_sq = dx * dx + dy * dy;
                if dist_sq > epsilon_sq {
                    continue;
                }

                // Check temporal proximity
                let time_diff = hi.tof().abs_diff(hj.tof());
                if time_diff > window_tof {
                    continue;
                }

                if uf.union(i, j) {
                    edges += 1;
                }
            }
        }

        // Collect clusters by root
        let mut cluster_map: std::collections::HashMap<usize, i32> =
            std::collections::HashMap::new();
        let mut next_cluster = 0i32;

        for i in 0..n {
            let root = uf.find(i);
            let cluster_id = *cluster_map.entry(root).or_insert_with(|| {
                let id = next_cluster;
                next_cluster += 1;
                id
            });
            labels[i] = cluster_id;
        }

        state.hits_processed = n;
        state.clusters_found = next_cluster as usize;
        state.edges_created = edges;

        Ok(state.clusters_found)
    }

    fn statistics(&self, state: &Self::State) -> ClusteringStatistics {
        ClusteringStatistics {
            hits_processed: state.hits_processed,
            clusters_found: state.clusters_found,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_graph_config_defaults() {
        let config = GraphConfig::default();
        assert_eq!(config.radius, 5.0);
        assert_eq!(config.temporal_window_ns, 75.0);
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

    #[test]
    fn test_graph_state_reset() {
        let mut state = GraphState::default();
        state.hits_processed = 100;
        state.clusters_found = 10;
        state.edges_created = 50;
        state.reset();
        assert_eq!(state.hits_processed, 0);
        assert_eq!(state.clusters_found, 0);
        assert_eq!(state.edges_created, 0);
    }
}
