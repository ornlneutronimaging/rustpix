#!/usr/bin/env python3
"""Version management script for rustpix.

Single source of truth: Cargo.toml [workspace.package].version

All workspace crates inherit from this via `version.workspace = true`.
This script syncs the version to pyproject.toml for the Python wheel.

Usage:
    python scripts/version.py show     # Display current version
    python scripts/version.py check    # Verify all versions are in sync
    python scripts/version.py sync     # Sync version to pyproject.toml
    python scripts/version.py patch    # Bump patch: 0.1.0 -> 0.1.1
    python scripts/version.py minor    # Bump minor: 0.1.0 -> 0.2.0
    python scripts/version.py major    # Bump major: 0.1.0 -> 1.0.0
"""

import re
import subprocess
import sys
from pathlib import Path

# Repository root (parent of scripts/)
REPO_ROOT = Path(__file__).parent.parent

# Single source of truth
CARGO_WORKSPACE = REPO_ROOT / "Cargo.toml"

# Files to sync
PYPROJECT = REPO_ROOT / "pyproject.toml"

# All crate Cargo.toml files (for verification)
CRATE_CARGO_FILES = [
    REPO_ROOT / "rustpix-core" / "Cargo.toml",
    REPO_ROOT / "rustpix-tpx" / "Cargo.toml",
    REPO_ROOT / "rustpix-algorithms" / "Cargo.toml",
    REPO_ROOT / "rustpix-io" / "Cargo.toml",
    REPO_ROOT / "rustpix-python" / "Cargo.toml",
    REPO_ROOT / "rustpix-cli" / "Cargo.toml",
    REPO_ROOT / "rustpix-gui" / "Cargo.toml",
    REPO_ROOT / "tools" / "Cargo.toml",
]


def read_workspace_version() -> str:
    """Read version from workspace Cargo.toml."""
    content = CARGO_WORKSPACE.read_text()
    match = re.search(r'\[workspace\.package\]\s*\n\s*version\s*=\s*"([^"]+)"', content)
    if not match:
        raise ValueError(f"Could not find [workspace.package] version in {CARGO_WORKSPACE}")
    return match.group(1)


def read_pyproject_version() -> str:
    """Read version from pyproject.toml."""
    content = PYPROJECT.read_text()
    match = re.search(r'^version\s*=\s*"([^"]+)"', content, re.MULTILINE)
    if not match:
        raise ValueError(f"Could not find version in {PYPROJECT}")
    return match.group(1)


def write_workspace_version(version: str) -> None:
    """Write version to workspace Cargo.toml (both package and dependencies)."""
    content = CARGO_WORKSPACE.read_text()

    # Update [workspace.package].version
    new_content = re.sub(
        r'(\[workspace\.package\]\s*\n\s*version\s*=\s*)"[^"]+"',
        f'\\1"{version}"',
        content,
    )

    # Update workspace.dependencies versions for internal crates (for crates.io)
    internal_crates = ["rustpix-core", "rustpix-tpx", "rustpix-algorithms", "rustpix-io"]
    for crate in internal_crates:
        new_content = re.sub(
            rf'({crate}\s*=\s*\{{\s*version\s*=\s*)"[^"]+"',
            f'\\1"{version}"',
            new_content,
        )

    CARGO_WORKSPACE.write_text(new_content)
    print(f"  Updated {CARGO_WORKSPACE.relative_to(REPO_ROOT)}")


def sync_pyproject(version: str) -> None:
    """Sync version to pyproject.toml."""
    content = PYPROJECT.read_text()
    new_content = re.sub(
        r'^(version\s*=\s*)"[^"]+"',
        f'\\1"{version}"',
        content,
        flags=re.MULTILINE,
    )
    PYPROJECT.write_text(new_content)
    print(f"  Updated {PYPROJECT.relative_to(REPO_ROOT)}")


def check_crate_uses_workspace_version(cargo_path: Path) -> bool:
    """Check if a crate Cargo.toml uses workspace version inheritance."""
    content = cargo_path.read_text()
    # Look for version.workspace = true or version = { workspace = true }
    return bool(re.search(r'version\.workspace\s*=\s*true', content) or
                re.search(r'version\s*=\s*\{\s*workspace\s*=\s*true', content))


