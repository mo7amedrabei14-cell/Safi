//! Atomic file writes and Watched Folder (F1) with path normalization and symlink prevention (Section 4.10, 18.4).

use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// Write content atomically: temp file -> fsync -> rename.
/// Never modifies the user's original file directly.
pub fn atomic_write<P: AsRef<Path>>(path: P, content: &[u8]) -> io::Result<()> {
    let path = path.as_ref();
    let parent = path.parent().ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "Invalid file parent directory")
    })?;

    fs::create_dir_all(parent)?;

    // Create a temporary file in the same directory to ensure atomic rename on the same filesystem
    let mut tmp_path = parent.to_path_buf();
    let file_name = path.file_name().ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "Invalid file name")
    })?;
    let mut tmp_filename = file_name.to_os_string();
    tmp_filename.push(".tmp");
    tmp_path.push(tmp_filename);

    {
        let mut tmp_file = File::create(&tmp_path)?;
        tmp_file.write_all(content)?;
        tmp_file.sync_all()?;
    }

    fs::rename(&tmp_path, path)?;
    Ok(())
}

/// Watched Folder (F1) configuration and verification.
/// Ensures strict path normalization, prevents directory traversal, and rejects symlinks.
#[derive(Debug, Clone)]
pub struct WatchedFolder {
    base_dir: PathBuf,
}

impl WatchedFolder {
    pub fn new<P: AsRef<Path>>(base_dir: P) -> io::Result<Self> {
        let canonical_base = fs::canonicalize(base_dir)?;
        if !canonical_base.is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::NotADirectory,
                "Watched folder path must be a valid directory",
            ));
        }
        Ok(Self {
            base_dir: canonical_base,
        })
    }

    /// Resolve and validate an incoming file path within the watched folder.
    /// Rejects directory traversal (../) and symbolic links.
    pub fn validate_watched_file_path<P: AsRef<Path>>(&self, incoming_path: P) -> io::Result<PathBuf> {
        let incoming_path = incoming_path.as_ref();

        // If relative, join with base_dir; if absolute, canonicalize directly
        let full_path = if incoming_path.is_absolute() {
            incoming_path.to_path_buf()
        } else {
            self.base_dir.join(incoming_path)
        };

        // Check symlink metadata on components or target if it exists
        if full_path.symlink_metadata().is_ok() {
            let meta = fs::symlink_metadata(&full_path)?;
            if meta.file_type().is_symlink() {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "SF-SEC-021: الروابط الرمزية (Symlinks) محظورة في المجلد المراقَب امنياً",
                ));
            }
        }

        // Canonicalize if file already exists, or canonicalize parent if file is new
        let canonical_target = if full_path.exists() {
            fs::canonicalize(&full_path)?
        } else {
            let parent = full_path.parent().ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidInput, "Invalid path parent")
            })?;
            let canonical_parent = fs::canonicalize(parent)?;
            let file_name = full_path.file_name().ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidInput, "Invalid file name")
            })?;
            let mut p = canonical_parent;
            p.push(file_name);
            p
        };

        // Enforce strict confinement within base_dir
        if !canonical_target.starts_with(&self.base_dir) {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "SF-SEC-022: محاولة خروج من المجلد المراقَب (Path Traversal محظور)",
            ));
        }

        Ok(canonical_target)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_atomic_write_success() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test_export.csv");
        let data = b"id,name,amount\n1,Ahmed,100.50\n";

        atomic_write(&file_path, data).unwrap();
        let read_back = fs::read(&file_path).unwrap();
        assert_eq!(read_back, data);
    }

    #[test]
    fn test_watched_folder_validation() {
        let dir = tempdir().unwrap();
        let watched = WatchedFolder::new(dir.path()).unwrap();

        // Valid file inside watched folder
        let valid_sub = dir.path().join("incoming_file.csv");
        let base = watched.base_dir.clone();
        let checked = watched.validate_watched_file_path(&valid_sub).unwrap();
        assert!(checked.starts_with(base));

        // Attempt traversal outside watched folder
        let traversal_path = dir.path().join("../outside_file.csv");
        let res = watched.validate_watched_file_path(&traversal_path);
        assert!(res.is_err());
    }
}
