//! Whole-file replacement that leaves the destination intact on failure.

use crate::Result;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Creates a new, empty file with a fresh name beside `path`.
///
/// Exclusive creation means an existing file or link at that name is never reused.
fn reserve_temp_sibling(path: &Path) -> io::Result<PathBuf> {
    for _ in 0..64 {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.subsec_nanos());
        let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let temp = path.with_file_name(format!(
            ".rustpix-{}-{sequence}-{nanos:08x}.partial",
            std::process::id()
        ));
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
        {
            Ok(_) => return Ok(temp),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "could not reserve a temporary output file",
    ))
}

/// Runs `write` on a temporary file beside `path`, then renames it onto `path`.
///
/// When `write` fails the temporary file is removed and `path` is untouched. An
/// existing regular file keeps its permissions; a symbolic link at `path` is
/// replaced, not followed.
pub(crate) fn write_then_replace<T>(
    path: &Path,
    write: impl FnOnce(&Path) -> Result<T>,
) -> Result<T> {
    let path = std::path::absolute(path)?;
    let temp = reserve_temp_sibling(&path)?;
    let outcome = write(&temp).and_then(|value| {
        if let Ok(existing) = fs::symlink_metadata(&path) {
            if existing.is_file() {
                fs::set_permissions(&temp, existing.permissions())?;
            }
        }
        fs::rename(&temp, &path)?;
        Ok(value)
    });
    if outcome.is_err() {
        let _ = fs::remove_file(&temp);
    }
    outcome
}
