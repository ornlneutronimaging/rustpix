use rustpix_algorithms::{DbscanClustering, DbscanConfig, DbscanState};
use rustpix_core::soa::HitBatch;

fn cluster_ids(hits: &[(u16, u16, u32)], min_points: usize) -> (usize, Vec<i32>) {
    let mut batch = HitBatch::with_capacity(hits.len());
    for &(x, y, tof) in hits {
        batch.push((x, y, tof, 20, tof, 0));
    }
    let algo = DbscanClustering::new(DbscanConfig {
        min_points,
        ..DbscanConfig::default()
    });
    let mut state = DbscanState::default();
    let n = algo.cluster(&mut batch, &mut state).unwrap();
    (n, batch.cluster_id)
}

const SINGLE: &[(u16, u16, u32)] = &[(10, 10, 100)];
const PAIR: &[(u16, u16, u32)] = &[(10, 10, 100), (11, 10, 101)];
const TRIPLE: &[(u16, u16, u32)] = &[(10, 10, 100), (11, 10, 101), (10, 11, 101)];

#[test]
fn default_min_points_clusters_a_pair() {
    assert_eq!(DbscanConfig::default().min_points, 2);
    assert_eq!(cluster_ids(PAIR, 2), (1, vec![0, 0]));
}

#[test]
fn min_points_counts_the_hit_itself() {
    assert_eq!(cluster_ids(PAIR, 3), (0, vec![-1, -1]));
    assert_eq!(cluster_ids(TRIPLE, 3), (1, vec![0, 0, 0]));
    assert_eq!(cluster_ids(TRIPLE, 4), (0, vec![-1, -1, -1]));
}

#[test]
fn isolated_hit_is_noise_unless_min_points_is_one() {
    assert_eq!(cluster_ids(SINGLE, 2), (0, vec![-1]));
    assert_eq!(cluster_ids(SINGLE, 1), (1, vec![0]));
}
