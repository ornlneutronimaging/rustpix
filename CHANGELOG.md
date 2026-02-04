# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [1.0.3] - 2026-02-03

### Fixed

- macOS app bundle: Excluded rustpix-python from build to fix linker errors

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
