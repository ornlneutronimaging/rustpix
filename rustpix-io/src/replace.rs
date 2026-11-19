//! Whole-file replacement that leaves the destination intact on failure.

use crate::Result;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static SEQUENCE: AtomicU64 = AtomicU64::new(0);

fn temp_sibling(path: &Path) -> PathBuf {
    let name = path.file_name().map_or_else(
        || "output".to_string(),
        |n| n.to_string_lossy().into_owned(),
    );
    let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    path.with_file_name(format!(".{name}.{}-{sequence}.partial", std::process::id()))
}

/// Runs `write` on a temporary file beside `path`, then renames it onto `path`.
///
/// When `write` fails the temporary file is removed and `path` is untouched.
pub(crate) fn write_then_replace<T>(
    path: &Path,
    write: impl FnOnce(&Path) -> Result<T>,
) -> Result<T> {
    let temp = temp_sibling(path);
    let outcome = write(&temp).and_then(|value| {
        std::fs::rename(&temp, path)?;
        Ok(value)
    });
    if outcome.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    outcome
}
