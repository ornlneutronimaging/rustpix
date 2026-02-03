# Homebrew Tap Setup

This directory contains the template for the `homebrew-rustpix` Homebrew tap.

## Setup Instructions

### 1. Create the Tap Repository

Create a new repository at: `https://github.com/ornlneutronimaging/homebrew-rustpix`

With this structure:
```
homebrew-rustpix/
├── Casks/
│   └── rustpix.rb
└── README.md
```

### 2. Copy the Cask Formula

Copy `rustpix.rb` from this directory to `Casks/rustpix.rb` in the tap repository.

### 3. Configure GitHub Secrets

In the **rustpix** repository (not the tap), add the following secret:

- **`HOMEBREW_TAP_TOKEN`**: A GitHub Personal Access Token with `repo` scope for the `homebrew-rustpix` repository.

To create the token:
1. Go to GitHub Settings → Developer settings → Personal access tokens → Fine-grained tokens
2. Create a new token with:
   - Repository access: `ornlneutronimaging/homebrew-rustpix`
   - Permissions: Contents (Read and write)

### 4. Users Can Install Via

```bash
brew tap ornlneutronimaging/rustpix
brew install --cask rustpix
```

## How It Works

1. When a new version tag (e.g., `v1.0.0`) is pushed, the release workflow runs
2. After the GitHub Release is created with the DMG, the `update-homebrew` job:
   - Downloads the DMG and calculates its SHA256
   - Clones the `homebrew-rustpix` tap repository
   - Updates the version and SHA256 in `Casks/rustpix.rb`
   - Commits and pushes the changes

## Manual Update

If needed, manually update the cask:

```bash
# Calculate SHA256 of the DMG
shasum -a 256 rustpix-X.Y.Z-macos-arm64.dmg

# Update Casks/rustpix.rb with new version and SHA256
```
