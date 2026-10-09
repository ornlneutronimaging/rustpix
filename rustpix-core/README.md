# rustpix-core

Shared types for the rustpix crates: columnar hit and neutron containers,
clustering and extraction settings, centroid extraction, and errors.
[`rustpix-io`](https://crates.io/crates/rustpix-io) reads TPX3 files into a
`HitBatch`, and [`rustpix-algorithms`](https://crates.io/crates/rustpix-algorithms)
clusters it into neutrons.

## Main types

- `soa::HitBatch`: hits as parallel vectors `x`, `y`, `tof`, `tot`, `timestamp`,
  `chip_id`, `cluster_id`. TOF is in 25 ns ticks.
- `ClusteringConfig`: `radius` (pixels), `temporal_window_ns`, `min_cluster_size`
  and `max_cluster_size` (applied by Grid only).
- `ExtractionConfig` and `SimpleCentroidExtraction`: one TOT-weighted centroid
  per labelled cluster.
- `Neutron`, `NeutronBatch`: extracted events; `x` and `y` are pixel coordinates
  times `super_resolution_factor` (default 8).
- `Error` and `Result`.

## Example

```rust
use rustpix_core::soa::HitBatch;
use rustpix_core::{ExtractionConfig, ExtractionError, NeutronExtraction, SimpleCentroidExtraction};

fn main() -> Result<(), ExtractionError> {
    let mut hits = HitBatch::with_capacity(3);
    // (x, y, tof, tot, timestamp, chip_id)
    hits.push((10, 10, 1000, 30, 0, 0));
    hits.push((12, 10, 1001, 10, 0, 0));
    hits.push((50, 60, 2000, 15, 0, 0));

    // Labels normally come from a rustpix-algorithms clustering pass; -1 is noise.
    hits.cluster_id.copy_from_slice(&[0, 0, 1]);

    let extractor = SimpleCentroidExtraction::with_config(ExtractionConfig::default());
    // Number of clusters; normally the value returned by cluster().
    let neutrons = extractor.extract_soa(&hits, 2)?;

    assert_eq!(neutrons.len(), 2);
    assert_eq!(neutrons[0].n_hits, 2);
    Ok(())
}
```

## Features

- `serde`: `Serialize` and `Deserialize` for `HitBatch`.

## License

MIT. See [LICENSE](https://github.com/ornlneutronimaging/rustpix/blob/main/LICENSE).
