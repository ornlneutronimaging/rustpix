# macOS Installation (Homebrew)

## GUI Application

Install the rustpix GUI application via Homebrew:

```bash
# Add the tap
brew tap ornlneutronimaging/rustpix

# Install the GUI app
brew install --cask rustpix
```

## Launch

After installation, launch from:
- **Spotlight**: Search for "Rustpix"
- **Applications**: Find Rustpix in `/Applications`
- **Terminal**: `open -a Rustpix`

## Requirements

- macOS Big Sur (11.0) or later
- Apple Silicon (ARM64) architecture

> **Note**: Intel Mac support is available via the [CLI tool](rust.md) or [Python package](python.md).

## Updating

```bash
brew upgrade --cask rustpix
```

## Uninstalling

```bash
brew uninstall --cask rustpix
```

## Gatekeeper Notice

On first launch, macOS may show a security warning. The Homebrew installation automatically handles the quarantine attribute, but if you see a warning:

1. Go to **System Preferences > Security & Privacy**
2. Click **Open Anyway** for Rustpix
