# Rustpix GUI - Design Document v1.0

## Changelog

| Date       | Version | Changes                                                      |
|------------|---------|--------------------------------------------------------------|
| 2025-01-30 | v1.0    | Initial design document based on beamline scientist workflow |
| 2026-01-31 | v1.1    | Refactor progress update (TOF ms axis, settings dialogs, grid toggle, scrollable sidebar, spectrum export) |
| 2026-02-01 | v1.2    | Phase 4 closed (HDF5 export + pixel masks + async export); Phase 5+ roadmap added |
| 2026-02-01 | v1.3    | Phase 6 closed (advanced clustering/extraction controls + detector profiles); precision inputs added |

---

This document captures the complete workflow and design for the rustpix GUI application,
based on real-world usage scenarios from beamline scientists and detector experts.

## Design Philosophy

> "Simple, informative, and professional - like ImageJ for TIFF files, but for TPX3 event data"

### Core Principles

1. **Progressive disclosure** - Simple defaults, advanced options behind gear icons
2. **Smart caching** - Cache when memory permits, regenerate when fast enough
3. **Unified viewer** - Same tools for Hits and Neutrons, just toggle data source
4. **NeXus compliance** - Metadata (flight path, TOF offset) saved per-file in HDF5

---

## Implementation Status (as of 2026-02-01)

**Implemented (GUI refactor branch):**
- TOF axis uses **milliseconds** by default (consistent with 1/60 Hz = 16.67 ms)
- Spectrum toolbar: logX/logY toggles, PNG/CSV export, gear for energy conversion settings
- Energy axis toggle (TOF ms ↔ Energy eV) with flight-path + TOF offset inputs
- Hyperstack settings dialog (top-right gear) with separate Hits/Neutrons TOF bins
- Super-resolution control in clustering advanced panel
- Scrollable left control panel (prevents panels from being clipped)
- Viewer grid toggle button next to Reset View (default OFF)
- TOF slicer row spans full width (visual alignment with mock)

**Completed (Phase 4 - 2026-02-01, closed):**
- HDF5 export pipeline (single file, hits/neutrons/histogram, metadata)
- Pixel mask workflow (dead/hot detection, overlay, export to HDF5)
- Export dialog with data selection + advanced options
- Async HDF5 export with progress indication (UI remains responsive)

**Planned (Phase 5+ roadmap):**
- Phase 5 (Telemetry + Diagnostics)
  - Memory utilization indicator in status bar (per-process), hover breakdown by major buffers **(Completed - 2026-02-01)**
  - Export validation utilities (basic file integrity + compression sanity checks) **(Completed - 2026-02-01)**
  - Pixel mask controls: exclude from stats + recompute action **(Completed - 2026-02-01)**
- Phase 6 (Advanced Configuration)
  - Clustering/extraction advanced settings (missing fields + reset defaults) **(Completed - 2026-02-01)**
  - Detector configuration profiles (presets + per-file overrides) **(Completed - 2026-02-01)**
  - Export options dialog expansion (compression level, chunk sizing, include fields)
- Phase 7 (Streaming + Resilience)
  - Full streaming pipeline + cancel flows for end-to-end large files **(Completed - 2026-02-02)**
  - Progressive loading indicators + background task management **(Completed - 2026-02-02)**

**Implemented (Phase 2 ROI foundation):**
- Multi-ROI drawing (rectangle + polygon) with shift-to-draw and edit mode
- ROI selection, move, resize, delete (context menu, Delete key, Clear All)
- Polygon editing (insert/move/delete vertices) with self-intersection validation
- Concave polygon fill via triangulation (consistent filled overlay)
- ROI persistence across Hits ↔ Neutrons view switches
- ROI tool group icons (SVG) and settings (debounce toggle)

**Implemented (Phase 3 ROI spectrum integration):**
- Data Selection panel (show/hide Full FOV + ROIs, quick actions, rename)
- Multi-curve spectrum display with ROI-matched colors
- Separate legend panel
- CSV export for visible spectra with ROI metadata + energy column
- Spectrum range panel, zoom tools, and PNG export parity with viewer

---

## User Workflow

### Stage 0: File Loading (Raw → Hits)

**Trigger:** User opens a TPX3 file

**Behavior:**
- Processing starts automatically (no separate "Run" button)
- Progress bar shows overall progress AND current stage (e.g., "Scanning sections...", "Processing hits...")
- Uses out-of-core streaming to handle files larger than RAM
- Cancel button to abort at any time

