# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
