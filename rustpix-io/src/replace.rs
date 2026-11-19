//! Whole-file replacement that leaves the destination intact on failure.

use crate::Result;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Creates a new directory with a fresh name beside `path`, accessible to its owner only.
///
/// Exclusive creation means an existing entry at that name is never reused, and
/// other users cannot add, replace or read entries inside it.
fn reserve_staging_dir(path: &Path) -> io::Result<PathBuf> {
    for _ in 0..64 {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.subsec_nanos());
        let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let dir = path.with_file_name(format!(
            ".rustpix-{}-{sequence}-{nanos:08x}.partial",
            std::process::id()
        ));
        let mut builder = fs::DirBuilder::new();
        owner_only(&mut builder);
        match builder.create(&dir) {
            Ok(()) => return Ok(dir),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "could not reserve a temporary output directory",
    ))
}

#[cfg(unix)]
fn owner_only(builder: &mut fs::DirBuilder) {
    use std::os::unix::fs::DirBuilderExt;
    builder.mode(0o700);
}

#[cfg(not(unix))]
fn owner_only(_: &mut fs::DirBuilder) {}

/// Runs `write` on a temporary file beside `path`, then renames it onto `path`.
///
/// The temporary file lives in an owner-only directory, so writers that open it
/// by name cannot be redirected. When `write` fails, `path` is untouched. An
/// existing regular file keeps its permissions; a symbolic link at `path` is
/// replaced, not followed.
pub(crate) fn write_then_replace<T>(
    path: &Path,
    write: impl FnOnce(&Path) -> Result<T>,
) -> Result<T> {
    let path = std::path::absolute(path)?;
    let dir = reserve_staging_dir(&path)?;
    let temp = dir.join("output");
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
    let _ = fs::remove_dir(&dir);
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
    fn staging_is_private_and_removed() {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("out.h5");
        fs::write(&dest, b"old").unwrap();
        fs::set_permissions(&dest, fs::Permissions::from_mode(0o440)).unwrap();

        write_then_replace(&dest, |temp| {
            let staging = temp.parent().unwrap();
            assert_eq!(staging.parent().unwrap(), dir.path());
            assert_eq!(mode(staging), 0o700);
            fs::write(temp, b"new")?;
            Ok(())
        })
        .unwrap();

        assert_eq!(mode(&dest), 0o440);
        assert_eq!(fs::read(&dest).unwrap(), b"new");
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn failed_write_leaves_destination_and_no_staging() {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("out.h5");
        fs::write(&dest, b"old").unwrap();

        let result: Result<()> = write_then_replace(&dest, |temp| {
            fs::write(temp, b"partial")?;
            Err(io::Error::other("write failed").into())
        });

        assert!(result.is_err());
        assert_eq!(fs::read(&dest).unwrap(), b"old");
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
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