**On Completion - Statistics Panel:**

| Metric | Example |
|--------|---------|
| Total hits | 12,345,678 |
| TOF range | 0.0 - 16.67 ms |
| Processing speed | 45,000,000 hits/sec |
| Duration | 0.27 s |

*(Numbers formatted with comma separators for readability)*

---

### Stage 1: Hits Analysis

After loading, the user enters **Hits Analysis Mode** with these tools:

#### 1.1 Histogramming Engine

- Converts event data → hyperstack `data[tof, y, x]`
- **TOF binning:** Machine-appropriate (based on TDC frequency)
- **X/Y binning:** Native detector resolution (no super-resolution at this stage)

#### 1.2 Views

**A. 2D Histogram View (Default)**
- All TOF bins collapsed (sum projection)
- Cursor hover shows `(x, y, counts)`
- Default colormap: **Grayscale**
- Reset View fits the full detector into the current plot area with symmetric padding
- Grid toggle (OFF by default) for alignment and measurement

**B. Hyperstack Slicer View**
- Slider to navigate through TOF bins
- View individual TOF slices
- Current slice position indicated on spectrum

**C. Spectrum Viewer** (toggleable window/panel)
- Default: `(TOF, counts)` integrated over full detector
- Features:
  - **Axis toggle:** TOF (ms) ↔ Energy (eV)
    - Requires user input: flight path (m), TOF offset (ns)
    - Per-file setting, saved to HDF5 metadata per NeXus format
  - **Plot style:** Line plot (default)
- **Toolbar:** Log X, Log Y toggles
- **Export:** Save PNG, Export CSV
- **Reset:** Rescale axes to visible data range
- **ROI Integration:**
  - Each ROI in histogram view adds a curve to spectrum
  - Data Selection panel: show/hide + rename ROIs
  - Legend panel (separate from toolbar)
  - CSV/PNG export matches visible curves

#### 1.3 ROI Tools

- Add multiple ROIs on histogram view (rectangle + free-form polygon)
- Shift+drag or shift+click to draw; default state supports move/selection
- Edit mode: resize rectangle or edit polygon vertices (insert/move/delete)
- Delete via context menu, Delete key, or Clear All
- ROIs persist when switching between Hits and Neutrons mode
- Spectrum integration and data selection panel (Phase 3)

---

### Stage 2: Neutron Analysis (Clustering)

**Trigger:** User initiates clustering (explicit action via "Run Clustering" button)

**Default Algorithm:** ABS (Adaptive Box Search) - fastest

**Algorithm Selection:**
- Simple dropdown for method selection (ABS, DBSCAN, Grid)
- Advanced parameters hidden behind gear icon

**Behavior:**
- Progress bar with stage information
- Streaming + out-of-core processing
- **Cancel button** to abort at any time

**On Completion - Statistics Panel:**

| Metric | Example |
|--------|---------|
| Neutrons identified | 1,234,567 |
| Processing speed | 8,500,000 neutrons/sec |
| Cluster size (avg) | 3.2 hits |
| Duration | 0.15 s |

---

### Stage 3: Neutron Visualization

After clustering, the user can **switch the viewer to Neutron mode**:

- Same histogram view, slicer, and spectrum tools
- Now operating on neutron events instead of hits
- Toggle: **[Hits ▼]** dropdown to switch data source
- ROIs defined in Hits mode persist and apply to Neutron data

---

### Stage 4: Export to HDF5

**Options:**
- Select data to save: Hits, Neutrons, or both
- Compression enabled by default
- Include masks (dead/hot pixels) if generated
- Include metadata (flight path, TOF offset) per NeXus format

---

### Global Features

**Abort/Cancel:**
- During any processing stage (loading, clustering), user can abort
- Returns to previous valid state

**Smart Caching:**
- If histogram generation is fast enough, regenerate on-the-fly
- If memory permits, cache for faster response time

---

### Bonus: Dead/Hot Pixel Detection

Primarily relevant for **Hits** analysis:

| Type | Definition | Visualization |
|------|------------|---------------|
| **Dead pixels** | Zero counts across all TOF bins | Black (natural) |
| **Hot pixels** | Counts > threshold (default: 5σ) | **Red** (outside colormap) |

**Statistics Panel Addition:**
- Dead pixel count
- Hot pixel count
- Threshold control behind gear icon

