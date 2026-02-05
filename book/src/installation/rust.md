# Rust Installation (cargo)

## CLI Tool

Install the command-line interface via cargo:

```bash
cargo install rustpix-cli
```

This installs the `rustpix` binary to `~/.cargo/bin/`.

## Verify Installation

```bash
rustpix --version
rustpix --help
```

## Library Usage

Add rustpix crates to your Rust project:

```bash
# Core types and traits
cargo add rustpix-core

# Clustering algorithms
cargo add rustpix-algorithms

# TPX3 parsing
cargo add rustpix-tpx

# File I/O
cargo add rustpix-io
```

## Example Cargo.toml

```toml
[dependencies]
rustpix-core = "1.0"
rustpix-algorithms = "1.0"
rustpix-tpx = "1.0"
rustpix-io = "1.0"
```

## API Documentation

Rust API documentation is available on docs.rs:

- [rustpix-core](https://docs.rs/rustpix-core)
- [rustpix-algorithms](https://docs.rs/rustpix-algorithms)
- [rustpix-tpx](https://docs.rs/rustpix-tpx)
- [rustpix-io](https://docs.rs/rustpix-io)

## Requirements

- Rust 1.70 or later
- For HDF5 support: HDF5 libraries (automatically handled via static linking)
