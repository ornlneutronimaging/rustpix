# Python Installation

## pip

The recommended way to install rustpix for Python is via pip:

```bash
pip install rustpix
```

This installs pre-built wheels for:
- Linux (x86_64, glibc 2.28+)
- macOS (ARM64 and x86_64)
- Windows (x86_64)

## Verify Installation

```python
import rustpix
print(rustpix.__version__)
```

## Virtual Environment (Recommended)

We recommend using a virtual environment:

```bash
# Create environment
python -m venv .venv
source .venv/bin/activate  # Linux/macOS
# or: .venv\Scripts\activate  # Windows

# Install
pip install rustpix
```

## With Scientific Stack

For data analysis workflows, install alongside NumPy and other tools:

```bash
pip install rustpix numpy matplotlib h5py
```

## Jupyter Notebooks

Rustpix works well in Jupyter notebooks:

```bash
pip install rustpix jupyterlab
jupyter lab
```

## Troubleshooting

### Python Version

Rustpix requires Python 3.11 or later. Check your version:

```bash
python --version
```

### Wheels Not Available

If no wheel is available for your platform, pip will attempt to build from source, which requires Rust. See [From Source](source.md) for build instructions.