**Mask Export:**
- Option to save mask to HDF5
- For Neutrons: mask still saved for statistical exclusion (scientific rigor)

---

## Application Layout

```
┌─────────────────────────────────────────────────────────────────────────────┐
│  File: sample_001.tpx3                                    [Hits ▼] [⚙]     │
├────────────────────┬────────────────────────────────────────────────────────┤
│                    │                                                        │
│   CONTROL PANEL    │                   MAIN VIEWER                          │
│                    │                                                        │
│  ┌──────────────┐  │   ┌────────────────────────────────────────────────┐   │
│  │ Statistics   │  │   │                                                │   │
│  │              │  │   │                                                │   │
│  │ Hits: 12.3M  │  │   │            2D Histogram View                   │   │
│  │ TOF: 0-16ms  │  │   │               (or Slicer)                      │   │
│  │ Speed: 45M/s │  │   │                                                │   │
│  └──────────────┘  │   │         [ROI 1]    [ROI 2]                     │   │
│                    │   │                                                │   │
│  ┌──────────────┐  │   │                                                │   │
│  │ Processing   │  │   └────────────────────────────────────────────────┘   │
│  │              │  │                                                        │
│  │ [Run Cluster]│  │   ┌────────────────────────────────────────────────┐   │
│  │ Algorithm:   │  │   │  TOF Slicer: [=====|================] 5/200    │   │
│  │ [ABS      ▼] │  │   └────────────────────────────────────────────────┘   │
│  │        [⚙]   │  │                                                        │
│  └──────────────┘  │   ┌────────────────────────────────────────────────┐   │
│                    │   │                                                │   │
│  ┌──────────────┐  │   │              Spectrum Viewer                   │   │
│  │ View Options │  │   │                                                │   │
│  │              │  │   │    ^                                           │   │
│  │ ☑ Slicer     │  │   │    │    /\      ___                           │   │
│  │ ☑ Spectrum   │  │   │    │   /  \____/   \___                       │   │
│  │ Colormap:    │  │   │    └──────────────────────>                   │   │
│  │ [Gray     ▼] │  │   │    [TOF ▼] [logX] [logY] [📷] [💾] [↺]        │   │
│  └──────────────┘  │   │    Legend: [■ Full] [■ ROI 1] [□ ROI 2]       │   │
│                    │   └────────────────────────────────────────────────┘   │
│  ┌──────────────┐  │                                                        │
│  │ Export       │  │                                                        │
│  │ [Save HDF5]  │  │                                                        │
│  └──────────────┘  │                                                        │
│                    │                                                        │
├────────────────────┴────────────────────────────────────────────────────────┤
│  Status: Ready │ Cursor: (256, 128) = 1,234 counts │ Dead: 12 │ Hot: 3     │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## Module Structure

```
rustpix-gui/src/
├── main.rs                 # Entry point, app initialization
├── app.rs                  # RustpixApp state and message handling
├── state/
│   ├── mod.rs
│   ├── app_state.rs        # Top-level application state
│   ├── processing.rs       # Loading/clustering state, progress
│   ├── data.rs             # HitData, NeutronData, cached histograms
│   └── session.rs          # Per-file metadata (flight_path, tof_offset)
├── ui/
│   ├── mod.rs
│   ├── control_panel.rs    # Left sidebar
│   ├── statistics.rs       # Statistics display widget
│   ├── processing.rs       # Algorithm selector, run button, progress
│   ├── view_options.rs     # Colormap, view toggles
│   ├── export.rs           # HDF5 save dialog
│   └── advanced/
│       ├── mod.rs
│       ├── clustering.rs   # Advanced clustering params
│       ├── extraction.rs   # Advanced extraction params
│       └── pixel_mask.rs   # Hot/dead pixel threshold
├── viewer/
│   ├── mod.rs
│   ├── histogram_2d.rs     # Main 2D view with ROI support
│   ├── slicer.rs           # TOF slice navigation
│   ├── spectrum.rs         # 1D spectrum plot
│   ├── roi.rs              # ROI data structure and rendering
│   └── colormap.rs         # Colormap implementations
├── pipeline/
│   ├── mod.rs
│   ├── loader.rs           # Raw → Hits pipeline (async)
│   ├── clustering.rs       # Hits → Neutrons pipeline (async)
│   └── histogram.rs        # Event → Hyperstack binning
├── histogram/
│   ├── mod.rs
│   ├── engine.rs           # N-dimensional histogramming
│   ├── projection.rs       # Sum/slice projections
│   └── cache.rs            # Smart caching logic
├── pixel_mask/
│   ├── mod.rs
│   ├── detector.rs         # Dead/hot pixel detection
│   └── mask.rs             # Mask data structure
└── export/
    ├── mod.rs
    ├── hdf5.rs             # HDF5 export (calls rustpix-io)
    └── csv.rs              # Spectrum CSV export
