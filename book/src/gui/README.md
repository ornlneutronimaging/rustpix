# GUI Application

The rustpix GUI provides interactive visualization and analysis of TPX3 data.

## Installation

### macOS (Homebrew)

```bash
brew tap ornlneutronimaging/rustpix
brew install --cask rustpix
```

### From Source

```bash
# Using pixi
pixi run gui

# Using cargo
cargo run --release -p rustpix-gui
```

## Features

- **Interactive file loading**: Open TPX3 or SNS NeXus (`.nxs.h5`) files via file dialog or drag-and-drop
- **Real-time visualization**: View hits and neutron events on 2D detector maps
- **Algorithm selection**: Choose between ABS, DBSCAN, and Grid clustering
- **Parameter tuning**: Adjust clustering parameters with immediate visual feedback
- **ROI selection**: Define regions of interest for focused analysis
- **Export options**: Save processed data to HDF5, CSV, TIFF, and other formats
- **Memory monitoring**: Track memory usage during processing

## Launching

### macOS

- **Spotlight**: Search for "Rustpix"
- **Applications**: Find in `/Applications`
- **Terminal**: `open -a Rustpix`

### From Source

```bash
# Release mode (faster)
pixi run gui

# Debug mode (faster compilation)
pixi run gui-debug
```

## Workflow

### 1. Load Data

1. Click **File > Open** or drag a `.tpx3` or NeXus (`.h5`/`.nxs.h5`) file onto the window
2. Wait for the file to load (progress shown in status bar)
3. Raw hits appear in the visualization panel

NeXus loading reads VENUS `bank100` events (from rustpix's own NXsnsevent
exports — facility ADARA files have empty event banks, since VENUS Timepix
data is recorded in `.tpx3` files). NeXus events carry no time-over-threshold,
and clustering is only available for TPX3 files.

### 2. Configure Processing

1. Select clustering algorithm from the dropdown
2. Adjust parameters:
   - **Radius**: Spatial clustering distance (pixels)
   - **Temporal Window**: Time clustering window (nanoseconds)
   - **Min Cluster Size**: Filter small clusters

### 3. Process

1. Click **Process** to run clustering
2. Neutron events appear in the visualization
3. Statistics shown in the info panel

### 4. Analyze

- **Pan/Zoom**: Mouse wheel and drag to navigate
- **ROI**: Draw regions of interest for statistics
- **Histogram**: View ToF and spatial distributions

### 5. Export

1. Click **File > Export**
2. Choose format:
   - **HDF5 (NeXus)**: Generic NeXus event data, scipp-compatible (`.h5`)
   - **HDF5 (SNS NXsnsevent)**: ORNL SNS/HFIR schema with run metadata (`.nxs.h5`)
   - **CSV**: Spectrum CSV (via spectrum toolbar, not File > Export)
   - **TIFF**: Image stack with TOF bins and spectrum CSV sidecar (`.tiff`)
3. Select output location

## Keyboard Shortcuts

| Shortcut | Action |
|----------|--------|
| `Cmd+O` | Open file |
| `Cmd+S` | Save/Export |
| `Cmd+Q` | Quit |
| `Space` | Toggle processing |
| `R` | Reset view |
| `Escape` | Cancel ROI selection |

## System Requirements

- macOS Big Sur (11.0) or later
- Apple Silicon (ARM64) recommended
- 8GB RAM minimum (16GB recommended for large files)
- OpenGL 3.3 or later

## Troubleshooting

### App Won't Open (macOS)

If macOS blocks the app:
1. Go to **System Preferences > Security & Privacy**
2. Click **Open Anyway** for Rustpix

### Out of Memory

For very large files:
- Enable streaming mode in preferences
- Reduce the loaded time range
- Use the CLI for batch processing instead

### Slow Rendering

- Reduce the number of displayed points (use downsampling)
- Close other applications to free GPU memory
- Try the CLI for processing, GUI for visualization only
