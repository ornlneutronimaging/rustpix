# rustpix-algorithms

Clustering algorithms with spatial indexing for neutron event detection.

## Algorithms

### ABS (Adjacency-Based Search)
Fast 8-connectivity clustering optimized for pixel detectors.

```rust
use rustpix_algorithms::abs::AbsClustering;

let clustering = AbsClustering::new(5.0, 75.0); // spatial, temporal epsilon
let clusters = clustering.cluster(&hits);
```

### DBSCAN
Density-based clustering with KD-tree spatial indexing.

```rust
use rustpix_algorithms::dbscan::DbscanClustering;

let clustering = DbscanClustering::new(5.0, 75.0, 1); // eps, time_eps, min_pts
let clusters = clustering.cluster(&hits);
```

### Graph
Union-find based connected component detection.

```rust
use rustpix_algorithms::graph::GraphClustering;

let clustering = GraphClustering::new(5.0, 75.0);
let clusters = clustering.cluster(&hits);
```

### Grid
Parallel grid-based clustering with spatial hashing.

```rust
use rustpix_algorithms::grid::GridClustering;

let clustering = GridClustering::new(5.0, 75.0);
let clusters = clustering.cluster(&hits);
```

## Performance

| Algorithm | Speed      | Memory     | Best For                    |
| --------- | ---------- | ---------- | --------------------------- |
| ABS       | Fastest    | Low        | Dense clusters              |
| DBSCAN    | Moderate   | Moderate   | Variable density            |
| Graph     | Fast       | Low        | General purpose             |
| Grid      | Very Fast  | Moderate   | Large datasets, parallelism |

## License

MIT License - see [LICENSE](../LICENSE) for details.
