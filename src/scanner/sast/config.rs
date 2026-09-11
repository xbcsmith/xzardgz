//! Engine configuration for the SAST scanner.
//!
//! [`SastEngineConfig`] is the single source of truth for all tunable
//! knobs that the SAST engine exposes.  It is serializable so it can
//! be embedded in a larger YAML configuration file alongside other
//! pipeline settings.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Default constants
// ---------------------------------------------------------------------------

/// Default maximum file size scanned by the SAST engine (5 MiB).
const DEFAULT_MAX_FILE_BYTES: u64 = 5 * 1024 * 1024;

/// Default per-rule scan timeout in milliseconds (5 000 ms).
const DEFAULT_RULE_TIMEOUT_MS: u64 = 5_000;

/// Default maximum number of matches reported per file per rule (100).
const DEFAULT_MAX_MATCHES_PER_FILE: usize = 100;

// ---------------------------------------------------------------------------
// Serde default helper functions (private)
// ---------------------------------------------------------------------------

fn default_max_file_bytes() -> u64 {
    DEFAULT_MAX_FILE_BYTES
}

fn default_rule_timeout_ms() -> u64 {
    DEFAULT_RULE_TIMEOUT_MS
}

fn default_max_matches_per_file() -> usize {
    DEFAULT_MAX_MATCHES_PER_FILE
}

// ---------------------------------------------------------------------------
// Configuration struct
// ---------------------------------------------------------------------------

/// Engine configuration shared across all SAST scanning consumers.
///
/// Construct with [`SastEngineConfig::new`] or [`SastEngineConfig::default`]
/// for sensible defaults and then override individual fields as needed.
///
/// The struct is fully serializable; embed it inside a larger YAML document
/// and use `serde_yaml` to load it:
///
/// ```yaml
/// sast:
///   max_file_bytes: 1048576
///   rule_timeout_ms: 2000
/// ```
///
/// # Examples
///
/// ```
/// use xzardgz::scanner::sast::config::SastEngineConfig;
///
/// let config = SastEngineConfig::new();
/// assert_eq!(config.max_file_bytes, 5_242_880);
/// assert_eq!(config.rule_timeout_ms, 5_000);
/// assert_eq!(config.max_matches_per_file, 100);
/// assert_eq!(config.jobs, 0);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SastEngineConfig {
    /// Maximum file size in bytes to scan.
    ///
    /// Files larger than this are skipped and reported as
    /// [`SastError::FileTooLarge`](crate::scanner::sast::error::SastError::FileTooLarge).
    /// Default: 5 MiB (5_242_880 bytes).
    #[serde(default = "default_max_file_bytes")]
    pub max_file_bytes: u64,

    /// Per-rule scan timeout in milliseconds.
    ///
    /// A rule evaluation that exceeds this budget is aborted and reported as
    /// [`SastError::ScanTimeout`](crate::scanner::sast::error::SastError::ScanTimeout).
    /// Default: 5 000 ms.
    #[serde(default = "default_rule_timeout_ms")]
    pub rule_timeout_ms: u64,

    /// Maximum number of matches emitted per file per rule.
    ///
    /// Once this limit is reached for a (file, rule) pair the engine stops
    /// searching for further matches in that file.  Default: 100.
    #[serde(default = "default_max_matches_per_file")]
    pub max_matches_per_file: usize,

    /// Number of parallel worker threads.
    ///
    /// `0` means the engine will use
    /// [`std::thread::available_parallelism`] to determine the thread count
    /// at runtime.  Default: 0.
    #[serde(default)]
    pub jobs: usize,
}

// ---------------------------------------------------------------------------
// Default implementation
// ---------------------------------------------------------------------------

impl Default for SastEngineConfig {
    /// Creates a `SastEngineConfig` populated with production-safe defaults.
    ///
    /// # Returns
    ///
    /// A `SastEngineConfig` with:
    /// - `max_file_bytes`: 5 MiB
    /// - `rule_timeout_ms`: 5 000 ms
    /// - `max_matches_per_file`: 100
    /// - `jobs`: 0 (use available parallelism)
    fn default() -> Self {
        Self {
            max_file_bytes: DEFAULT_MAX_FILE_BYTES,
            rule_timeout_ms: DEFAULT_RULE_TIMEOUT_MS,
            max_matches_per_file: DEFAULT_MAX_MATCHES_PER_FILE,
            jobs: 0,
        }
    }
}

