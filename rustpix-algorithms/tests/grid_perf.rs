#![allow(
    clippy::uninlined_format_args,
    clippy::cast_possible_truncation,
    clippy::unreadable_literal
)]
use rustpix_algorithms::{
    AbsClustering, AbsConfig, AbsState, GridClustering, GridConfig, GridState,
};
use rustpix_core::soa::HitBatch;
use std::time::Instant;

#[test]
fn test_grid_vs_abs_performance() {
    // Generate synthetic data: 100K hits
    // Random positions, but sorted time.
    let n = 100_000;
    let mut batch = HitBatch::with_capacity(n);

    // Simulate a detector with hits spread over time
    // To make it challenging for Grid (without pruning), we need temporal depth in each spatial cell.
    // Concentrate all hits in a small region (e.g. 64x64) to stress the algorithm.

    let mut rng_seed: u64 = 12345;
    let mut rand = || {
        rng_seed = (rng_seed.wrapping_mul(1103515245).wrapping_add(12345)) & 0x7fffffff;
        rng_seed as u16
    };

    for i in 0..n {
        let x = rand() % 64; // Concentrated in 0..64
        let y = rand() % 64;
        let tof = i as u32 * 10; // 10 ticks per hit, sorted by construction
        batch.push(x, y, tof, 1, 0, 0);
    }

    println!("Generated {} hits", n);

    // Run ABS
    let abs_config = AbsConfig {
        radius: 5.0,
        neutron_correlation_window_ns: 75.0,
        ..Default::default()
    };
    let abs = AbsClustering::new(abs_config);
    let mut abs_state = AbsState::default();
    let mut batch_abs = batch.clone();

    let start_abs = Instant::now();
    let _ = abs.cluster(&mut batch_abs, &mut abs_state).unwrap();
    let duration_abs = start_abs.elapsed();
    println!("ABS time: {:?}", duration_abs);

    // Run Grid
    let grid_config = GridConfig {
        radius: 5.0,
        temporal_window_ns: 75.0,
        ..Default::default()
    };
    let grid = GridClustering::new(grid_config);
    let mut grid_state = GridState::default();
    let mut batch_grid = batch.clone();

    let start_grid = Instant::now();
    let _ = grid.cluster(&mut batch_grid, &mut grid_state).unwrap();
    let duration_grid = start_grid.elapsed();
    println!("Grid time: {:?}", duration_grid);

    // Performance check: Grid should be within 5x of ABS
    // (allows for CI variance; pre-optimization was 100x+ slower)
    let ratio = duration_grid.as_secs_f64() / duration_abs.as_secs_f64();
    println!("Ratio Grid/ABS: {:.2}x", ratio);

    assert!(ratio < 5.0, "Grid is too slow! Ratio: {:.2}x", ratio);
}
