use rustpix_algorithms::{
    cluster_and_extract_batch, AbsClustering, AbsConfig, AbsState, AlgorithmParams,
    ClusteringAlgorithm, DbscanClustering, DbscanConfig, DbscanState, GridClustering, GridConfig,
    GridState,
};
use rustpix_core::clustering::{ClusteringConfig, ClusteringError};
use rustpix_core::extraction::ExtractionConfig;
use rustpix_core::soa::HitBatch;

const BAD_VALUES: [f64; 4] = [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -1.0];

fn two_hits() -> HitBatch {
    let mut batch = HitBatch::default();
    batch.push((10, 10, 1000, 30, 0, 0));
    batch.push((11, 10, 1001, 30, 0, 0));
    batch
}

fn assert_invalid(result: &Result<usize, ClusteringError>, what: &str) {
    assert!(
        matches!(result, Err(ClusteringError::InvalidConfig(_))),
        "{what}: expected InvalidConfig, got {result:?}"
    );
}

#[test]
fn abs_rejects_bad_radius_and_window() {
    for bad in BAD_VALUES {
        let radius = AbsConfig {
            radius: bad,
            ..Default::default()
        };
        assert_invalid(
            &AbsClustering::new(radius).cluster(&mut two_hits(), &mut AbsState::default()),
            &format!("abs radius {bad}"),
        );
        let window = AbsConfig {
            neutron_correlation_window_ns: bad,
            ..Default::default()
        };
        assert_invalid(
            &AbsClustering::new(window).cluster(&mut two_hits(), &mut AbsState::default()),
            &format!("abs window {bad}"),
        );
    }
}

#[test]
fn abs_rejects_zero_scan_interval() {
    let config = AbsConfig {
        scan_interval: 0,
        ..Default::default()
    };
    let algo = AbsClustering::new(config);
    assert_invalid(
        &algo.cluster(&mut two_hits(), &mut AbsState::default()),
        "scan_interval 0",
    );
    assert_invalid(
        &algo.cluster(&mut HitBatch::default(), &mut AbsState::default()),
        "scan_interval 0, empty batch",
    );
}

#[test]
fn zero_scan_interval_is_an_error_through_the_pipeline() {
    let params = AlgorithmParams {
        abs_scan_interval: 0,
        ..Default::default()
    };
    let result = cluster_and_extract_batch(
        &mut two_hits(),
        ClusteringAlgorithm::Abs,
        &ClusteringConfig::default(),
        &ExtractionConfig::default(),
        &params,
    );
    assert!(result.is_err());
}

#[test]
fn grid_rejects_bad_radius_and_window() {
    for bad in BAD_VALUES {
        let radius = GridConfig {
            radius: bad,
            ..Default::default()
        };
        assert_invalid(
            &GridClustering::new(radius).cluster(&mut two_hits(), &mut GridState::default()),
            &format!("grid radius {bad}"),
        );
        let window = GridConfig {
            temporal_window_ns: bad,
            ..Default::default()
        };
        assert_invalid(
            &GridClustering::new(window).cluster(&mut two_hits(), &mut GridState::default()),
            &format!("grid window {bad}"),
        );
    }
}

#[test]
fn dbscan_rejects_bad_epsilon_and_window() {
    for bad in BAD_VALUES {
        let epsilon = DbscanConfig {
            epsilon: bad,
            ..Default::default()
        };
        assert_invalid(
            &DbscanClustering::new(epsilon).cluster(&mut two_hits(), &mut DbscanState::default()),
            &format!("dbscan epsilon {bad}"),
        );
        let window = DbscanConfig {
            temporal_window_ns: bad,
            ..Default::default()
        };
        assert_invalid(
            &DbscanClustering::new(window).cluster(&mut two_hits(), &mut DbscanState::default()),
            &format!("dbscan window {bad}"),
        );
    }
}

#[test]
fn very_large_finite_parameters_do_not_overflow() {
    let hits_far_apart_in_time = || {
        let mut batch = HitBatch::default();
        batch.push((10, 10, 1000, 30, 0, 0));
        batch.push((11, 10, 400_000, 30, 0, 0));
        batch
    };

    let abs = AbsConfig {
        radius: 1e12,
        neutron_correlation_window_ns: 1e15,
        ..Default::default()
    };
    let clusters = AbsClustering::new(abs)
        .cluster(&mut hits_far_apart_in_time(), &mut AbsState::default())
        .unwrap();
    assert_eq!(clusters, 1);

    let grid = GridConfig {
        radius: 1e12,
        temporal_window_ns: 1e15,
        ..Default::default()
    };
    let clusters = GridClustering::new(grid)
        .cluster(&mut hits_far_apart_in_time(), &mut GridState::default())
        .unwrap();
    assert_eq!(clusters, 1);

    let dbscan = DbscanConfig {
        epsilon: 1e12,
        temporal_window_ns: 1e15,
        ..Default::default()
    };
    let clusters = DbscanClustering::new(dbscan)
        .cluster(&mut hits_far_apart_in_time(), &mut DbscanState::default())
        .unwrap();
    assert_eq!(clusters, 1);
}