// ---------------------------------------------------------------------------
// Constructor
// ---------------------------------------------------------------------------

impl SastEngineConfig {
    /// Creates a new `SastEngineConfig` with default values.
    ///
    /// This is equivalent to [`SastEngineConfig::default`] and is provided
    /// as a named constructor for clarity at call sites.
    ///
    /// # Returns
    ///
    /// A `SastEngineConfig` with all fields set to their documented defaults.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::scanner::sast::config::SastEngineConfig;
    ///
    /// let config = SastEngineConfig::new();
    /// assert_eq!(config.max_file_bytes, 5_242_880);
    /// ```
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sast_engine_config_default_max_file_bytes_is_five_mib() {
        let config = SastEngineConfig::default();
        assert_eq!(config.max_file_bytes, 5 * 1024 * 1024);
    }

    #[test]
    fn test_sast_engine_config_default_rule_timeout_ms_is_five_thousand() {
        let config = SastEngineConfig::default();
        assert_eq!(config.rule_timeout_ms, 5_000);
    }

    #[test]
    fn test_sast_engine_config_default_max_matches_per_file_is_one_hundred() {
        let config = SastEngineConfig::default();
        assert_eq!(config.max_matches_per_file, 100);
    }

    #[test]
    fn test_sast_engine_config_default_jobs_is_zero() {
        let config = SastEngineConfig::default();
        assert_eq!(config.jobs, 0);
    }

    #[test]
    fn test_sast_engine_config_new_equals_default() {
        let via_new = SastEngineConfig::new();
        let via_default = SastEngineConfig::default();
        assert_eq!(via_new.max_file_bytes, via_default.max_file_bytes);
        assert_eq!(via_new.rule_timeout_ms, via_default.rule_timeout_ms);
        assert_eq!(
            via_new.max_matches_per_file,
            via_default.max_matches_per_file
        );
        assert_eq!(via_new.jobs, via_default.jobs);
    }

    #[test]
    fn test_sast_engine_config_serialization_roundtrip_preserves_defaults() {
        let original = SastEngineConfig::new();
        let yaml = serde_yaml::to_string(&original)
            .expect("serialization must succeed for a valid config");
        let restored: SastEngineConfig =
            serde_yaml::from_str(&yaml).expect("deserialization must succeed for valid yaml");
        assert_eq!(original.max_file_bytes, restored.max_file_bytes);
        assert_eq!(original.rule_timeout_ms, restored.rule_timeout_ms);
        assert_eq!(original.max_matches_per_file, restored.max_matches_per_file);
        assert_eq!(original.jobs, restored.jobs);
    }

    #[test]
    fn test_sast_engine_config_serialization_roundtrip_preserves_custom_values() {
        let mut original = SastEngineConfig::new();
        original.max_file_bytes = 1_000_000;
        original.rule_timeout_ms = 1_500;
        original.max_matches_per_file = 50;
        original.jobs = 4;

        let yaml = serde_yaml::to_string(&original)
            .expect("serialization must succeed for a valid config");
        let restored: SastEngineConfig =
            serde_yaml::from_str(&yaml).expect("deserialization must succeed for valid yaml");

        assert_eq!(restored.max_file_bytes, 1_000_000);
        assert_eq!(restored.rule_timeout_ms, 1_500);
        assert_eq!(restored.max_matches_per_file, 50);
        assert_eq!(restored.jobs, 4);
    }

    #[test]
    fn test_sast_engine_config_partial_yaml_uses_serde_defaults() {
        // When only some fields are present, serde should fill the rest with defaults.
        let yaml = "jobs: 8\n";
        let config: SastEngineConfig =
            serde_yaml::from_str(yaml).expect("partial yaml deserialization must succeed");
        assert_eq!(config.jobs, 8);
        assert_eq!(config.max_file_bytes, DEFAULT_MAX_FILE_BYTES);
        assert_eq!(config.rule_timeout_ms, DEFAULT_RULE_TIMEOUT_MS);
        assert_eq!(config.max_matches_per_file, DEFAULT_MAX_MATCHES_PER_FILE);
    }

    #[test]
    fn test_sast_engine_config_clone_is_independent() {
        let original = SastEngineConfig::new();
        let mut cloned = original.clone();
        cloned.jobs = 99;
        assert_ne!(original.jobs, cloned.jobs);
    }
}
