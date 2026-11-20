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
| `-f, --format <FMT>` | Auto | Output format override (`csv`, `binary`, `hdf5`, `sns-hdf5`, `tiff`) |
| `-a, --algorithm <ALGO>` | `abs` | Clustering algorithm (`abs`, `dbscan`, `grid`) |
| `--radius <FLOAT>` | `5.0` | Spatial radius for clustering (pixels) |
| `--temporal-window-ns <FLOAT>` | `75.0` | Temporal window for clustering (nanoseconds) |
| `--min-cluster-size <INT>` | `1` | Minimum cluster size |
| `--out-of-core <BOOL>` | `true` | Enable out-of-core processing |
| `--memory-fraction <FLOAT>` | `0.5` | Fraction of available memory to use (out-of-core only) |
| `--memory-budget-bytes <INT>` | Auto | Explicit memory budget in bytes |
| `--parallelism <INT>` | `1` | Worker threads for processing (out-of-core only) |
| `--queue-depth <INT>` | `2` | Pipeline queue depth (out-of-core only) |
| `--async-io <BOOL>` | `false` | Enable async I/O pipeline (out-of-core only) |
| `--run-number <INT>` | `0` | Run number for SNS HDF5 export |
| `--ipts <STRING>` | `""` | Experiment identifier for SNS HDF5 (e.g., `IPTS-XXXXX`) |
| `--instrument <NAME>` | `venus` | Instrument preset for SNS HDF5 (`venus`) |
| `--tof-bins <INT>` | `200` | Number of TOF bins for TIFF output |
| `--tof-max <INT>` | Auto | Maximum TOF in 25ns ticks for TIFF (auto-detect if omitted); neutrons above it are left out and counted in a warning |
| `--bit-depth <INT>` | `16` | Bit depth for TIFF output (`16` or `32`) |
| `-v, --verbose` | Off | Verbose output |

The out-of-core options (`--memory-fraction`, `--memory-budget-bytes`,
`--parallelism`, `--queue-depth`, `--async-io`) have no effect with
`--out-of-core false`.

The CLI uses the VENUS detector configuration with a 60 Hz TDC. For a run at
another TDC rate, use the Python API or the GUI and set the TDC frequency.

### Output files

- Inputs are checked before anything is written: each must contain TPX3 data,
  and none may be the output file.
- The output is written to a temporary file next to its destination and moved
  into place when the run succeeds. A failed run leaves an existing output
  file as it was.
- An output path ending in `.tpx3` is refused.
- TIFF output also writes a spectrum to `<output stem>.csv`. If that file
  exists and is not a spectrum from an earlier run, the command stops instead
  of overwriting it.

### Examples

```bash
# Basic CSV export
rustpix process input.tpx3 -o output.csv

# Process multiple files
rustpix process file1.tpx3 file2.tpx3 -o combined.csv

# Generic NeXus HDF5 export
rustpix process input.tpx3 -o output.h5

# ORNL SNS NXsnsevent HDF5 export
rustpix process input.tpx3 -o output.nxs.h5 \
    --run-number 12345 \
    --ipts IPTS-XXXXX

# TIFF stack with custom TOF binning
rustpix process input.tpx3 -o output.tiff \
    --tof-bins 500 \
    --tof-max 40000 \
    --bit-depth 32

# Override format (ignore file extension)
rustpix process input.tpx3 -o output.dat -f sns-hdf5 \
    --run-number 12345

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
Benchmarking with 5242880 hits in 3600 pulses, 5 iterations
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

