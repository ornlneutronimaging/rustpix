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
# Process a file to CSV
rustpix process input.tpx3 -o output.csv

# Export to generic NeXus HDF5
rustpix process input.tpx3 -o output.h5

# Export to ORNL SNS NXsnsevent HDF5
rustpix process input.tpx3 -o output.nxs.h5 \
    --run-number 12345 --ipts IPTS-35004

# Export to TIFF stack with custom TOF binning
rustpix process input.tpx3 -o output.tiff \
    --tof-bins 500 --bit-depth 32

# Override auto-detected format
rustpix process input.tpx3 -o output.dat -f sns-hdf5

# Show file info
rustpix info input.tpx3

# Benchmark algorithms
rustpix benchmark input.tpx3

# Get help
rustpix --help
rustpix process --help
```

## Output Formats

The output format is auto-detected from the file extension, or can be overridden with `--format` (`-f`):

| Extension | Format | Description |
|-----------|--------|-------------|
| `.csv` | `csv` | Comma-separated values with header |
| `.h5`, `.hdf5`, `.nxs` | `hdf5` | Generic NeXus HDF5 (scipp-compatible) |
| `.nxs.h5` | `sns-hdf5` | ORNL SNS NXsnsevent HDF5 with run metadata |
| `.tif`, `.tiff` | `tiff` | TIFF image stack with spectrum CSV sidecar |
| `.bin`, `.dat`, other | `binary` | Compact binary format (default) |

### Format-Specific Flags

| Flag | Formats | Default | Description |
|------|---------|---------|-------------|
| `--format` (`-f`) | All | Auto-detect | Override output format |
| `--run-number` | `sns-hdf5` | `0` | SNS run number |
| `--ipts` | `sns-hdf5` | `""` | Experiment identifier (e.g., `IPTS-35004`) |
| `--instrument` | `sns-hdf5` | `venus` | Instrument preset |
| `--tof-bins` | `tiff` | `200` | Number of TOF bins |
| `--tof-max` | `tiff` | Auto | Maximum TOF in 25ns ticks |
| `--bit-depth` | `tiff` | `16` | Bit depth (`16` or `32`) |

See [Commands Reference](commands.md) for detailed usage.
