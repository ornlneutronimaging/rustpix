use rustpix_algorithms::{
    cluster_and_extract_batch, AbsClustering, AbsConfig, AbsState, AlgorithmParams,
    ClusteringAlgorithm,
};
use rustpix_core::clustering::ClusteringConfig;
use rustpix_core::extraction::ExtractionConfig;
use rustpix_core::soa::HitBatch;

/// Five adjacent hits in ascending TOF order with their corner at `(x0, y0)`.
fn compact_cluster(x0: u16, y0: u16) -> HitBatch {
    let mut batch = HitBatch::default();
    for (dx, dy, tof) in [
        (0, 0, 1000),
        (1, 0, 1000),
        (0, 1, 1000),
        (1, 1, 1001),
        (2, 1, 1001),
    ] {
        batch.push((x0 + dx, y0 + dy, tof, 30, 0, 0));
    }
    batch
}

fn abs_clusters(batch: &mut HitBatch, state: &mut AbsState) -> usize {
    AbsClustering::new(AbsConfig::default())
        .cluster(batch, state)
        .unwrap()
}

#[test]
fn narrow_tall_pulse_keeps_clusters_whole() {
    for y0 in [100, 287, 288, 290, 319, 320, 400, 511] {
        let mut batch = compact_cluster(50, y0);
        let clusters = abs_clusters(&mut batch, &mut AbsState::default());
        assert_eq!(clusters, 1, "cluster at y = {y0}");
        assert!(
            batch.cluster_id.iter().all(|&id| id == 0),
            "labels at y = {y0}"
        );
    }
}

#[test]
fn narrow_tall_pulse_yields_one_neutron() {
    let mut batch = compact_cluster(50, 400);
    let neutrons = cluster_and_extract_batch(
        &mut batch,
        ClusteringAlgorithm::Abs,
        &ClusteringConfig::default(),
        &ExtractionConfig::default(),
        &AlgorithmParams::default(),
    )
    .unwrap();
    assert_eq!(neutrons.n_hits, vec![5]);
}

#[test]
fn reused_state_handles_changing_extents() {
    let mut state = AbsState::default();
    for (x0, y0) in [(500, 10), (10, 500), (300, 300), (20, 900), (900, 20)] {
        let mut batch = compact_cluster(x0, y0);
        assert_eq!(abs_clusters(&mut batch, &mut state), 1, "at ({x0}, {y0})");
    }
}

#[test]
fn two_separate_clusters_in_a_narrow_tall_pulse() {
    let mut batch = compact_cluster(50, 300);
    let far = compact_cluster(50, 450);
    batch.append(&far);
    batch.sort_by_tof();
    assert_eq!(abs_clusters(&mut batch, &mut AbsState::default()), 2);
}

#[test]
fn cluster_growing_across_cells_stays_whole() {
    let mut batch = HitBatch::default();
    for x in (30..=65).step_by(5) {
        batch.push((x, 10, 1000, 30, 0, 0));
    }
    assert_eq!(abs_clusters(&mut batch, &mut AbsState::default()), 1);
}

#[test]
fn radius_larger_than_a_cell_links_hits() {
    let mut batch = HitBatch::default();
    batch.push((30, 10, 1000, 30, 0, 0));
    batch.push((70, 10, 1000, 30, 0, 0));
    let config = AbsConfig {
        radius: 50.0,
        ..Default::default()
    };
    let clusters = AbsClustering::new(config)
        .cluster(&mut batch, &mut AbsState::default())
        .unwrap();
    assert_eq!(clusters, 1);
}

#[test]
fn aged_buckets_are_closed_and_reusable() {
    // Same pixel, hits far apart in time: each must open its own cluster.
    let mut batch = HitBatch::default();
    for i in 0..500u32 {
        batch.push((40, 400, 1000 + i * 100, 30, 0, 0));
    }
    let config = AbsConfig {
        scan_interval: 7,
        ..Default::default()
    };
    let clusters = AbsClustering::new(config)
        .cluster(&mut batch, &mut AbsState::default())
        .unwrap();
    assert_eq!(clusters, 500);
}

#[test]
fn reused_state_sizes_the_grid_from_the_current_batch() {
    let mut state = AbsState::default();
    for (x, y) in [(0, 65_535), (65_535, 0)] {
        let mut batch = HitBatch::default();
        batch.push((x, y, 1000, 30, 0, 0));
        assert_eq!(abs_clusters(&mut batch, &mut state), 1);
    }
}

#[test]
fn search_does_not_wrap_into_the_next_row() {
    // Radius 50 gives 50 px cells; the last hit's right-hand neighbour column
    // is past the grid edge and must not alias row 1, column 0.
    let mut batch = HitBatch::default();
    for (x, y) in [(90, 99), (0, 50), (50, 50), (100, 55)] {
        batch.push((x, y, 1000, 30, 0, 0));
    }
    let config = AbsConfig {
        radius: 50.0,
        ..Default::default()
    };
    let clusters = AbsClustering::new(config)
        .cluster(&mut batch, &mut AbsState::default())
        .unwrap();
    assert!(clusters >= 1);
    assert!(batch.cluster_id.iter().all(|&id| id >= 0));
}
