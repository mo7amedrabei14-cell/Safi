//! Desktop Installers, Bundling Manifests, and Portable Mode configuration (Section 10.2, 10.3).

use std::path::{Path, PathBuf};

/// Installer and bundle targets supported by SafiData desktop shell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BundleTarget {
    Msi,
    Msix,
    Portable,
    AppImage,
    Deb,
    Rpm,
    Flatpak,
}

impl BundleTarget {
    pub const ALL: [Self; 7] = [
        Self::Msi,
        Self::Msix,
        Self::Portable,
        Self::AppImage,
        Self::Deb,
        Self::Rpm,
        Self::Flatpak,
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Msi => "msi",
            Self::Msix => "msix",
            Self::Portable => "portable",
            Self::AppImage => "appimage",
            Self::Deb => "deb",
            Self::Rpm => "rpm",
            Self::Flatpak => "flatpak",
        }
    }

    #[must_use]
    pub const fn platform(self) -> &'static str {
        match self {
            Self::Msi | Self::Msix | Self::Portable => "windows",
            Self::AppImage | Self::Deb | Self::Rpm | Self::Flatpak => "linux",
        }
    }
}

/// Portable Mode configuration manager for standalone deployments.
#[derive(Debug, Clone)]
pub struct PortableManager {
    app_dir: PathBuf,
}

impl PortableManager {
    pub fn new<P: AsRef<Path>>(app_dir: P) -> Self {
        Self {
            app_dir: app_dir.as_ref().to_path_buf(),
        }
    }

    /// Checks if portable mode is active (indicated by a local `.portable` marker file next to the executable).
    pub fn is_portable_mode(&self) -> bool {
        let marker = self.app_dir.join(".portable");
        marker.exists()
    }

    /// Returns the local data directory, which is self-contained inside app_dir if portable mode is active.
    pub fn resolve_local_data_dir(&self) -> PathBuf {
        if self.is_portable_mode() {
            self.app_dir.join("data")
        } else {
            // Standard system local app data path fallback
            self.app_dir.clone()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_bundle_targets_completeness() {
        assert_eq!(BundleTarget::ALL.len(), 7);
        for target in BundleTarget::ALL {
            assert!(!target.name().is_empty());
            assert!(!target.platform().is_empty());
        }
    }

    #[test]
    fn test_portable_mode_detection() {
        let dir = tempdir().unwrap();
        let manager = PortableManager::new(dir.path());

        // Initially not portable
        assert!(!manager.is_portable_mode());
        assert_eq!(manager.resolve_local_data_dir(), dir.path());

        // Create .portable marker
        let marker = dir.path().join(".portable");
        std::fs::write(&marker, b"portable=true").unwrap();

        assert!(manager.is_portable_mode());
        assert_eq!(manager.resolve_local_data_dir(), dir.path().join("data"));
    }
}
