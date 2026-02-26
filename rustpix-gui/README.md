# rustpix-gui

GUI application for rustpix pixel detector processing.

## Installation

### macOS (Homebrew)

```bash
brew tap ornlneutronimaging/rustpix
brew install --cask rustpix
```

### pip

```bash
pip install rustpix-gui
rustpix-gui
```

### From Source

```bash
cargo run --release -p rustpix-gui
```

## Features

- Interactive file loading (open or drag-and-drop TPX3 files)
- Real-time 2D detector map visualization
- ABS, DBSCAN, and Grid clustering algorithms
- Parameter tuning with immediate visual feedback
- ROI selection for focused analysis
- Export to HDF5 (NeXus), HDF5 (SNS NXsnsevent), TIFF, and CSV

## Documentation

See the [full documentation](https://ornlneutronimaging.github.io/rustpix/gui/).
