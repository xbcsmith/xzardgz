//! Path validation sandbox for sandboxed agent file tool access.
//!
//! Provides [`PathValidator`] which enforces read and write zone restrictions
//! before any filesystem operation is permitted.

use crate::error::{PipelineError, Result};
use std::path::{Component, Path, PathBuf};

/// Enforces read and write zone restrictions for agent file operations.
///
/// All paths are validated for traversal attacks, symlink escapes, and
/// zone membership before any filesystem operation is permitted.
///
/// Zone paths are canonicalized at construction time so that symlinks in
/// zone definitions are resolved consistently with the paths being validated.
///
/// # Examples
///
/// ```no_run
/// use std::path::PathBuf;
/// use std::sync::Arc;
/// use xzardgz::tools::sandbox::PathValidator;
///
/// let validator = Arc::new(PathValidator::new(
///     vec![PathBuf::from("/workspace/src")],
///     vec![PathBuf::from("/workspace/output")],
/// ));
/// ```
pub struct PathValidator {
    read_zones: Vec<PathBuf>,
    write_zones: Vec<PathBuf>,
}

impl PathValidator {
    /// Creates a new `PathValidator` with the given read and write zones.
    ///
    /// Zone paths that cannot be canonicalized (e.g., do not exist) are silently
    /// dropped. Callers should ensure all zone paths exist before constructing
    /// the validator.
    ///
    /// # Arguments
    ///
    /// * `read_zones` - Directories from which files may be read.
    /// * `write_zones` - Directories to which files may be written.
    pub fn new(read_zones: Vec<PathBuf>, write_zones: Vec<PathBuf>) -> Self {
        let read_zones = read_zones
            .into_iter()
            .filter_map(|z| std::fs::canonicalize(&z).ok())
            .collect();
        let write_zones = write_zones
            .into_iter()
            .filter_map(|z| std::fs::canonicalize(&z).ok())
            .collect();
        Self {
            read_zones,
            write_zones,
        }
    }

    /// Creates a `PathValidator` with read zones but no write zones.
    ///
    /// Any attempt to validate a write path will fail immediately with
    /// "write access denied: no write zones configured".
    ///
    /// # Arguments
    ///
    /// * `read_zones` - Directories from which files may be read.
    pub fn read_only(read_zones: Vec<PathBuf>) -> Self {
        Self::new(read_zones, Vec::new())
    }

    /// Validates a path for read access.
    ///
    /// Returns the canonicalized path if it exists and falls within a
    /// configured read zone.
    ///
    /// # Arguments
    ///
    /// * `path` - The path to validate.
    ///
    /// # Returns
    ///
    /// The canonical `PathBuf` on success.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::Tool`] if:
    /// - The path contains `..` components (traversal rejected)
    /// - The path cannot be canonicalized (e.g., file does not exist)
    /// - The canonical path is not within any configured read zone
    pub fn validate_read(&self, path: &Path) -> Result<PathBuf> {
        Self::reject_traversal(path)?;
        let canonical = Self::canonicalize_for_read(path)?;
        if Self::is_within_any_zone(&self.read_zones, &canonical) {
            Ok(canonical)
        } else {
            Err(PipelineError::Tool(format!(
                "path is outside read zones: {}",
                path.display()
            )))
        }
    }

    /// Validates a path for write access.
    ///
    /// Returns the canonicalized path if write access is permitted. The target
    /// file does not need to exist; only the parent directory must exist.
    ///
    /// # Arguments
    ///
    /// * `path` - The path to validate.
    ///
    /// # Returns
    ///
    /// The canonical `PathBuf` on success.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::Tool`] if:
    /// - The path contains `..` components (traversal rejected)
    /// - No write zones are configured
    /// - The parent directory cannot be canonicalized
    /// - The canonical path is not within any configured write zone
    pub fn validate_write(&self, path: &Path) -> Result<PathBuf> {
        Self::reject_traversal(path)?;
        if self.write_zones.is_empty() {
            return Err(PipelineError::Tool(
                "write access denied: no write zones configured".to_string(),
            ));
        }
        let canonical = Self::canonicalize_for_write(path)?;
        if Self::is_within_any_zone(&self.write_zones, &canonical) {
            Ok(canonical)
        } else {
            Err(PipelineError::Tool(format!(
                "path is outside write zones: {}",
                path.display()
            )))
        }
    }

    /// Returns the configured read zones.
    pub fn read_zones(&self) -> &[PathBuf] {
        &self.read_zones
    }

    /// Returns the configured write zones.
    pub fn write_zones(&self) -> &[PathBuf] {
        &self.write_zones
    }

    /// Rejects any path containing `..` (parent directory) components.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::Tool`] if `path` contains a [`Component::ParentDir`].
    fn reject_traversal(path: &Path) -> Result<()> {
        for component in path.components() {
            if component == Component::ParentDir {
                return Err(PipelineError::Tool(format!(
                    "path traversal rejected: {}",
                    path.display()
                )));
            }
        }
        Ok(())
    }

    /// Canonicalizes a path that must already exist on the filesystem.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::Tool`] if `std::fs::canonicalize` fails (e.g.,
    /// path does not exist).
    fn canonicalize_for_read(path: &Path) -> Result<PathBuf> {
        std::fs::canonicalize(path).map_err(|e| {
            PipelineError::Tool(format!(
                "cannot canonicalize path {}: {}",
                path.display(),
                e
            ))
        })
    }

