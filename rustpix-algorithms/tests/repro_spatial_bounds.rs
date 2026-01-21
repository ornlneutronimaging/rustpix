use rustpix_algorithms::DbscanClustering;
use rustpix_core::clustering::HitClustering;
use rustpix_core::hit::GenericHit;

#[test]
fn test_clusters_outside_bounds() {
    let clustering = DbscanClustering::default();
    let mut state = clustering.create_state();

    let mut hits = Vec::new();

    // Cluster 1: (100, 100) - Within default 512x512 bounds
    for i in 0..10 {
        hits.push(GenericHit::new(100 + i, 100, 100, 0, 1, 0));
    }

    // Cluster 2: (600, 600) - Outside default bounds
    for i in 0..10 {
        hits.push(GenericHit::new(100 + i, 600, 600, 0, 1, 0));
    }

    let mut labels = vec![0; hits.len()];
    let count = clustering.cluster(&hits, &mut state, &mut labels).unwrap();

    // Should find 2 clusters
    assert_eq!(count, 2, "Expected 2 clusters, found {}", count);

    // Verify all hits have a cluster ID != -1
    for (i, label) in labels.iter().enumerate() {
        assert_ne!(*label, -1, "Hit {} was classified as noise", i);
    }
}