def bump_version(version: str, component: str) -> str:
    """Bump a version component (major, minor, or patch)."""
    parts = version.split(".")
    if len(parts) != 3:
        raise ValueError(f"Invalid version format: {version} (expected X.Y.Z)")

    major, minor, patch = map(int, parts)

    if component == "major":
        return f"{major + 1}.0.0"
    elif component == "minor":
        return f"{major}.{minor + 1}.0"
    elif component == "patch":
        return f"{major}.{minor}.{patch + 1}"
    else:
        raise ValueError(f"Unknown component: {component}")


def cmd_show() -> None:
    """Show current version."""
    cargo_version = read_workspace_version()
    pyproject_version = read_pyproject_version()

    print(f"Cargo workspace version: {cargo_version}")
    print(f"pyproject.toml version:  {pyproject_version}")

    if cargo_version != pyproject_version:
        print("\n⚠️  Versions are out of sync! Run: pixi run version-sync")


def cmd_check() -> int:
    """Verify all versions are in sync. Returns 0 if OK, 1 if issues found."""
    cargo_version = read_workspace_version()
    pyproject_version = read_pyproject_version()
    issues = []

    print(f"Checking versions (source of truth: {cargo_version})...")
    print()

    # Check pyproject.toml
    if cargo_version == pyproject_version:
        print(f"  ✓ pyproject.toml: {pyproject_version}")
    else:
        print(f"  ✗ pyproject.toml: {pyproject_version} (expected {cargo_version})")
        issues.append("pyproject.toml version mismatch")

    # Check all crate Cargo.toml files use workspace inheritance
    print()
    print("Checking workspace version inheritance...")
    for cargo_path in CRATE_CARGO_FILES:
        if not cargo_path.exists():
            print(f"  ? {cargo_path.relative_to(REPO_ROOT)}: file not found")
            continue

        if check_crate_uses_workspace_version(cargo_path):
            print(f"  ✓ {cargo_path.relative_to(REPO_ROOT)}")
        else:
            print(f"  ✗ {cargo_path.relative_to(REPO_ROOT)}: not using version.workspace = true")
            issues.append(f"{cargo_path.name} not using workspace version")

    # Verify Cargo workspace resolves correctly
    print()
    print("Verifying Cargo workspace...")
    try:
        result = subprocess.run(
            ["cargo", "metadata", "--format-version=1", "--no-deps"],
            cwd=REPO_ROOT,
            capture_output=True,
            text=True,
            check=True,
        )
        print("  ✓ Cargo workspace resolves correctly")
    except subprocess.CalledProcessError as e:
        print(f"  ✗ Cargo workspace error: {e.stderr}")
        issues.append("Cargo workspace resolution failed")

    print()
    if issues:
        print(f"Found {len(issues)} issue(s):")
        for issue in issues:
            print(f"  - {issue}")
        print("\nRun 'pixi run version-sync' to fix version mismatches.")
        return 1
    else:
        print("All versions are in sync!")
        return 0


def cmd_sync() -> None:
    """Sync version from Cargo.toml to pyproject.toml."""
    version = read_workspace_version()
    print(f"Syncing version {version} to pyproject.toml...")
    sync_pyproject(version)
    print("Done!")


def cmd_bump(component: str) -> None:
    """Bump version and sync."""
    old_version = read_workspace_version()
    new_version = bump_version(old_version, component)

    print(f"Bumping {component} version: {old_version} -> {new_version}")
    print()

    # Update Cargo.toml first (single source of truth)
    print("Updating Cargo workspace...")
    write_workspace_version(new_version)

    # Sync to pyproject.toml
    print()
    print("Syncing to Python...")
    sync_pyproject(new_version)

    print()
    print(f"✓ Version bumped to {new_version}")
    print()
    print("Next steps:")
    print("  1. Review changes: git diff")
    print("  2. Stage files: git add Cargo.toml pyproject.toml")
    print(f'  3. Commit: git commit -m "chore: bump version to {new_version}"')
    print(f"  4. Tag (optional): git tag v{new_version}")


def main() -> int:
    if len(sys.argv) < 2:
        print(__doc__)
        return 1

    command = sys.argv[1].lower()

    if command == "show":
        cmd_show()
        return 0
    elif command == "check":
        return cmd_check()
    elif command == "sync":
        cmd_sync()
        return 0
    elif command in ("patch", "minor", "major"):
        cmd_bump(command)
        return 0
    else:
        print(f"Unknown command: {command}")
        print(__doc__)
        return 1


if __name__ == "__main__":
    sys.exit(main())