    /// Canonicalizes a path that may not yet exist (for write operations).
    ///
    /// Canonicalizes the immediate parent directory and appends the filename.
    /// This handles the common case where the target file does not yet exist
    /// but its containing directory does.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::Tool`] if the parent directory cannot be
    /// canonicalized or if the path has no usable parent/filename.
    fn canonicalize_for_write(path: &Path) -> Result<PathBuf> {
        if let (Some(parent), Some(file_name)) = (path.parent(), path.file_name()) {
            let effective_parent = if parent == Path::new("") {
                Path::new(".")
            } else {
                parent
            };
            let canonical_parent = std::fs::canonicalize(effective_parent).map_err(|e| {
                PipelineError::Tool(format!(
                    "cannot canonicalize parent of {}: {}",
                    path.display(),
                    e
                ))
            })?;
            Ok(canonical_parent.join(file_name))
        } else {
            // Path with no filename (e.g., "/" or bare ".") - try direct canonicalization
            Self::canonicalize_for_read(path)
        }
    }

    /// Returns `true` if `canonical` starts with any of the given zones.
    fn is_within_any_zone(zones: &[PathBuf], canonical: &Path) -> bool {
        zones.iter().any(|zone| canonical.starts_with(zone))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_reject_traversal_rejects_dotdot_path() {
        let result = PathValidator::reject_traversal(Path::new("../secret"));
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("path traversal rejected"));
    }

    #[test]
    fn test_reject_traversal_accepts_normal_path() {
        let result = PathValidator::reject_traversal(Path::new("some/normal/path.txt"));
        assert!(result.is_ok());
    }

    #[test]
    fn test_read_only_has_empty_write_zones() {
        let dir = TempDir::new().unwrap();
        let validator = PathValidator::read_only(vec![dir.path().to_path_buf()]);
        assert!(validator.write_zones().is_empty());
        assert!(!validator.read_zones().is_empty());
    }

    #[test]
    fn test_validate_read_rejects_traversal() {
        let dir = TempDir::new().unwrap();
        let validator = PathValidator::new(vec![dir.path().to_path_buf()], vec![]);
        let result = validator.validate_read(Path::new("../etc/passwd"));
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("traversal"));
    }

    #[test]
    fn test_validate_write_rejects_when_no_write_zones() {
        let dir = TempDir::new().unwrap();
        let validator = PathValidator::read_only(vec![dir.path().to_path_buf()]);
        let target = dir.path().join("file.txt");
        let result = validator.validate_write(&target);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("no write zones configured")
        );
    }

    #[test]
    fn test_validate_write_rejects_path_outside_write_zone() {
        let zone = TempDir::new().unwrap();
        let outside = TempDir::new().unwrap();
        let validator = PathValidator::new(
            vec![zone.path().to_path_buf()],
            vec![zone.path().to_path_buf()],
        );
        let outside_target = outside.path().join("file.txt");
        let result = validator.validate_write(&outside_target);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("outside write zones")
        );
    }

    #[test]
    fn test_validate_write_accepts_path_within_write_zone() {
        let zone = TempDir::new().unwrap();
        let validator = PathValidator::new(
            vec![zone.path().to_path_buf()],
            vec![zone.path().to_path_buf()],
        );
        let target = zone.path().join("new_file.txt");
        let result = validator.validate_write(&target);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_read_rejects_path_outside_read_zone() {
        let zone = TempDir::new().unwrap();
        let outside = TempDir::new().unwrap();
        // Create a real file outside the zone so canonicalization can succeed
        let outside_file = outside.path().join("secret.txt");
        fs::write(&outside_file, "secret").unwrap();
        let validator = PathValidator::new(vec![zone.path().to_path_buf()], vec![]);
        let result = validator.validate_read(&outside_file);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("outside read zones")
        );
    }

    #[test]
    fn test_validate_read_accepts_path_within_read_zone() {
        let zone = TempDir::new().unwrap();
        let file = zone.path().join("readme.txt");
        fs::write(&file, "hello").unwrap();
        let validator = PathValidator::new(vec![zone.path().to_path_buf()], vec![]);
        let result = validator.validate_read(&file);
        assert!(result.is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn test_symlink_within_zone_is_accepted() {
        use std::os::unix::fs::symlink;
        let zone = TempDir::new().unwrap();
        let real_file = zone.path().join("real.txt");
        fs::write(&real_file, "data").unwrap();
        let link = zone.path().join("link.txt");
        symlink(&real_file, &link).unwrap();
        let validator = PathValidator::new(vec![zone.path().to_path_buf()], vec![]);
        let result = validator.validate_read(&link);
        assert!(
            result.is_ok(),
            "symlink within zone should be accepted: {:?}",
            result
        );
    }

    #[cfg(unix)]
    #[test]
    fn test_symlink_escaping_zone_is_rejected() {
        use std::os::unix::fs::symlink;
        let zone = TempDir::new().unwrap();
        let outside = TempDir::new().unwrap();
        let outside_file = outside.path().join("secret.txt");
        fs::write(&outside_file, "secret").unwrap();
        // Symlink inside zone pointing to file outside zone
        let link = zone.path().join("escape.txt");
        symlink(&outside_file, &link).unwrap();
        let validator = PathValidator::new(vec![zone.path().to_path_buf()], vec![]);
        let result = validator.validate_read(&link);
        assert!(result.is_err(), "symlink escaping zone should be rejected");
    }

    #[test]
    fn test_absolute_path_outside_zones_is_rejected() {
        let zone = TempDir::new().unwrap();
        let outside = TempDir::new().unwrap();
        let outside_file = outside.path().join("data.txt");
        fs::write(&outside_file, "data").unwrap();
        let validator = PathValidator::new(vec![zone.path().to_path_buf()], vec![]);
        let result = validator.validate_read(&outside_file);
        assert!(result.is_err());
    }
}
