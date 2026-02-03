# rustpix-cli

Command-line interface for rustpix pixel detector data processing.

## Installation

### From crates.io

```bash
cargo install rustpix-cli
```

### From source

```bash
cargo install --path rustpix-cli
```

## Usage

```bash
# Process a TPX3 file
rustpix process input.tpx3 -o output.h5

# Show file info
rustpix info input.tpx3

# Convert to different format
rustpix convert input.tpx3 -f json -o output.json

# Run with specific clustering algorithm
rustpix process input.tpx3 --algorithm abs --eps 5.0 -o output.h5
```

## Commands

| Command | Description |
|---------|-------------|
| `process` | Process TPX3 file with clustering |
| `info` | Display file information |
| `convert` | Convert between formats |
| `validate` | Validate file integrity |

## Options

```
-o, --output <FILE>     Output file path
-a, --algorithm <ALG>   Clustering algorithm (abs, dbscan, graph, grid)
-e, --eps <FLOAT>       Spatial epsilon for clustering
-t, --time-eps <FLOAT>  Temporal epsilon for clustering
-v, --verbose           Verbose output
-h, --help              Print help
```

## License

MIT License - see [LICENSE](../LICENSE) for details.
