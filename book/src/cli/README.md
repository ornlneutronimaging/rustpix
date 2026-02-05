# Command Line Interface

The `rustpix` CLI provides batch processing capabilities for TPX3 files.

## Installation

```bash
cargo install rustpix-cli
```

Or build from source:

```bash
cargo build --release -p rustpix-cli
```

## Commands

| Command | Description |
|---------|-------------|
| `process` | Process TPX3 files to extract neutron events |
| `info` | Show information about a TPX3 file |
| `benchmark` | Benchmark clustering algorithms |

## Quick Examples

```bash
# Process a file
rustpix process input.tpx3 -o output.csv

# Show file info
rustpix info input.tpx3

# Benchmark algorithms
rustpix benchmark input.tpx3

# Get help
rustpix --help
rustpix process --help
```

## Output Formats

The output format is determined by file extension:

| Extension | Format |
|-----------|--------|
| `.csv` | Comma-separated values with header |
| `.bin`, `.dat` | Binary format (compact) |
| Other | Binary format (default) |

See [Commands Reference](commands.md) for detailed usage.
