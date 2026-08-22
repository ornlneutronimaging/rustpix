#!/usr/bin/env bash
# Launch the rustpix GUI application.
set -euo pipefail

REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BIN="$REPO_DIR/target/release/rustpix-gui"

# Build the release binary if it doesn't exist yet
if [[ ! -x "$BIN" ]]; then
    echo "rustpix-gui binary not found, building it first..."
    (cd "$REPO_DIR" && cargo build --release -p rustpix-gui)
fi

exec "$BIN" "$@"
