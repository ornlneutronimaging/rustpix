# Building From Source

## Prerequisites

- Git
- Rust 1.70+ (install via [rustup](https://rustup.rs/))
- Python 3.11+ (for Python bindings)
- [Pixi](https://prefix.dev/) (recommended) or Cargo

## Clone Repository

```bash
git clone https://github.com/ornlneutronimaging/rustpix
cd rustpix
```

## Using Pixi (Recommended)

Pixi manages all dependencies automatically:

```bash
# Install pixi
curl -fsSL https://pixi.sh/install.sh | bash

# Install dependencies and build
pixi install
pixi run build
```

### Available Tasks

```bash
pixi run test        # Run all tests
pixi run clippy      # Run linter
pixi run gui         # Launch GUI (release mode)
pixi run gui-debug   # Launch GUI (debug mode)
pixi run docs        # Build Rust documentation
```

## Using Cargo

Build without pixi:

```bash
# Build all crates
cargo build --release --workspace

# Run tests
cargo test --workspace

# Build CLI
cargo build --release -p rustpix-cli

# Build GUI
cargo build --release -p rustpix-gui
```

## Python Bindings

Build Python package with maturin:

```bash
# Install maturin
pip install maturin

# Build and install in development mode
cd rustpix-python
maturin develop --release
```

Or build a wheel:

```bash
maturin build --release -m rustpix-python/Cargo.toml
pip install target/wheels/rustpix-*.whl
```

## Development Setup

For development with hot reloading:

```bash
pixi install
pixi run gui-debug  # Faster builds, debug symbols
```

## Troubleshooting

### HDF5 Linking Errors

HDF5 is statically linked by default. If you encounter issues:

```bash
# Ensure HDF5 dev packages are installed
# Ubuntu/Debian:
sudo apt install libhdf5-dev

# macOS:
brew install hdf5
```

### Python Not Found

Ensure Python 3.11+ is available:

```bash
python3 --version
# or with pixi:
pixi run python --version
```
