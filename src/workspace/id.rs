//! Workspace identity and timestamp utilities.
//!
//! This module provides helpers for generating workspace IDs, hashing repository
//! targets, and obtaining the current UTC time in various formats.

use sha2::{Digest, Sha256};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Schema version for the workspace state file.
///
/// Increment this value when making backward-incompatible changes to the
/// on-disk state file format so that readers can detect stale files.
pub const WORKSPACE_STATE_VERSION: &str = "1";

// ---------------------------------------------------------------------------
// ID generation
// ---------------------------------------------------------------------------

/// Generates a new workspace ID as a ULID string (26 upper-case characters).
///
/// ULIDs are monotonically increasing and lexicographically sortable, making
/// them suitable for workspace directories where chronological ordering matters.
///
/// # Examples
///
/// ```
/// use xzardgz::workspace::id::new_workspace_id;
/// let id = new_workspace_id();
/// assert_eq!(id.len(), 26);
/// ```
pub fn new_workspace_id() -> String {
    ulid::Ulid::new().to_string()
}

// ---------------------------------------------------------------------------
// Repository hashing
// ---------------------------------------------------------------------------

/// Computes a deterministic 64-character lower-hex SHA-256 digest of the
/// repository URL or local path string.
///
/// The hash uniquely identifies a repository target within the workspace root,
/// enabling `WorkspaceManager::open` to locate existing workspaces for the
/// same repository without scanning all state files.
///
/// # Arguments
///
/// * `repo` - Repository URL or local path string.
///
/// # Examples
///
/// ```
/// use xzardgz::workspace::id::hash_repository;
/// let h1 = hash_repository("https://github.com/example/repo");
/// let h2 = hash_repository("https://github.com/example/repo");
/// assert_eq!(h1, h2, "hashes must be deterministic");
/// assert_eq!(h1.len(), 64);
/// ```
pub fn hash_repository(repo: &str) -> String {
    let bytes = Sha256::digest(repo.as_bytes());
    bytes
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect::<String>()
}

// ---------------------------------------------------------------------------
// Timestamp helpers
// ---------------------------------------------------------------------------

/// Returns the current UTC time as a `chrono::DateTime<chrono::Utc>`.
///
/// # Examples
///
/// ```
/// use xzardgz::workspace::id::now_utc;
/// let t = now_utc();
/// assert!(t.timestamp() > 0);
/// ```
pub fn now_utc() -> chrono::DateTime<chrono::Utc> {
    chrono::Utc::now()
}

/// Returns the current UTC time formatted as an RFC 3339 string.
///
/// # Examples
///
/// ```
/// use xzardgz::workspace::id::now_rfc3339;
/// let s = now_rfc3339();
/// // RFC 3339 strings contain 'T' and '+' or 'Z'
/// assert!(s.contains('T'));
/// ```
pub fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_workspace_id_returns_26_char_string() {
        let id = new_workspace_id();
        assert_eq!(
            id.len(),
            26,
            "ULID must be exactly 26 characters, got: {}",
            id
        );
    }

    #[test]
    fn test_new_workspace_id_generates_unique_values() {
        let id1 = new_workspace_id();
        let id2 = new_workspace_id();
        // Two IDs generated in sequence must not collide (ULID monotonicity
        // guarantees this even within the same millisecond).
        assert_ne!(id1, id2, "consecutive workspace IDs must be unique");
    }

    #[test]
    fn test_hash_repository_is_deterministic() {
        let url = "https://github.com/example/repo";
        let h1 = hash_repository(url);
        let h2 = hash_repository(url);
        assert_eq!(h1, h2, "hash must be deterministic for the same input");
    }

    #[test]
    fn test_hash_repository_returns_64_char_hex() {
        let h = hash_repository("https://github.com/example/repo");
        assert_eq!(h.len(), 64, "SHA-256 hex digest must be 64 characters");
        assert!(
            h.chars().all(|c| c.is_ascii_hexdigit()),
            "digest must contain only hex digits, got: {}",
            h
        );
    }

    #[test]
    fn test_hash_repository_differs_for_different_inputs() {
        let h1 = hash_repository("https://github.com/example/repo-a");
        let h2 = hash_repository("https://github.com/example/repo-b");
        assert_ne!(h1, h2, "different inputs must produce different hashes");
    }

    #[test]
    fn test_now_rfc3339_contains_rfc3339_markers() {
        let s = now_rfc3339();
        assert!(
            s.contains('T'),
            "RFC 3339 string must contain 'T' separator, got: {}",
            s
        );
    }

    #[test]
    fn test_workspace_state_version_is_one() {
        assert_eq!(WORKSPACE_STATE_VERSION, "1");
    }
}