```

---

## State Machine

```
                    ┌─────────────┐
                    │    Empty    │
                    └──────┬──────┘
                           │ Open file
                           ▼
                    ┌─────────────┐
            ┌───────│   Loading   │←──────┐
            │       └──────┬──────┘       │
            │ Cancel       │ Complete     │ Open new file
            ▼              ▼              │
     ┌──────────┐   ┌─────────────┐       │
     │  Empty   │   │ HitsReady   │───────┤
     └──────────┘   └──────┬──────┘       │
                           │ Run Cluster  │
                           ▼              │
                    ┌─────────────┐       │
            ┌───────│ Clustering  │       │
            │       └──────┬──────┘       │
            │ Cancel       │ Complete     │
            ▼              ▼              │
     ┌───────────┐  ┌──────────────┐      │
     │ HitsReady │  │ NeutronsReady│──────┘
     └───────────┘  └──────────────┘
```

**View Mode** (orthogonal to processing state):
- `Hits` - viewing hit event data
- `Neutrons` - viewing neutron event data (only available in `NeutronsReady`)

---

## Data Flow

```
┌──────────────┐     ┌──────────────┐     ┌──────────────┐
│   TPX3 File  │────▶│  Hit Events  │────▶│   Neutrons   │
└──────────────┘     └──────┬───────┘     └──────┬───────┘
                            │                     │
                            ▼                     ▼
                     ┌──────────────┐      ┌──────────────┐
                     │  Hyperstack  │      │  Hyperstack  │
                     │ [tof, y, x]  │      │ [tof, y, x]  │
                     └──────┬───────┘      └──────┬───────┘
                            │                     │
              ┌─────────────┼─────────────┐       │
              ▼             ▼             ▼       ▼
        ┌──────────┐  ┌──────────┐  ┌──────────┐
        │ 2D Proj  │  │ TOF Slice│  │ Spectrum │
        │ sum(tof) │  │ [y, x]   │  │ [tof]    │
        └──────────┘  └──────────┘  └──────────┘
                            │             │
                            └──────┬──────┘
                                   ▼
                            ┌──────────────┐
                            │  ROI Curves  │
                            └──────────────┘
```

---

## Widget Specifications

### Statistics Widget

```
┌─ Statistics ──────────────────────────────┐
│                                           │
│  Hits:          12,345,678                │
│  TOF range:     0.0 - 16.67 ms            │
│  Processing:    45,230,000 hits/sec       │
│  Duration:      0.27 s                    │
│                                           │
│  ─── After Clustering ───                 │
│  Neutrons:      1,234,567                 │
│  Cluster avg:   3.2 hits                  │
│  Speed:         8,500,000 neutrons/sec    │
│                                           │
│  ─── Pixel Health ───                     │
│  Dead pixels:   12                        │
│  Hot pixels:    3                    [⚙]  │
└───────────────────────────────────────────┘
```

### Spectrum Toolbar

```
┌───────────────────────────────────────────────────────────────┐
│ [TOF (ms) ▼] [⚙] [logX] [logY] │ [📷 PNG] [💾 CSV] │ [↺]     │
└───────────────────────────────────────────────────────────────┘
        │
        ▼ Dropdown options
   ┌─────────────┐
   │ TOF (ms)    │
   │ Energy (eV) │  ← Only enabled if flight_path configured
   └─────────────┘
