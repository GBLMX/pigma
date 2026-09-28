//! Replacing a file on disk without ever leaving a half-written one behind.

use std::{
    io::{self, Write},
    path::Path,
    sync::atomic::{AtomicU32, Ordering},
};

/// Distinguishes temp files made by this process; the pid already separates processes.
static TEMP_SEQUENCE: AtomicU32 = AtomicU32::new(0);

/// Write `bytes` to `path` atomically.
///
/// A plain `fs::write` truncates `path` in place, so a reader (another instance, or the next
/// start after a crash) between the truncate and the last byte sees an empty or half-written
/// file — which is how a config or a cache index is silently lost. Here the bytes go to a
/// uniquely named temp file in the same directory, are flushed to disk, and the temp file is
/// then renamed over `path`: `rename` replaces the destination in a single step. The temp file
/// lives beside `path` on purpose — a rename across filesystems is not atomic and fails
/// outright.
///
/// The directory is not `fsync`ed: the file's own data being on disk before the rename is what
/// keeps it from coming back truncated.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let dir = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(dir)?;

    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("file");
    let tmp = dir.join(format!(
        ".{name}.{}.{}.tmp",
        std::process::id(),
        TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));

    let result = write_and_rename(&tmp, path, bytes);
    if result.is_err() {
        // Best effort: a leftover temp file is harmless, but leaving one after a failure
        // that may repeat is not.
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

fn write_and_rename(tmp: &Path, path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(tmp)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    std::fs::rename(tmp, path)
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf};

    use super::*;

    fn scratch(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "boxpigma-fs-test-{}-{label}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn write_atomic_replaces_the_whole_file() {
        let path = scratch("replace").join("config.toml");
        fs::write(&path, "old contents that are longer").unwrap();

        write_atomic(&path, b"new").unwrap();

        assert_eq!(fs::read_to_string(&path).unwrap(), "new");
    }

    /// The point of the temp file: no half-written file under `path` is ever observable, and
    /// nothing is left lying next to it.
    #[test]
    fn write_atomic_leaves_no_temp_file_behind() {
        let dir = scratch("no-temp");
        let path = dir.join("index.json");
        write_atomic(&path, b"{}").unwrap();

        let leftovers: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .filter_map(Result::ok)
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|name| name != "index.json")
            .collect();
        assert!(leftovers.is_empty(), "temp files left: {leftovers:?}");
    }
}
