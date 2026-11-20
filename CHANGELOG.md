# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed

- ABS clustering split a cluster into single-hit clusters when the cluster
  started at y >= 288 in a pulse whose hits all lay in a narrow strip at low x
  (for example a sparse pulse on the lower-left chip of a quad detector).
  Each hit was then reported as its own neutron.
- ABS and Grid clustering missed neighbours farther apart than one 32-pixel
  grid cell: with a radius above 32, or for an ABS cluster more than about
  33 pixels across. Grid now uses a cell at least as large as the radius.
- DBSCAN counted a neighbour twice when `epsilon` was above 32 and the hits
  spanned a single grid column, which could form a cluster below `min_points`.
- DBSCAN panicked when a `DbscanState` was reused for a batch covering a
  larger area than the previous one.
- `SpatialGrid::remove` took pixel coordinates but looked up a cell index, so
  it removed nothing.
- The weighted centroid was wrong for a cluster whose summed ToT exceeded
  `u32::MAX`.
- `rustpix process` created (and so truncated) the output before opening any
  input. A missing input emptied an existing output file, and naming the
  input as the output destroyed it. Inputs are now checked first, an output
  that is one of the inputs or ends in `.tpx3` is refused, and the output is
  written to a temporary file and moved into place only on success.
- TIFF export overwrote an existing `<stem>.csv` with its spectrum, even an
  unrelated event file. It now stops unless that file is empty or a spectrum
  from an earlier run. With `-f tiff -o file.csv` the verbose log no longer claims a
  spectrum was written.
- TIFF export counted every neutron with a TOF above `--tof-max` in the last
  bin. Such neutrons are left out and reported in a warning.
- `--tof-bins` values too large to allocate ended in an abort or a panic; they
  are now an ordinary error, reported before any data is read.
- Files with no TPX3 data (empty or not TPX3) were processed "successfully"
  into an empty output. `process`, `info` and both benchmarks now reject them.
- `benchmark --iterations 0` and `out-of-core-benchmark --iterations 0`
  printed NaN and infinities; zero iterations is now an error.
- An error while writing the TIFF spectrum CSV could be lost.
- The one-shot HDF5 writers (`write_hits_hdf5`, `write_neutrons_hdf5`,
  `write_combined_hdf5[_batches]`, `write_histogram_hdf5`, `write_hits_sns`,
  `write_neutrons_sns`) write to a temporary file and rename it into place on
  success. A failed export used to leave a truncated file that read back as
  valid, and could destroy an existing file before failing.
- The SNS reader ignored `units`, `offset_seconds` and `offset_nanoseconds`
  attributes stored as 1-element arrays and silently used defaults (TOF could
  come back 1000x too large; pulse times run-relative instead of absolute).
- String metadata with invalid UTF-8 produced a `String` violating Rust's
  UTF-8 guarantee; such values are now treated as missing (SNS) or an error
  (generic attributes).
- File values that used to wrap or panic are errors: a non-finite TOF, a
  pulse time beyond `u64` nanoseconds, an oversized bank, and out-of-range
  pulse timestamps given to the SNS writer.
- The generic hit writer accepted coordinates outside `x_size` x `y_size`,
  giving two pixels the same `event_id`. Both event writers reject batches
  whose columns differ in length, as does the SNS writer.
- The generic readers check that per-event columns agree, that
  `event_index` is ordered and in range, and that histogram axes match the
  counts shape.
- `Hdf5HistogramSink` lost every cached chunk after one failed chunk write.
- `SnsEventSink::finalize` reported success on a second call after a failed
  first one.
- `SnsEventSink::finalize` replaced a caller-supplied `duration` whenever
  events were written, and a caller-supplied `end_time` when the start time
  also parsed.
- ISO 8601 offsets without a colon (`+0530`) were read as whole hours, and
  out-of-range dates and times were accepted; both are now rejected.

- GUI: a failed HDF5 export deleted the file chosen as destination even when
  the export never wrote to it (for example a read-only or locked file).
  Exports now write to a temporary file in an owner-only directory beside
  the destination and replace it only on success. The replaced file keeps its permissions; a symbolic link chosen as
  the destination is replaced by the file, not followed.
