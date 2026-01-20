use rustpix_algorithms::{AbsClustering, DbscanClustering, GraphClustering, GridClustering};
use rustpix_core::clustering::HitClustering;
use rustpix_core::hit::GenericHit;

fn generate_hits() -> Vec<GenericHit> {
    let mut hits = Vec::new();
    // Cluster 1: centered at 100, 100, tof=1000
    for i in 0..10 {
        hits.push(GenericHit {
            x: 100 + (i % 3),
            y: 100 + (i / 3),
            tof: 1000,
            tot: 10,
            timestamp: 0,
            chip_id: 0,
            _padding: 0,
            cluster_id: -1,
        });
    }
    // Cluster 2: centered at 150, 150, tof=2000
    for i in 0..10 {
        hits.push(GenericHit {
            x: 150 + (i % 3),
            y: 150 + (i / 3),
            tof: 2000,
            tot: 10,
            timestamp: 0,
            chip_id: 0,
            _padding: 0,
            cluster_id: -1,
        });
    }
    hits
}

#[test]
fn test_verification_abs() {
    let hits = generate_hits();
    let algo = AbsClustering::default();
    let mut state = algo.create_state();
    let mut labels = vec![0; hits.len()];
    let n = algo.cluster(&hits, &mut state, &mut labels).unwrap();
    assert_eq!(n, 2, "ABS Found {} clusters, expected 2", n);
}

#[test]
fn test_verification_grid() {
    let hits = generate_hits();
    let algo = GridClustering::default();
    let mut state = algo.create_state();
    let mut labels = vec![0; hits.len()];
    let n = algo.cluster(&hits, &mut state, &mut labels).unwrap();
    assert_eq!(n, 2, "Grid Found {} clusters, expected 2", n);
}

#[test]
fn test_verification_graph() {
    let hits = generate_hits();
    let algo = GraphClustering::default();
    let mut state = algo.create_state();
    let mut labels = vec![0; hits.len()];
    let n = algo.cluster(&hits, &mut state, &mut labels).unwrap();
    assert_eq!(n, 2, "Graph Found {} clusters, expected 2", n);
}

#[test]
fn test_verification_dbscan() {
    let hits = generate_hits();
    let algo = DbscanClustering::default();
    let mut state = algo.create_state();
    let mut labels = vec![0; hits.len()];
    let n = algo.cluster(&hits, &mut state, &mut labels).unwrap();
    assert_eq!(n, 2, "DBSCAN Found {} clusters, expected 2", n);
}
