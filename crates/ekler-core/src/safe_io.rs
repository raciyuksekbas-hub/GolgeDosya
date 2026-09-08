//! Shared no-clobber publication. Existing files (including aliases of sources)
//! are never overwritten. Staging stays on the destination filesystem.
use crate::{EklerError, Result};
use std::io::Write;
use std::path::{Path, PathBuf};

fn io(path: &Path, source: std::io::Error) -> EklerError {
    EklerError::IoError {
        path: path.into(),
        source,
    }
}

pub fn ensure_new_destination(destination: &Path, sources: &[PathBuf]) -> Result<()> {
    // symlink_metadata also catches dangling symlinks; no-clobber is deliberately
    // stronger than an identity comparison and covers hardlinks on every OS.
    match std::fs::symlink_metadata(destination) {
        Ok(_) => {
            return Err(EklerError::ValidationFailed(format!(
                "Hedef zaten mevcut; üzerine yazılmaz: {}",
                destination.display()
            )))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(io(destination, e)),
    }
    let parent = destination.parent().unwrap_or(Path::new("."));
    let canonical_parent = parent.canonicalize().map_err(|e| io(parent, e))?;
    let name = destination
        .file_name()
        .ok_or_else(|| EklerError::ValidationFailed("Geçersiz hedef".into()))?;
    let resolved = canonical_parent.join(name);
    for source in sources {
        let canonical_source = source.canonicalize().map_err(|e| io(source, e))?;
        if canonical_source == resolved {
            return Err(EklerError::ValidationFailed(
                "Kaynağın üzerine yazılamaz".into(),
            ));
        }
    }
    Ok(())
}

pub fn write_new_file<F>(destination: &Path, sources: &[PathBuf], write: F) -> Result<()>
where
    F: FnOnce(&mut std::fs::File) -> Result<()>,
{
    ensure_new_destination(destination, sources)?;
    let parent = destination.parent().unwrap_or(Path::new("."));
    let mut staged = tempfile::NamedTempFile::new_in(parent).map_err(|e| io(parent, e))?;
    write(staged.as_file_mut())?;
    staged
        .as_file_mut()
        .flush()
        .map_err(|e| io(destination, e))?;
    staged
        .as_file()
        .sync_all()
        .map_err(|e| io(destination, e))?;
    staged
        .persist_noclobber(destination)
        .map_err(|e| io(destination, e.error))?;
    Ok(())
}

pub fn write_new_bytes(destination: &Path, sources: &[PathBuf], bytes: &[u8]) -> Result<()> {
    write_new_file(destination, sources, |f| {
        f.write_all(bytes).map_err(|e| io(destination, e))
    })
}

/// A single no-replace directory rename publishes an entire package. No copy
/// fallback: a filesystem that cannot provide this contract returns an error.
pub fn publish_directory(staged: &Path, destination: &Path) -> Result<()> {
    ensure_new_destination(destination, &[])?;
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        use std::os::unix::ffi::OsStrExt;
        let from = std::ffi::CString::new(staged.as_os_str().as_bytes())
            .map_err(|_| EklerError::ValidationFailed("Geçersiz yol".into()))?;
        let to = std::ffi::CString::new(destination.as_os_str().as_bytes())
            .map_err(|_| EklerError::ValidationFailed("Geçersiz yol".into()))?;
        // SAFETY: both C strings are NUL-terminated and live throughout the call.
        #[cfg(target_os = "macos")]
        let status = unsafe {
            libc::renameatx_np(
                libc::AT_FDCWD,
                from.as_ptr(),
                libc::AT_FDCWD,
                to.as_ptr(),
                libc::RENAME_EXCL,
            )
        };
        #[cfg(target_os = "linux")]
        let status = unsafe {
            libc::renameat2(
                libc::AT_FDCWD,
                from.as_ptr(),
                libc::AT_FDCWD,
                to.as_ptr(),
                libc::RENAME_NOREPLACE,
            )
        };
        if status != 0 {
            return Err(io(destination, std::io::Error::last_os_error()));
        }
        Ok(())
    }
    #[cfg(target_os = "windows")]
    {
        std::fs::rename(staged, destination).map_err(|e| io(destination, e))
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        Err(EklerError::ValidationFailed(
            "Atomik paket yayını bu platformda desteklenmiyor".into(),
        ))
    }
}
