//! Safe placement of output files.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// A file written beside its destination and renamed onto it by [`commit`](Self::commit).
///
/// The staging file is created exclusively, so an existing file or link at that
/// path is never written through or removed. Dropping it uncommitted removes it
/// and leaves the destination untouched.
pub(crate) struct PendingOutput {
    temp: PathBuf,
    dest: PathBuf,
    committed: bool,
}

impl PendingOutput {
    pub(crate) fn new(dest: &Path) -> io::Result<Self> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let dest = std::path::absolute(dest)?;
        let existing = fs::symlink_metadata(&dest)
            .ok()
            .filter(fs::Metadata::is_file)
            .map(|m| m.permissions());
        for _ in 0..64 {
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |d| d.subsec_nanos());
            let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let temp = dest.with_file_name(format!(
                ".rustpix-{}-{sequence}-{nanos:08x}.partial",
                std::process::id()
            ));
            match fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temp)
            {
                Ok(file) => {
                    let pending = Self {
                        temp,
                        dest,
                        committed: false,
                    };
                    if let Some(permissions) = &existing {
                        restrict_staging(&file, permissions)?;
                    }
                    return Ok(pending);
                }
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
                Err(e) => return Err(e),
            }
        }
        Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "could not reserve a temporary output file",
        ))
    }

    /// Path to write to until the output is complete.
    pub(crate) fn path(&self) -> &Path {
        &self.temp
    }

    /// Moves the finished file onto its destination, keeping an existing file's permissions.
    ///
    /// A symbolic link at the destination is replaced by the file, not followed.
    pub(crate) fn commit(mut self) -> io::Result<()> {
        if let Ok(existing) = fs::symlink_metadata(&self.dest) {
            if existing.is_file() {
                fs::set_permissions(&self.temp, existing.permissions())?;
            }
        }
        fs::rename(&self.temp, &self.dest)?;
        self.committed = true;
        Ok(())
    }
}

impl Drop for PendingOutput {
    fn drop(&mut self) {
        if !self.committed {
            let _ = fs::remove_file(&self.temp);
        }
    }
}

/// Gives the staging file the replaced file's access bits before any data is written.
#[cfg(unix)]
fn restrict_staging(file: &fs::File, existing: &fs::Permissions) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    // Owner write stays set so writers can reopen the path; commit applies the exact mode.
    file.set_permissions(fs::Permissions::from_mode(
        (existing.mode() & 0o777) | 0o200,
    ))
}

#[cfg(not(unix))]
fn restrict_staging(_: &fs::File, _: &fs::Permissions) -> io::Result<()> {
    Ok(())
}

/// True when both paths name the same existing file, including through links.
pub(crate) fn same_file(a: &Path, b: &Path) -> bool {
    let (Ok(a), Ok(b)) = (fs::canonicalize(a), fs::canonicalize(b)) else {
        return false;
    };
    a == b || same_inode(&a, &b)
}

#[cfg(unix)]
fn same_inode(a: &Path, b: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    match (fs::metadata(a), fs::metadata(b)) {
        (Ok(a), Ok(b)) => a.dev() == b.dev() && a.ino() == b.ino(),
        _ => false,
    }
}

#[cfg(not(unix))]
fn same_inode(_: &Path, _: &Path) -> bool {
    false
}

/// Case-insensitive extension test.
pub(crate) fn has_extension(path: &Path, extension: &str) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case(extension))
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn mode(path: &Path) -> u32 {
        fs::metadata(path).unwrap().permissions().mode() & 0o777
    }

    #[test]
    fn staging_file_is_private_before_data_is_written() {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("out.csv");
        fs::write(&dest, b"old").unwrap();
        fs::set_permissions(&dest, fs::Permissions::from_mode(0o600)).unwrap();

        let pending = PendingOutput::new(&dest).unwrap();
        assert_eq!(mode(pending.path()), 0o600);

        fs::write(pending.path(), b"new").unwrap();
        pending.commit().unwrap();
        assert_eq!(mode(&dest), 0o600);
        assert_eq!(fs::read(&dest).unwrap(), b"new");
    }

    #[test]
    fn read_only_destination_stays_writable_until_commit() {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("out.csv");
        fs::write(&dest, b"old").unwrap();
        fs::set_permissions(&dest, fs::Permissions::from_mode(0o440)).unwrap();

        let pending = PendingOutput::new(&dest).unwrap();
        assert_eq!(mode(pending.path()), 0o640);
        fs::write(pending.path(), b"new").unwrap();
        pending.commit().unwrap();
        assert_eq!(mode(&dest), 0o440);
    }
}
