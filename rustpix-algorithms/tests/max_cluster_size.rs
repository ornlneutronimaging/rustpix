use rustpix_algorithms::{
    cluster_and_extract_batch, AlgorithmParams, ClusteringAlgorithm, ClusteringConfig,
    GridClustering, GridConfig, GridState,
};
use rustpix_core::extraction::ExtractionConfig;
use rustpix_core::soa::HitBatch;

const ALGORITHMS: [ClusteringAlgorithm; 3] = [
    ClusteringAlgorithm::Abs,
    ClusteringAlgorithm::Grid,
    ClusteringAlgorithm::Dbscan,
];

fn three_and_one() -> HitBatch {
    let mut batch = HitBatch::default();
    for (x, y, tof) in [(10, 10, 100), (11, 10, 101), (10, 11, 101), (200, 200, 102)] {
        batch.push((x, y, tof, 20, tof, 0));
    }
    batch
}

fn run(algorithm: ClusteringAlgorithm, max_cluster_size: Option<u16>) -> (usize, Vec<i32>) {
    let mut batch = three_and_one();
    let clustering = ClusteringConfig {
        max_cluster_size,
        ..ClusteringConfig::default()
    };
    let params = AlgorithmParams {
        dbscan_min_points: 1,
        ..AlgorithmParams::default()
    };
    let neutrons = cluster_and_extract_batch(
        &mut batch,
        algorithm,
        &clustering,
        &ExtractionConfig::default(),
        &params,
    )
    .unwrap();
    (neutrons.len(), batch.cluster_id)
}

#[test]
fn clusters_above_max_are_dropped() {
    for algorithm in ALGORITHMS {
        assert_eq!(
            run(algorithm, Some(2)),
            (1, vec![-1, -1, -1, 0]),
            "{algorithm:?}"
        );
    }
}

#[test]
fn clusters_at_max_are_kept() {
    for algorithm in ALGORITHMS {
        assert_eq!(run(algorithm, Some(3)).0, 2, "{algorithm:?}");
        assert_eq!(run(algorithm, None).0, 2, "{algorithm:?}");
    }
}

#[test]
fn grid_applies_its_own_max_cluster_size() {
    let mut batch = three_and_one();
    let algo = GridClustering::new(GridConfig {
        max_cluster_size: Some(2),
        ..GridConfig::default()
    });
    let n = algo.cluster(&mut batch, &mut GridState::default()).unwrap();
    assert_eq!(n, 1);
    assert_eq!(batch.cluster_id, vec![-1, -1, -1, 0]);
}
