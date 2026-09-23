//! Crash-resistant same-directory file replacement helpers.
//!
//! The temporary file is created beside the destination so the final rename stays
//! on the same filesystem. Unix replacement is atomic. On Windows the standard
//! library cannot replace an existing destination atomically; the fallback keeps a
//! rollback backup and restores it if promotion of the temporary file fails.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(1);

fn sibling_path(path: &Path, marker: &str) -> Result<PathBuf, String> {
    let parent = path
        .parent()
        .ok_or_else(|| String::from("destination has no parent directory"))?;
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("photoshow");
    let extension = path.extension().and_then(|e| e.to_str());
    let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let mut name = format!(
        ".{stem}.photoshow-{marker}-{}-{sequence}",
        std::process::id()
    );
    if let Some(extension) = extension {
        name.push('.');
        name.push_str(extension);
    }
    Ok(parent.join(name))
}

fn sync_file(path: &Path) -> Result<(), String> {
    std::fs::OpenOptions::new()
        .write(true)
        .open(path)
        .map_err(|e| format!("open for sync {}: {e}", path.display()))?
        .sync_all()
        .map_err(|e| format!("sync {}: {e}", path.display()))
}

#[cfg(unix)]
fn sync_parent(path: &Path) {
    if let Some(parent) = path.parent()
        && let Ok(dir) = std::fs::File::open(parent)
    {
        let _ = dir.sync_all();
    }
}

fn promote(temp: &Path, destination: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        std::fs::rename(temp, destination)
            .map_err(|e| format!("replace {}: {e}", destination.display()))?;
        sync_parent(destination);
        Ok(())
    }

    #[cfg(not(unix))]
    {
        if !destination.exists() {
            std::fs::rename(temp, destination)
                .map_err(|e| format!("promote {}: {e}", destination.display()))
        } else {
            let backup = sibling_path(destination, "backup")?;
            std::fs::rename(destination, &backup)
                .map_err(|e| format!("backup {}: {e}", destination.display()))?;

            match std::fs::rename(temp, destination) {
                Ok(()) => {
                    let _ = std::fs::remove_file(backup);
                    Ok(())
                }
                Err(error) => {
                    let _ = std::fs::rename(&backup, destination);
                    Err(format!("promote {}: {error}", destination.display()))
                }
            }
        }
    }
}

/// Writes a destination through a same-directory temporary file.
///
/// The callback must create the temporary file at the provided path and return
/// only after its complete contents have been written.
pub fn write_atomic(
    destination: &Path,
    writer: impl FnOnce(&Path) -> Result<(), String>,
) -> Result<(), String> {
    let temp = sibling_path(destination, "tmp")?;
    let result = (|| {
        writer(&temp)?;
        if let Ok(metadata) = std::fs::metadata(destination) {
            std::fs::set_permissions(&temp, metadata.permissions())
                .map_err(|e| format!("copy permissions: {e}"))?;
        }
        sync_file(&temp)?;
        promote(&temp, destination)
    })();

    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result
}

/// Atomically writes a byte slice through write_atomic.
pub fn write_bytes_atomic(destination: &Path, bytes: &[u8]) -> Result<(), String> {
    write_atomic(destination, |temp| {
        std::fs::write(temp, bytes).map_err(|e| format!("write {}: {e}", temp.display()))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn successful_write_replaces_destination() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("config.json");
        std::fs::write(&path, b"old").expect("seed");

        write_bytes_atomic(&path, b"new").expect("atomic write");

        assert_eq!(std::fs::read(&path).expect("read"), b"new");
    }

    #[test]
    fn failed_writer_preserves_destination() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("photo.jpg");
        std::fs::write(&path, b"original").expect("seed");

        let error = write_atomic(&path, |_temp| Err(String::from("injected failure")))
            .expect_err("writer must fail");

        assert!(error.contains("injected failure"));
        assert_eq!(std::fs::read(&path).expect("read"), b"original");
    }
}
