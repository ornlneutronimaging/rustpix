# rustpix-cli

`rustpix` clusters the hits in Timepix3 (`.tpx3`) files into neutron events and
writes them as CSV, binary, NeXus HDF5 or SNS HDF5, or bins them into a TIFF
time-of-flight stack. It is the command-line tool of
[rustpix](https://github.com/ornlneutronimaging/rustpix).

## Install

```bash
cargo install --locked rustpix-cli
```

This builds HDF5 from source and needs CMake and a C compiler. Prebuilt binaries
are attached to each [GitHub release](https://github.com/ornlneutronimaging/rustpix/releases).

## Commands

| Command | Action |
|---|---|
| `process <INPUT>... -o <OUTPUT>` | Cluster hits and write neutron events |
| `info <INPUT>` | Print packet and hit counts and TOF, x and y ranges |
| `benchmark <INPUT>` | Time ABS, DBSCAN and Grid on the file |
| `out-of-core-benchmark <INPUT>` | Time out-of-core processing with one and with several workers |

`rustpix <command> --help` lists every flag and its default.

## Examples

```bash
# CSV, default ABS (Age-Based Spatial) clustering
rustpix process run.tpx3 -o events.csv

# Two files into one HDF5 file, Grid clustering, 3-pixel radius
rustpix process run1.tpx3 run2.tpx3 -o events.h5 -a grid --radius 3
```

The output format follows the extension of `-o` (`.csv`, `.h5`, `.nxs.h5` for
SNS, `.tif`; anything else is binary) or `-f`. `-a` selects `abs`, `dbscan` or
`grid`.

## License

MIT. See [LICENSE](https://github.com/ornlneutronimaging/rustpix/blob/main/LICENSE).
