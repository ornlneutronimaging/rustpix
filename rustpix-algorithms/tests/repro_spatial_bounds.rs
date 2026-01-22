#![allow(clippy::uninlined_format_args)]
use rustpix_algorithms::{DbscanClustering, DbscanConfig, DbscanState};
use rustpix_core::soa::HitBatch;

#[test]
fn test_clusters_outside_bounds() {
    let config = DbscanConfig {
        epsilon: 5.0,
        temporal_window_ns: 100.0,
        min_points: 2,
        min_cluster_size: 1,
    };
    let clustering = DbscanClustering::new(config);
    let mut state = DbscanState::default();

    let mut batch = HitBatch::with_capacity(20);

    // Cluster 1: (100, 100) - Within default 512x512 bounds
    for i in 0..10 {
        batch.push(100 + i, 100, 100, 0, 1, 0);
    }

    // Cluster 2: (600, 600) - Outside default bounds
    for i in 0..10 {
        batch.push(100 + i, 600, 600, 0, 1, 0);
    }

    let count = clustering.cluster(&mut batch, &mut state).unwrap();

    // Should find 2 clusters
    assert_eq!(count, 2, "Expected 2 clusters, found {}", count);

    // Verify all hits have a cluster ID != -1
    for (i, label) in batch.cluster_id.iter().enumerate() {
        assert_ne!(*label, -1, "Hit {} was classified as noise", i);
    }
}
