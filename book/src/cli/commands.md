# Commands Reference

## rustpix process

Process TPX3 files to extract neutron events.

```bash
rustpix process [OPTIONS] -o <OUTPUT> <INPUT>...
```

### Arguments

| Argument | Description |
|----------|-------------|
| `<INPUT>...` | Input TPX3 file(s) |

### Options

| Option | Default | Description |
|--------|---------|-------------|
| `-o, --output <PATH>` | Required | Output file path |
| `-a, --algorithm <ALGO>` | `abs` | Clustering algorithm (`abs`, `dbscan`, `grid`) |
| `--radius <FLOAT>` | `5.0` | Spatial radius for clustering (pixels) |
| `--temporal-window-ns <FLOAT>` | `75.0` | Temporal window for clustering (nanoseconds) |
| `--min-cluster-size <INT>` | `1` | Minimum cluster size |
| `--out-of-core <BOOL>` | `true` | Enable out-of-core processing |
| `--memory-fraction <FLOAT>` | `0.5` | Fraction of available memory to use |
| `--memory-budget-bytes <INT>` | Auto | Explicit memory budget in bytes |
| `--parallelism <INT>` | Auto | Worker threads for processing |
| `--queue-depth <INT>` | `2` | Pipeline queue depth |
| `--async-io <BOOL>` | `false` | Enable async I/O pipeline |
| `-v, --verbose` | Off | Verbose output |

### Examples

```bash
# Basic processing
rustpix process input.tpx3 -o output.csv

# Process multiple files
rustpix process file1.tpx3 file2.tpx3 -o combined.csv

# Use DBSCAN with custom parameters
rustpix process input.tpx3 -o output.csv \
    --algorithm dbscan \
    --radius 3.0 \
    --temporal-window-ns 50.0

# Verbose output with parallel processing
rustpix process input.tpx3 -o output.bin \
    --verbose \
    --parallelism 8 \
    --async-io true

# Memory-constrained processing
rustpix process huge_file.tpx3 -o output.csv \
    --memory-fraction 0.3 \
    --out-of-core true
```

## rustpix info

Display information about a TPX3 file.

```bash
rustpix info <INPUT>
```

### Example

```bash
$ rustpix info data.tpx3
File: data.tpx3
Size: 104857600 bytes (104.86 MB)
Packets: 6553600
Hits: 5242880
TOF range: 0 - 16666666
X range: 0 - 511
Y range: 0 - 511
```

## rustpix benchmark

Benchmark clustering algorithms on a TPX3 file.

```bash
rustpix benchmark [OPTIONS] <INPUT>
```

### Options

| Option | Default | Description |
|--------|---------|-------------|
| `-i, --iterations <INT>` | `3` | Number of benchmark iterations |

### Example

```bash
$ rustpix benchmark data.tpx3 --iterations 5
Benchmarking with 5242880 hits, 5 iterations
Algorithm  | Mean Time (ms)  | Min Time (ms)   | Max Time (ms)
-----------------------------------------------------------------
ABS        | 245.32          | 238.45          | 256.78
DBSCAN     | 1234.56         | 1198.23         | 1287.34
Grid       | 312.45          | 298.12          | 334.56
```

## rustpix out-of-core-benchmark

Benchmark out-of-core processing modes.

```bash
rustpix out-of-core-benchmark [OPTIONS] <INPUT>
```

### Options

| Option | Default | Description |
|--------|---------|-------------|
| `-a, --algorithm <ALGO>` | `abs` | Clustering algorithm |
| `--radius <FLOAT>` | `5.0` | Spatial radius (pixels) |
| `--temporal-window-ns <FLOAT>` | `75.0` | Temporal window (ns) |
| `--min-cluster-size <INT>` | `1` | Minimum cluster size |
| `-i, --iterations <INT>` | `3` | Number of iterations |
| `--memory-fraction <FLOAT>` | `0.5` | Memory fraction |
| `--parallelism <INT>` | Auto | Worker threads |
| `--queue-depth <INT>` | `2` | Queue depth |
| `--async-io <BOOL>` | `false` | Enable async I/O |

### Example

```bash
$ rustpix out-of-core-benchmark data.tpx3 --parallelism 4 --async-io true
Out-of-core benchmark (3 iterations)
Single-thread avg: 12.345s
Multi-thread avg: 4.567s (threads: 4, async: true)
Speedup: 2.70x
```

## Environment Variables

The CLI respects standard environment variables:

| Variable | Description |
|----------|-------------|
| `RAYON_NUM_THREADS` | Override default thread count for parallel processing |
