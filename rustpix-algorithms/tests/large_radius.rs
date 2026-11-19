use rustpix_algorithms::{
    DbscanClustering, DbscanConfig, DbscanState, GridClustering, GridConfig, GridState,
};
use rustpix_core::soa::HitBatch;

/// Two hits 40 px apart at the same TOF: two 32 px cells apart.
fn distant_pair() -> HitBatch {
    let mut batch = HitBatch::default();
    batch.push((30, 10, 1000, 30, 0, 0));
    batch.push((70, 10, 1000, 30, 0, 0));
    batch
}

#[test]
fn grid_links_hits_when_radius_exceeds_cell_size() {
    for cell_size in [0, 1, 8, 32, 64] {
        let config = GridConfig {
            radius: 50.0,
            cell_size,
            ..Default::default()
        };
        let clusters = GridClustering::new(config)
            .cluster(&mut distant_pair(), &mut GridState::default())
            .unwrap();
        assert_eq!(clusters, 1, "cell_size = {cell_size}");
    }
}

#[test]
fn grid_keeps_hits_apart_beyond_the_radius() {
    let config = GridConfig {
        radius: 39.0,
        cell_size: 8,
        ..Default::default()
    };
    let clusters = GridClustering::new(config)
        .cluster(&mut distant_pair(), &mut GridState::default())
        .unwrap();
    assert_eq!(clusters, 2);
}

#[test]
fn grid_state_survives_a_cell_size_change() {
    let mut state = GridState::default();
    for radius in [5.0, 50.0, 5.0] {
        let config = GridConfig {
            radius,
            ..Default::default()
        };
        let expected = if radius >= 40.0 { 1 } else { 2 };
        let clusters = GridClustering::new(config)
            .cluster(&mut distant_pair(), &mut state)
            .unwrap();
        assert_eq!(clusters, expected, "radius = {radius}");
    }
}

#[test]
fn dbscan_does_not_count_a_neighbour_twice() {
    // One-column grid: with min_points = 3, two hits must stay noise.
    let mut batch = HitBatch::default();
    batch.push((10, 50, 1000, 30, 0, 0));
    batch.push((10, 51, 1000, 30, 0, 0));
    let config = DbscanConfig {
        epsilon: 50.0,
        min_points: 3,
        ..Default::default()
    };
    let clusters = DbscanClustering::new(config)
        .cluster(&mut batch, &mut DbscanState::default())
        .unwrap();
    assert_eq!(clusters, 0);
    assert_eq!(batch.cluster_id, vec![-1, -1]);
}

#[test]
fn dbscan_state_reuse_across_growing_extent() {
    let algo = DbscanClustering::new(DbscanConfig::default());
    let mut state = DbscanState::default();

    let mut compact = HitBatch::default();
    for i in 0..200u16 {
        compact.push((10 + i % 3, 10 + i % 2, 1000, 30, 0, 0));
    }
    assert_eq!(algo.cluster(&mut compact, &mut state).unwrap(), 1);

    let mut spread = HitBatch::default();
    spread.push((12, 11, 1000, 30, 0, 0));
    spread.push((500, 500, 1000, 30, 0, 0));
    assert_eq!(algo.cluster(&mut spread, &mut state).unwrap(), 0);
    assert_eq!(spread.cluster_id, vec![-1, -1]);
}
