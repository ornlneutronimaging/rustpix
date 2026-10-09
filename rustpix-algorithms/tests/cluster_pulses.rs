use rustpix_algorithms::{
    cluster_and_extract_batch, cluster_and_extract_pulses, AlgorithmParams, ClusteringAlgorithm,
    ClusteringConfig,
};
use rustpix_core::extraction::ExtractionConfig;
use rustpix_core::soa::HitBatch;

const ALGORITHMS: [ClusteringAlgorithm; 3] = [
    ClusteringAlgorithm::Abs,
    ClusteringAlgorithm::Grid,
    ClusteringAlgorithm::Dbscan,
];

fn batch(hits: &[(u16, u16, u32)]) -> HitBatch {
    let mut batch = HitBatch::with_capacity(hits.len());
    for &(x, y, tof) in hits {
        batch.push((x, y, tof, 20, tof, 0));
    }
    batch
}

fn params_keeping_single_hits() -> AlgorithmParams {
    AlgorithmParams {
        dbscan_min_points: 1,
        ..AlgorithmParams::default()
    }
}

fn pulses(
    hits: &mut HitBatch,
    starts: &[usize],
    algorithm: ClusteringAlgorithm,
    params: &AlgorithmParams,
) -> rustpix_core::Result<Vec<u32>> {
    cluster_and_extract_pulses(
        hits,
        starts,
        algorithm,
        &ClusteringConfig::default(),
        &ExtractionConfig::default(),
        params,
    )
    .map(|neutrons| neutrons.tof)
}

#[test]
fn coincident_hits_in_different_pulses_stay_separate() {
    let params = params_keeping_single_hits();
    for algorithm in ALGORITHMS {
        let hits = [(10, 10, 100), (10, 10, 101)];

        let mut whole = batch(&hits);
        let merged = cluster_and_extract_batch(
            &mut whole,
            algorithm,
            &ClusteringConfig::default(),
            &ExtractionConfig::default(),
            &params,
        )
        .unwrap();
        assert_eq!(merged.len(), 1, "{algorithm:?}: whole-batch clustering");

        let mut split = batch(&hits);
        let tofs = pulses(&mut split, &[0, 1], algorithm, &params).unwrap();
        assert_eq!(tofs, vec![100, 101], "{algorithm:?}");
        assert_eq!(split.cluster_id, vec![0, 1], "{algorithm:?}");
    }
}

#[test]
fn cluster_ids_are_unique_across_pulses() {
    let params = params_keeping_single_hits();
    for algorithm in ALGORITHMS {
        let mut hits = batch(&[
            (10, 10, 100),
            (11, 10, 101),
            (200, 200, 400),
            (10, 10, 100),
            (10, 11, 101),
        ]);
        let tofs = pulses(&mut hits, &[0, 3], algorithm, &params).unwrap();
        assert_eq!(tofs.len(), 3, "{algorithm:?}");
        assert_eq!(hits.cluster_id, vec![0, 0, 1, 2, 2], "{algorithm:?}");
    }
}

#[test]
fn noise_hits_keep_negative_cluster_id() {
    let mut hits = batch(&[(10, 10, 100), (50, 50, 100), (51, 50, 101)]);
    let tofs = pulses(
        &mut hits,
        &[1],
        ClusteringAlgorithm::Dbscan,
        &AlgorithmParams::default(),
    )
    .unwrap();
    assert_eq!(tofs.len(), 1);
    assert_eq!(hits.cluster_id, vec![-1, 0, 0]);
}

#[test]
fn empty_starts_cluster_the_batch_as_one_pulse() {
    let params = params_keeping_single_hits();
    let hits = [(10, 10, 100), (10, 10, 101), (90, 90, 300)];
    for algorithm in ALGORITHMS {
        let mut whole = batch(&hits);
        let expected = cluster_and_extract_batch(
            &mut whole,
            algorithm,
            &ClusteringConfig::default(),
            &ExtractionConfig::default(),
            &params,
        )
        .unwrap();

        let mut split = batch(&hits);
        let tofs = pulses(&mut split, &[], algorithm, &params).unwrap();
        assert_eq!(tofs, expected.tof, "{algorithm:?}");
        assert_eq!(split.cluster_id, whole.cluster_id, "{algorithm:?}");
    }
}

#[test]
fn repeated_and_boundary_starts_are_accepted() {
    let params = params_keeping_single_hits();
    let mut hits = batch(&[(10, 10, 100), (10, 10, 101)]);
    let tofs = pulses(
        &mut hits,
        &[0, 0, 1, 2, 2],
        ClusteringAlgorithm::Abs,
        &params,
    )
    .unwrap();
    assert_eq!(tofs, vec![100, 101]);
    assert_eq!(hits.cluster_id, vec![0, 1]);
}

#[test]
fn invalid_starts_are_rejected() {
    let params = AlgorithmParams::default();
    let mut hits = batch(&[(10, 10, 100), (10, 10, 101), (10, 10, 102)]);
    assert!(pulses(&mut hits, &[2, 1], ClusteringAlgorithm::Abs, &params).is_err());
    assert!(pulses(&mut hits, &[0, 4], ClusteringAlgorithm::Abs, &params).is_err());
}