```

### Advanced Settings Panels

**Clustering ⚙:**
```
┌─ Clustering Parameters ───────────────────┐
│                                           │
│  Radius:              [5.0    ] pixels    │
│  Temporal window:     [75.0   ] ns        │
│  Min cluster size:    [1      ]           │
│  Max cluster size:    [       ] (empty=∞) │
│                                           │
│  ─── Algorithm-specific ───               │
│  DBSCAN min points:   [2      ]           │
│  Grid cell size:      [32     ]           │
│                                           │
│              [Reset to Defaults]          │
└───────────────────────────────────────────┘
```

**Extraction ⚙:**
```
┌─ Extraction Parameters ───────────────────┐
│                                           │
│  Super-resolution:    [8.0    ] factor    │
│  Weighted by TOT:     [☑]                 │
│  Min TOT threshold:   [10     ]           │
│                                           │
│              [Reset to Defaults]          │
└───────────────────────────────────────────┘
```

**Pixel Mask ⚙:**
```
┌─ Pixel Mask Settings ─────────────────────┐
│                                           │
│  Hot pixel threshold: [5.0    ] σ         │
│  Dead pixel def:      Zero counts across  │
│                       all TOF bins        │
│                                           │
│  [☑] Show hot pixels in red               │
│  [☑] Exclude from statistics              │
│                                           │
│              [Recalculate Mask]           │
└───────────────────────────────────────────┘
```

**Energy Conversion ⚙:**
```
┌─ Energy Conversion ───────────────────────┐
│                                           │
│  Flight path:         [10.5   ] m         │
│  TOF offset:          [1250   ] ns        │
│                                           │
│  [☑] Save to HDF5 metadata                │
│                                           │
└───────────────────────────────────────────┘
```

---

## Implementation Phases

| Phase | Title | Issues | Deliverables |
|-------|-------|--------|--------------|
| **1** | Architecture Refactor | NEW | Module structure, state machine, message passing |
| **2** | Core Loading Pipeline | #23 | File loading with progress, cancel, statistics |
| **3** | Histogram Engine | #27 | Hyperstack binning, projections, caching |
| **4** | 2D Histogram Viewer | #41 | Histogram view, colormap, cursor info, Hits/Neutrons toggle |
| **5** | Slicer & Spectrum | #28, #29 | TOF slicer, spectrum viewer, axis toggle, export |
| **6** | ROI System | NEW | Multi-ROI, drag/resize, spectrum integration |
| **7** | Clustering Pipeline | #24, #40 | Clustering with progress, extraction config |
| **8** | Pixel Mask | NEW | Dead/hot detection, visualization, mask export |
| **9** | Advanced Settings | #21, #22 | Detector config, clustering/extraction params, statistics panel |
| **10** | HDF5 Export | #26 | Save hits/neutrons with metadata and masks |
| **11** | Full Streaming | #25 | Memory-bounded end-to-end for large files |

---

## Issue Mapping (GitHub)

### Project Board
- Org project: **rustpix Sprint 1** (Project #2)
- Status values currently used: `Todo`, `Done`

### Open GUI Issues (repo: ornlneutronimaging/rustpix)

| Issue | Title | Notes |
|-------|-------|-------|
| #20 | [EPIC] GUI Revamp: Multi-format support, pipelines, and visualization overhaul | Phase tracker |
| #80 | [GUI] Architecture refactor | State machine + module cleanup |
| #23 | [GUI] Pipeline architecture: Raw → Hits | Auto processing + progress + cancel |
| #27 | [GUI] N-dimensional histogramming engine | Hyperstack + projections |
| #41 | [GUI] Visualize extracted neutron histogram | Hits/Neutrons viewer toggle |
| #28 | [GUI] Slice viewer widget for nD data | TOF slicer |
| #29 | [GUI] 1D histogram plot improvements | Spectrum viewer |
| #81 | [GUI] ROI system for spectrum analysis | Multi-ROI + curves |
| #24 | [GUI] Pipeline architecture: Hits → Neutrons | Clustering + extraction |
| #82 | [GUI] Dead/hot pixel detection and masking | Pixel health + masks |
| #21 | [GUI] Detector configuration system | Profiles + advanced config |
| #22 | [GUI] Statistics panel for data inspection | Pre/post stats |
| #26 | [GUI] HDF5 output with compression | Export + metadata |
| #25 | [GUI] Pipeline architecture: Full streaming (Raw → Neutrons) | End-to-end bounded memory |
| #13 | [ENHANCEMENT] Improve GUI for neutron imaging and detector development | Legacy; superseded by #20 |

### Related (non-GUI)
- #49 [EPIC][IO] HDF5/NeXus IO (scipp-compatible events + histograms)
- #14 [ENHANCEMENT] Add TPX4/SPIDR4 data format support

---

## References

- NeXus format: https://www.nexusformat.org/
- egui: https://github.com/emilk/egui
- rustpix HDF5 schema: docs/hdf5_schema.md
