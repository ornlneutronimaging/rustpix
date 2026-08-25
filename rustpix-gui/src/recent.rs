//! The last few TPX3 files opened, persisted across restarts.
//!
//! The list lives in `~/.config/venus_rust_tools/rustpix_recent`, one
//! absolute path per line, most recent first. Persistence is best effort:
//! an unwritable home directory only loses the list, it never errors.

use std::path::{Path, PathBuf};

pub const MAX: usize = 5;

/// The list file, under `$XDG_CONFIG_HOME` (or `~/.config`).
fn list_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;
    Some(base.join("venus_rust_tools").join("rustpix_recent"))
}

pub fn load() -> Vec<PathBuf> {
    let Some(text) = list_path().and_then(|p| std::fs::read_to_string(p).ok()) else {
        return Vec::new();
    };
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(PathBuf::from)
        .take(MAX)
        .collect()
}

/// Move `path` to the front of the list (deduplicated, capped at [`MAX`])
/// and persist it. Canonicalized so the same file reached through different
/// mounts (/SNS vs /gpfs) or relative paths only appears once.
pub fn add(list: &mut Vec<PathBuf>, path: &Path) {
    let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    list.retain(|p| p != &path);
    list.insert(0, path);
    list.truncate(MAX);
    save(list);
}

pub fn clear(list: &mut Vec<PathBuf>) {
    list.clear();
    save(list);
}

fn save(list: &[PathBuf]) {
    let Some(path) = list_path() else { return };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let mut text = String::new();
    for p in list {
        text.push_str(&p.to_string_lossy());
        text.push('\n');
    }
    let _ = std::fs::write(path, text);
}
