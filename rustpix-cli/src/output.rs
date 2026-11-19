//! Safe placement of output files.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// A file written beside its destination and renamed onto it by [`commit`](Self::commit).
///
/// Dropping it uncommitted removes the partial file and leaves the destination untouched.
pub(crate) struct PendingOutput {
    temp: PathBuf,
    dest: PathBuf,
    committed: bool,
}

impl PendingOutput {
    pub(crate) fn new(dest: &Path) -> Self {
        let name = dest.file_name().map_or_else(
            || "output".to_string(),
            |n| n.to_string_lossy().into_owned(),
        );
        let temp = dest.with_file_name(format!(".{name}.{}.partial", std::process::id()));
        Self {
            temp,
            dest: dest.to_path_buf(),
            committed: false,
        }
    }

    /// Path to write to until the output is complete.
    pub(crate) fn path(&self) -> &Path {
        &self.temp
    }

    /// Moves the finished file onto its destination, replacing any existing file.
    pub(crate) fn commit(mut self) -> io::Result<()> {
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
