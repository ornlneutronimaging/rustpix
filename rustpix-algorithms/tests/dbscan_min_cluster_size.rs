use rustpix_algorithms::{DbscanClustering, DbscanConfig, DbscanState};
use rustpix_core::soa::HitBatch;

#[test]
fn test_dbscan_min_cluster_size_ignored() {
    let mut batch = HitBatch::with_capacity(10);

    // Cluster 1: 5 points (Dense enough, Large enough)
    // Coords: (10,10), (10,11), (10,12), (11,10), (11,11)
    // All within epsilon=2.0
    batch.push((10, 10, 100, 1, 0, 0));
    batch.push((10, 11, 100, 1, 0, 0));
    batch.push((10, 12, 100, 1, 0, 0));
    batch.push((11, 10, 100, 1, 0, 0));
    batch.push((11, 11, 100, 1, 0, 0));

    // Cluster 2: 3 points (Dense enough, but Too Small)
    // Coords: (50,50), (50,51), (50,52)
    batch.push((50, 50, 200, 1, 0, 0));
    batch.push((50, 51, 200, 1, 0, 0));
    batch.push((50, 52, 200, 1, 0, 0));

    let config = DbscanConfig {
        epsilon: 3.0,
        temporal_window_ns: 50.0,
        min_points: 2,       // Both clusters meet this
        min_cluster_size: 4, // Only Cluster 1 meets this
    };

    let algo = DbscanClustering::new(config);
    let mut state = DbscanState::default();

    // CURRENT BEHAVIOR: Both clusters are kept (returns 2)
    // EXPECTED BEHAVIOR: Only 1 cluster returned
    let n = algo.cluster(&mut batch, &mut state).unwrap();

    // Verify results
    // We expect this to FAIL until fixed
    assert_eq!(n, 1, "Expected 1 cluster, but got {n}");

    // Verify Cluster 1 points have a valid ID (>= 0)
    for i in 0..5 {
        assert!(batch.cluster_id[i] >= 0, "Point {i} should be in a cluster");
    }

    // Verify Cluster 2 points are noise (-1)
    for i in 5..8 {
        assert_eq!(batch.cluster_id[i], -1, "Point {i} should be noise");
    }
}
