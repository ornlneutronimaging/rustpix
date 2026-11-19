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
/// A `private` file is created readable by its owner only.
fn reserve_temp_sibling(path: &Path, private: bool) -> io::Result<PathBuf> {
    for _ in 0..64 {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.subsec_nanos());
        let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let temp = path.with_file_name(format!(
            ".rustpix-{}-{sequence}-{nanos:08x}.partial",
            std::process::id()
        ));
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        if private {
            owner_only(&mut options);
        }
        match options.open(&temp) {
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

#[cfg(unix)]
fn owner_only(options: &mut fs::OpenOptions) {
    use std::os::unix::fs::OpenOptionsExt;
    options.mode(0o600);
}

#[cfg(not(unix))]
fn owner_only(_: &mut fs::OpenOptions) {}

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
    let replacing_file = fs::symlink_metadata(&path).is_ok_and(|m| m.is_file());
    // The replaced file's mode is applied only at rename, so the data stays private until then.
    let temp = reserve_temp_sibling(&path, replacing_file)?;
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

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn mode(path: &Path) -> u32 {
        fs::metadata(path).unwrap().permissions().mode() & 0o777
    }

    #[test]
    fn replacement_is_owner_only_until_renamed() {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("out.h5");
        fs::write(&dest, b"old").unwrap();
        fs::set_permissions(&dest, fs::Permissions::from_mode(0o440)).unwrap();

        write_then_replace(&dest, |temp| {
            assert_eq!(mode(temp), 0o600);
            fs::write(temp, b"new")?;
            Ok(())
        })
        .unwrap();

        assert_eq!(mode(&dest), 0o440);
        assert_eq!(fs::read(&dest).unwrap(), b"new");
    }

    #[test]
    fn new_destination_keeps_the_default_mode() {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("out.h5");
        let reference = dir.path().join("reference");
        fs::write(&reference, b"").unwrap();

        write_then_replace(&dest, |temp| {
            fs::write(temp, b"new")?;
            Ok(())
        })
        .unwrap();

        assert_eq!(mode(&dest), mode(&reference));
    }
}
