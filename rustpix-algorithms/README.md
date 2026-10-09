# rustpix-algorithms

Clusters pixel-detector hits into neutron events and extracts one neutron per
cluster. Each clustering call takes the hits of one pulse as a
`rustpix_core::soa::HitBatch` and writes a label to each hit's `cluster_id`
(`-1` for noise).

## Algorithms

| Algorithm | Types | A hit joins a cluster when it is |
| --- | --- | --- |
| ABS (Age-Based Spatial) | `AbsClustering`, `AbsConfig` | within `radius` of the cluster's bounding box and within the time window of the cluster's first hit |
| DBSCAN | `DbscanClustering`, `DbscanConfig` | within `epsilon` and the time window of a hit with at least `min_points` neighbours, itself included |
| Grid | `GridClustering`, `GridConfig` | within `radius` and the time window of another hit in the cluster |

ABS and Grid need hits in ascending TOF order. With DBSCAN's default
`min_points = 2` an isolated hit is noise.

## Pipeline

- `cluster_and_extract_batch`: cluster one pulse and return a `NeutronBatch`.
- `cluster_and_extract_pulses`: the same for a batch holding several pulses,
  given each pulse's first hit index.
- `cluster_and_extract_stream_iter`: one `NeutronBatch` per pulse from an
  iterator of pulses.

`ClusteringAlgorithm` picks the algorithm, `ClusteringConfig` holds the shared
settings and `AlgorithmParams` the per-algorithm ones.

## Example

```toml
[dependencies]
rustpix-algorithms = "1.4"
rustpix-core = "1.4"
```

```rust
use rustpix_algorithms::{
    cluster_and_extract_batch, AlgorithmParams, ClusteringAlgorithm, ClusteringConfig,
};
use rustpix_core::{soa::HitBatch, ExtractionConfig, NeutronBatch};

fn extract(batch: &mut HitBatch) -> rustpix_core::Result<NeutronBatch> {
    // batch holds one pulse; use cluster_and_extract_pulses for several
    batch.sort_by_tof();
    cluster_and_extract_batch(
        batch,
        ClusteringAlgorithm::Grid,
        &ClusteringConfig::default().with_min_cluster_size(2),
        &ExtractionConfig::default(),
        &AlgorithmParams::default(),
    )
}
```

## License

MIT. See [LICENSE](https://github.com/ornlneutronimaging/rustpix/blob/main/LICENSE).
