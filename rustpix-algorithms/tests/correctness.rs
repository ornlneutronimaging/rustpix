#![allow(clippy::uninlined_format_args)]
use rustpix_algorithms::{
    AbsClustering, AbsConfig, AbsState, DbscanClustering, DbscanConfig, DbscanState,
    GridClustering, GridConfig, GridState,
};
use rustpix_core::soa::HitBatch;

fn generate_hits() -> HitBatch {
    let mut batch = HitBatch::with_capacity(20);
    // Cluster 1: centered at 100, 100, tof=1000
    for i in 0..10 {
        batch.push(100 + (i % 3), 100 + (i / 3), 1000, 10, 0, 0);
    }
    // Cluster 2: centered at 150, 150, tof=2000
    for i in 0..10 {
        batch.push(150 + (i % 3), 150 + (i / 3), 2000, 10, 0, 0);
    }
    batch
}

#[test]
fn test_verification_abs() {
    let mut batch = generate_hits();
    let config = AbsConfig {
        radius: 5.0,
        neutron_correlation_window_ns: 100.0,
        min_cluster_size: 1,
        scan_interval: 100,
    };
    let algo = AbsClustering::new(config);
    let mut state = AbsState::default();
    let n = algo.cluster(&mut batch, &mut state).unwrap();
    assert_eq!(n, 2, "ABS Found {} clusters, expected 2", n);
}

#[test]
fn test_verification_grid() {
    let mut batch = generate_hits();
    let config = GridConfig {
        radius: 5.0,
        temporal_window_ns: 100.0,
        min_cluster_size: 1,
        cell_size: 32,
        max_cluster_size: None,
    };
    let algo = GridClustering::new(config);
    let mut state = GridState::default();
    let n = algo.cluster(&mut batch, &mut state).unwrap();
    assert_eq!(n, 2, "Grid Found {} clusters, expected 2", n);
}

#[test]
fn test_verification_dbscan() {
    let mut batch = generate_hits();
    let config = DbscanConfig {
        epsilon: 5.0,
        temporal_window_ns: 100.0,
        min_points: 2,
        min_cluster_size: 1,
    };
    let algo = DbscanClustering::new(config);
    let mut state = DbscanState::default();
    let n = algo.cluster(&mut batch, &mut state).unwrap();
    assert_eq!(n, 2, "DBSCAN Found {} clusters, expected 2", n);
}