- GUI: cancelling a TPX3 load could leave the loader threads blocked for the
  rest of the session, holding the mapped file and partial data.
- GUI: results of a cancelled or superseded load or clustering run could be
  applied to the newly selected file; each job now has its own message
  channel and cancel flag.
- GUI: re-running clustering in the Neutrons view left the previous image on
  screen, and the TOF slice index was not reset for new data.
- GUI: deleting a polygon vertex, or a rejected vertex drag, left the ROI
  spectrum stale; deleting a vertex could also make the polygon cross itself.
- GUI: a zero-byte or headerless `.tpx3` file panicked the loader or loaded
  as an empty "Ready" dataset, and a large non-TPX3 file made the section
  scan crawl; these are now load errors.
- GUI: the spectrum TOF axis and spectrum CSV followed the live TDC
  frequency control instead of the loaded data's binning.
- GUI: the generic HDF5 export wrote all neutrons as one pulse at TDC 0; it
  now writes one pulse per recorded pulse.
- GUI: SNS NeXus TOF values were truncated to 25 ns ticks; events above
  16.384 ms could land one tick early. They are now rounded.
- GUI: events with TOF beyond the histogram range were added to the last
  bin; they are now left out and counted in the status line.
- GUI: the spectrum CSV dropped rows whose energy is undefined; they are now
  kept with an empty energy cell. A non-numeric SNS run number is refused
  instead of being written as 0.

### Changed

- Clustering rejects invalid parameters with `ClusteringError::InvalidConfig`
  instead of panicking or silently misbehaving: a NaN, infinite or negative
  radius, epsilon or temporal window, and an ABS scan interval of 0.
- ABS lists a cluster in every grid cell its bounding box covers. A hit that
  fits two open clusters can now join the other one, which can also change
  the cluster count; in synthetic full-detector tests this affected about
  0.01% of hits on very dense pulses and none on sparse ones. Single-thread
  ABS throughput is about 5% lower.
- `SpatialGrid::insert` and `remove` ignore a `Default` (cell-less) grid
  instead of dividing by zero.
- `rustpix process` replaces an existing output file only when the run
  succeeds, keeping that file's permissions. A symbolic link given as `-o` is
  replaced by the new file rather than followed.
- `rustpix info` reads the file pulse by pulse instead of loading every hit.
- `rustpix benchmark` clusters each pulse separately, as `process` does, and
  no longer times the copy of the input. Its first output line now also
  reports the pulse count.
- One-shot HDF5 exports keep an existing destination's permissions; a
  symbolic link at the destination is replaced by the new file, not followed.
  The file is written in an owner-only directory beside the destination, so
  other users can neither read it nor redirect the write.
- `SnsEventSink::create` rejects bank geometries whose pixel IDs or
  coordinates do not fit, and a run duration that is not between 0 and 1e10 s.
- The SNS reader returns an error for an unparseable ISO `offset` on
  `event_time_zero` when there is no EPICS offset to fall back on.
- `docs/hdf5_schema.md` and the book describe the pixel-mask group, neutron
  `f64` coordinates and `super_resolution_factor`, the SNS `title` and the
  `(1,)` shape of a bank's `total_counts`.


## [1.4.1] - 2026-10-09

### Fixed

- `rustpix-io` with the `hdf5` feature, and `cargo install rustpix-cli`, failed
  to build without the repository's lockfile: `hdf5-metno` resolved to
  `ndarray` 0.17 while `rustpix-io` required 0.16. `rustpix-io` now uses
  `ndarray` 0.17.
- `max_cluster_size` (Python `ClusteringConfig`, the GUI cluster-size limit)
  is applied: clusters with more hits are dropped, for all three algorithms.
  In 1.4.0 it had no effect.

### Changed

- The crates.io READMEs describe the current API, with one compiling example
  each.

## [1.4.0] - 2026-10-09

### Added

