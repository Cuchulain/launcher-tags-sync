//! File-system safety: path identity checks and atomic writes.

use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

/// Upper bound for reading a Quick Search JSON backup into memory.
pub const MAX_QUICKSEARCH_BYTES: u64 = 256 * 1024 * 1024;

/// Best-effort canonical form of a path that may not exist yet.
fn canonical_target(p: &Path) -> PathBuf {
    if let Ok(c) = std::fs::canonicalize(p) {
        return c;
    }
    let parent = match p.parent() {
        Some(par) if !par.as_os_str().is_empty() => par,
        _ => Path::new("."),
    };
    match (std::fs::canonicalize(parent), p.file_name()) {
        (Ok(par), Some(name)) => par.join(name),
        _ => p.to_path_buf(),
    }
}

/// True when `a` and `b` refer to the same file (same path after resolving symlinks and
/// `.`/`..`, or the same inode on Unix, which also catches hard links and case aliases).
pub fn same_file(a: &Path, b: &Path) -> bool {
    if canonical_target(a) == canonical_target(b) {
        return true;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if let (Ok(ma), Ok(mb)) = (std::fs::metadata(a), std::fs::metadata(b)) {
            return ma.dev() == mb.dev() && ma.ino() == mb.ino();
        }
    }
    false
}

/// Refuse to write `path` if it is one of `inputs`, a directory, or exists without `force`.
pub fn check_writable(
    label: &str,
    path: &Path,
    inputs: &[(&str, &Path)],
    force: bool,
) -> Result<()> {
    for (name, input) in inputs {
        if same_file(path, input) {
            bail!(
                "{label} path {} is the same file as {name}; refusing to overwrite an input (even with --force)",
                path.display()
            );
        }
    }
    if path.is_dir() {
        bail!("{label} path {} is a directory", path.display());
    }
    if path.symlink_metadata().is_ok() && !force {
        bail!(
            "{label} file {} already exists; pass --force to replace it",
            path.display()
        );
    }
    Ok(())
}

pub fn read_limited(path: &Path, limit: u64, what: &str) -> Result<Vec<u8>> {
    let meta = std::fs::metadata(path)
        .with_context(|| format!("cannot read {what} {}", path.display()))?;
    if !meta.is_file() {
        bail!("{what} {} is not a regular file", path.display());
    }
    if meta.len() > limit {
        bail!("{what} {} is larger than {limit} bytes", path.display());
    }
    std::fs::read(path).with_context(|| format!("cannot read {what} {}", path.display()))
}

/// Write via a temporary file in the destination directory, fsync, then rename into place.
/// Without `overwrite` the final rename refuses to replace an existing file. The temporary
/// file is removed on any error. New files are created with owner-only permissions (0600).
pub fn write_atomic(path: &Path, bytes: &[u8], overwrite: bool) -> Result<()> {
    let parent = match path.parent() {
        Some(p) if !p.as_os_str().is_empty() => p,
        _ => Path::new("."),
    };
    let mut tmp = tempfile::Builder::new()
        .prefix(".launcher-tags-sync.")
        .suffix(".tmp")
        .tempfile_in(parent)
        .with_context(|| format!("cannot create temporary file in {}", parent.display()))?;
    tmp.write_all(bytes)?;
    tmp.flush()?;
    tmp.as_file().sync_all()?;
    let result = if overwrite {
        tmp.persist(path).map(drop)
    } else {
        tmp.persist_noclobber(path).map(drop)
    };
    result.map_err(|e| e.error).with_context(|| {
        if overwrite {
            format!("cannot move output into place at {}", path.display())
        } else {
            format!(
                "cannot create {} (it may already exist; use --force)",
                path.display()
            )
        }
    })?;
    #[cfg(unix)]
    if let Ok(dir) = std::fs::File::open(parent) {
        let _ = dir.sync_all();
    }
    Ok(())
}