- `Tpx3FileReader::read_batch_with_pulse_starts` returns the first hit index of
  each pulse, and `rustpix_algorithms::cluster_and_extract_pulses` clusters a
  multi-pulse batch pulse by pulse. (#140)

### Fixed

- Python: `cluster_hits` on a `read_tpx3_hits` batch and
  `process_tpx3_neutrons(collect=True, time_ordered=False)` cluster each pulse
  separately, so hits from different pulses are no longer merged into one
  neutron. (#140)
- DBSCAN: `min_points` counts the hit itself. At the default of 2 an isolated
  pair forms a cluster; a setting of N behaves like N - 1 did in 1.3.0.
  `min_points = 1` keeps isolated hits. (#136)
- Threaded out-of-core pipeline (`parallelism > 1` or `async_io`): a pulse
  split to fit the memory budget yields one batch, as in the single-threaded
  pipeline. (#138)
- `Hdf5HistogramSink` writes its buffered counts when dropped without
  `flush()`. (#139)

### Changed

- Docs: clustering input requirements (one pulse per call; ascending TOF order
  for ABS and Grid) and how each algorithm applies `radius` and
  `temporal_window_ns`. The user guide's description of ABS matches the
  implementation. (#135, #137)
- `rustpix-tpx`: removed the unused `parser` and `error` source files and the
  `thiserror` and `rayon` dependencies. (#134)

## [1.3.0] - 2026-10-02

### Added

- GUI: the image view and spectrum plot can be resized against each other —
  a drag handle now sits between them, the spectrum plot fills whatever
  height it is given (it was fixed at 140 px), and the chosen split is
  persisted across restarts in `~/.config/venus_rust_tools/rustpix_layout`.
  The image keeps at least 200 px and the spectrum at least 120 px, so
  neither can be dragged away entirely.

- GUI: the load progress bar now shows a time-remaining estimate
  ("Processed 12,000,000 hits... — ~45s left") while reading TPX3 and SNS
  NeXus files, and during clustering. The rate is measured over a 10-second
  sliding window of progress samples, so the estimate tracks the current
  phase of a load instead of averaging over everything since the start, and
  it counts down between progress messages. The estimate appears once about
  a second of history exists, disappears while progress is stalled, and is
  deliberately rounded (10-second granularity above one minute).

- GUI: the export dialogs (TIFF, HDF5, SNS HDF5, spectrum CSV and PNG) now
  start in the experiment's `IPTS-####/shared` folder, taken from the loaded
  file's path. When the file is not inside an experiment folder, or the
  folder has no `shared/`, they start in the last used directory.

### Fixed

- GUI: the TIFF export base name now follows the loaded file. Previously,
  after a second file was loaded, the export dialog still proposed the
  first file's name.

### Changed

- Homebrew: the cask template uses `postflight_steps` instead of the
  deprecated `postflight` stanza, matching the tap.

## [1.2.1] - 2026-08-25

### Fixed

- GUI wheel build: v1.2.0's switch to rfd's gtk3 backend broke the Linux
  manylinux wheel (the build container has no GTK3 dev libs, and a wheel
  linking libgtk-3 would fail manylinux compliance anyway), so no 1.2.0
  `rustpix-gui` package reached PyPI. The backend is now selected by cargo
  feature: `dialog-gtk3` (default — honors the file dialog's starting
  directory, needs GTK3 at build time) or `dialog-portal` (no GTK link,
  ignores the starting directory), and the wheel builds with
  `dialog-portal`.

## [1.2.0] - 2026-08-25

### Added

- GUI: the same file-open tools as `rust_nexus_viewer` — the Open dialog now
  starts in the current (or most recently opened) file's directory, a ⌨
  toolbar button opens a type/paste-a-path modal (a file path loads
  directly, a directory path starts the browser there, `~/` expands), a 🕘
  menu reopens one of the last 5 files (persisted in
  `~/.config/venus_rust_tools/rustpix_recent`, deduplicated across /SNS
  vs /gpfs mounts), and a `.tpx3` file can be dropped onto the window to
  load it. Requires switching `rfd` to the gtk3 backend, since the default
  xdg-portal one ignores the requested starting directory.

- GUI: SNS NeXus event files (`.nxs.h5`, `.h5`, `.hdf5`, `.nxs`) can now be
  loaded like TPX3 files — via the Open dialog, the path popup, the recent
  menu, or drag-and-drop. VENUS `bank100` events are read in 2M-event
  slices, decoded on the gap-removed 512×512 grid, converted to 25 ns TOF
  ticks, and grouped per pulse (sorted by TOF within each pulse), so
  rebinning, ROI spectra, and export all work as after a TPX3 load. NeXus
  events carry no ToT (stored as 0), and clustering stays TPX3-only (the
  button explains why). Loading a facility ADARA file, whose event banks
  are empty (VENUS Timepix data is recorded in `.tpx3` files), reports a
  clear error pointing at the run's `.tpx3` file.
- IO: `sns_read_summary` example prints a NeXus file's banks, run
  metadata, and event statistics from a full sliced read of `bank100`
  (`cargo run -p rustpix-io --features hdf5 --example sns_read_summary -- <file>`).

- IO: `SnsEventReader` reads SNS NeXus event files (`*.nxs.h5`) — both
  facility files written by the ADARA translation service and rustpix's own
  `NXsnsevent` exports. Banks are discovered from `entry/<bank>_events`
  groups; events can be read in slices so multi-billion-event files fit in
  memory a chunk at a time. Pixel IDs are decoded back to x/y with the bank
  configuration, TOF is converted to nanoseconds from the recorded units,
  and per-event pulse timestamps are reconstructed as absolute Unix-epoch
  nanoseconds (ADARA's `offset_seconds` counts from the EPICS 1990-01-01
  epoch; the ISO `offset` attribute is preferred when present). Also
  exposes best-effort run metadata (`run_number`, `start_time`, proton
  charge, …). One-shot helper: `read_sns_events_venus`.
- Python: `read_sns_events(path, bank="bank100", start=0, count=None, ...)`
  returns event slices as NumPy arrays (`event_id`, `x`, `y`, `tof_ns`,
  `pulse_time_ns`), and `sns_file_info(path)` summarizes a file's banks and
  run metadata. Defaults match VENUS bank100 (pixel-ID offset 1,000,000 on
  a 512×512 gap-removed grid); other banks take an explicit
  `pixel_id_offset`.

## [1.1.3] - 2026-07-28

### Changed

- GUI: Raised the TOF bin limit in Hyperstack Settings from 2,000 to 1,000,000
  for both the hits and neutrons hyperstacks. The old cap stood in for a memory
  guard; it also blocked bin counts that instruments legitimately need. The
  limit is now a rail against mistyped input — a 60 Hz source carries no
  information past ~666,667 bins, and memory binds well before that.
- GUI: TOF bin drag sensitivity now scales with the current value, so the drag
  gesture can still traverse the wider range. Typing an exact value is
  unchanged.

### Added

- GUI: Hyperstack Settings now estimates the memory each hyperstack will need
  at the selected bin count, and warns when the estimate exceeds free system
  memory. Hyperstack storage is dense, so cost is linear in bin count: about
  2 MB per bin on a 514×514 VENUS detector, so 10,000 bins reads as
  "19.68 GB" in the settings window. The warning is advisory only and never
  blocks a rebuild, since free-memory readings are unreliable under cgroup
  limits and on cluster nodes.

### Notes

Existing behaviour that becomes reachable now that higher bin counts are
allowed. None of it is changed by this release.

- Exporting to HDF5 builds a second, transposed copy of the whole hyperstack
  before writing, so an export needs roughly double the stack's memory and is
  the slowest operation at high bin counts.
- Exporting a TIFF *stack* above ~8,128 bins (514×514, 16-bit) exceeds the 4 GB
  standard-TIFF limit. It is already reported with a clear error; use "TIFF
  Folder", or set the stack behaviour to "Auto BigTIFF if needed" or "Always
  BigTIFF".
- Rebuilding a hyperstack runs on the UI thread with no progress bar, so at high
  bin counts the window will be unresponsive for the duration of the rebuild.

## [1.0.5] - 2026-02-05

### Added

- GUI: TIFF export format support for images
- GUI: Histogram view transforms with ROI remapping

## [1.0.4] - 2026-02-04

### Fixed

- macOS app bundle: Use working-directory to build from rustpix-gui crate, avoiding workspace-level build that includes rustpix-python

## [1.0.3] - 2026-02-03

### Fixed

- macOS app bundle: Excluded rustpix-python from build to fix linker errors (failed - invalid flag)

## [1.0.2] - 2026-02-03

### Fixed

- PyPI metadata: Fixed LICENSE file inclusion in source distribution

## [1.0.1] - 2026-02-03

### Fixed

- GitHub Actions workflow: Updated macOS runner from retired macos-13 to macos-15
- GitHub Actions workflow: Fixed publish job conditions to handle tag-triggered releases
- PyPI publishing now works correctly for tag-triggered releases

## [1.0.0] - 2026-02-03

### Added

#### Core Functionality
- Timepix3 (TPX3) packet parser with parallel processing
- Memory-mapped file I/O for efficient large file handling
- Streaming architecture for processing files larger than RAM
- Hit and Neutron trait system for detector-agnostic interfaces

#### Clustering Algorithms
- ABS (Adjacency-Based Search) - Fast 8-connectivity clustering
- DBSCAN - Density-based clustering with spatial indexing
- Graph - Union-find based connected component detection
- Grid - Parallel grid-based clustering with spatial indexing
- Configurable spatial and temporal epsilon parameters

#### Python Bindings
- Thin PyO3 wrappers for Rust pipelines
- NumPy structured array (SoA) outputs
- Streaming and batch processing modes
- Configuration objects for clustering and extraction

#### CLI Tool
- `rustpix process` - Process TPX3 files with clustering
- `rustpix info` - Display file information and metadata
- `rustpix benchmark` - Benchmark clustering algorithms
- `rustpix convert` - Convert between output formats

#### GUI Application
- Interactive TPX3 file loading and processing
- Real-time visualization of hits and neutron events
- Algorithm selection and parameter tuning
- Export to multiple formats (HDF5, CSV, Arrow)
- ROI (Region of Interest) selection tools
- Memory usage monitoring

#### Output Formats
- HDF5 with hierarchical structure
- Apache Arrow/Parquet
- CSV for simple data export
- Binary formats for performance

#### Release Infrastructure
- Automated version management via pixi tasks
- Multi-platform GitHub release workflow
  - Python wheels (Linux, macOS, Windows)
  - CLI binaries for all platforms
  - macOS .app bundle with DMG installer
- PyPI publishing with maturin
- crates.io publishing for Rust crates
- Homebrew tap for macOS installation
- Comprehensive CI/CD pipeline

#### Documentation
- Rust API documentation (docs.rs)
- Python API docstrings
- README with installation and usage examples
- Per-crate README files
- CHANGELOG following Keep a Changelog format

### Technical Details

#### Performance
- 96M+ hits/sec throughput on modern hardware
- SIMD-friendly Structure-of-Arrays (SoA) data layout
- Multi-threaded processing with rayon
- Zero-copy operations where possible

#### Architecture
- Workspace structure with modular crates
- Trait-based design for extensibility
- Static HDF5 linking for portability
- Cross-platform support (Linux, macOS, Windows)

#### Testing
- Comprehensive test suite for all algorithms
- CI testing on multiple platforms
- Coverage reporting with codecov
- Pre-commit hooks for code quality

### Known Limitations
- Currently supports TPX3 format only (TPX4 planned)
- GUI is macOS/Linux only (Windows support in progress)
- HDF5 output requires static linking

---

## Release Process

To create a new release:

1. Update version: `pixi run version-major` (or `minor`/`patch`)
2. Update this CHANGELOG with release date
3. Commit changes: `git add -A && git commit -m "chore: release vX.Y.Z"`
4. Create tag: `git tag vX.Y.Z`
5. Push: `git push && git push --tags`
6. GitHub Actions will automatically:
   - Build all artifacts
   - Publish to PyPI and crates.io
   - Create GitHub Release
   - Update Homebrew tap

[Unreleased]: https://github.com/ornlneutronimaging/rustpix/compare/v1.0.0...HEAD
[1.0.0]: https://github.com/ornlneutronimaging/rustpix/releases/tag/v1.0.0
