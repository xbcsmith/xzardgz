//! Watcher task matcher.
//!
//! [`WatcherMatcher`] filters incoming [`WatcherTaskMessage`] values against
//! configuration-defined allow-lists.  When any list is non-empty, only tasks
//! that satisfy all populated lists are forwarded to the executor.  An empty
//! matcher (all lists empty) accepts every task.

use std::collections::HashMap;

use crate::config::MatcherConfig;
use crate::watcher::task::WatcherTaskMessage;

// ---------------------------------------------------------------------------
// WatcherMatcher
// ---------------------------------------------------------------------------

/// Filters [`WatcherTaskMessage`] values according to configured allow-lists.
///
/// Each non-empty list acts as an allow-list for its respective field.  A task
/// must satisfy every non-empty list to be accepted.  An empty `WatcherMatcher`
/// (all lists empty) accepts all tasks.
///
/// # Examples
///
/// ```
/// use std::collections::HashMap;
/// use xzardgz::watcher::matcher::WatcherMatcher;
/// use xzardgz::config::MatcherConfig;
///
/// let config = MatcherConfig::default();
/// let matcher = WatcherMatcher::from_config(&config);
/// // Default config has event_types and plugins populated, so is not empty.
/// assert!(!matcher.is_empty());
/// ```
pub struct WatcherMatcher {
    event_types: Vec<String>,
    repositories: Vec<String>,
    plugins: Vec<String>,
    platforms: Vec<String>,
    metadata: HashMap<String, String>,
}

impl WatcherMatcher {
    /// Constructs a [`WatcherMatcher`] from a [`MatcherConfig`].
    ///
    /// All fields are cloned from the config so the matcher owns its data.
    ///
    /// # Arguments
    ///
    /// * `config` - The matcher configuration from the pipeline config file.
    ///
    /// # Returns
    ///
    /// A new `WatcherMatcher` populated with the config allow-lists.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::config::MatcherConfig;
    /// use xzardgz::watcher::matcher::WatcherMatcher;
    ///
    /// let mut config = MatcherConfig::default();
    /// config.event_types.clear();
    /// config.plugins.clear();
    /// let matcher = WatcherMatcher::from_config(&config);
    /// assert!(matcher.is_empty());
    /// ```
    pub fn from_config(config: &MatcherConfig) -> Self {
        Self {
            event_types: config.event_types.clone(),
            repositories: config.repositories.clone(),
            plugins: config.plugins.clone(),
            platforms: config.platforms.clone(),
            metadata: config.metadata.clone(),
        }
    }

    /// Returns `true` if `task` satisfies all non-empty allow-lists.
    ///
    /// Matching rules (each rule only applies when its list is non-empty):
    ///
    /// - **event_types**: the task's `event_type.as_str()` must appear in the list.
    /// - **repositories**: `task.repository` must appear in the list.
    /// - **plugins**: `task.plugin` must appear in the list.
    /// - **platforms**: `task.metadata["platform"]` must appear in the list.
    /// - **metadata**: every key/value pair in the matcher must be present and
    ///   equal in `task.metadata`.
    ///
    /// # Arguments
    ///
    /// * `task` - The task message to evaluate.
    ///
    /// # Returns
    ///
    /// `true` when the task passes all populated filters.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::collections::HashMap;
    /// use xzardgz::config::MatcherConfig;
    /// use xzardgz::watcher::matcher::WatcherMatcher;
    /// use xzardgz::watcher::task::{WatcherTaskMessage, WATCHER_TASK_VERSION};
    /// use xzardgz::watcher::event_type::WatcherEventType;
    ///
    /// let mut config = MatcherConfig::default();
    /// config.event_types.clear();
    /// config.plugins.clear();
    /// let matcher = WatcherMatcher::from_config(&config);
    ///
    /// let task = WatcherTaskMessage {
    ///     id: "t1".to_string(),
    ///     version: WATCHER_TASK_VERSION.to_string(),
    ///     spec_version: "1.0".to_string(),
    ///     event_type: WatcherEventType::TechnicalReviewTask,
    ///     source: "ci".to_string(),
    ///     repository: "github.com/org/repo".to_string(),
    ///     target_branch: None,
    ///     provider: None,
    ///     model: None,
    ///     plugin: "technical_review".to_string(),
    ///     plugin_config: serde_json::json!({}),
    ///     dry_run: false,
    ///     workspace_directory: None,
    ///     metadata: HashMap::new(),
    ///     requested_report_formats: vec![],
    ///     correlation_id: "c1".to_string(),
    ///     reply_topic_override: None,
    /// };
    ///
    /// assert!(matcher.matches(&task));
    /// ```
    pub fn matches(&self, task: &WatcherTaskMessage) -> bool {
        // Check event_type allow-list.
        if !self.event_types.is_empty() {
            let task_event = task.event_type.as_str();
            if !self.event_types.iter().any(|e| e == task_event) {
                return false;
            }
        }

        // Check repository allow-list.
        if !self.repositories.is_empty() && !self.repositories.iter().any(|r| r == &task.repository)
        {
            return false;
        }

        // Check plugin allow-list.
        if !self.plugins.is_empty() && !self.plugins.iter().any(|p| p == &task.plugin) {
            return false;
        }

        // Check platform allow-list (resolved from task metadata).
        if !self.platforms.is_empty() {
            match task.metadata.get("platform") {
                Some(p) if self.platforms.iter().any(|pl| pl == p) => {}
                _ => return false,
            }
        }

        // Check metadata key/value allow-list.
        for (key, value) in &self.metadata {
            match task.metadata.get(key) {
                Some(v) if v == value => {}
                _ => return false,
            }
        }

        true
    }

    /// Returns `true` if all allow-lists are empty (the matcher accepts every task).
    ///
    /// # Returns
    ///
    /// `true` when `event_types`, `repositories`, `plugins`, `platforms`, and
    /// `metadata` are all empty.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::config::MatcherConfig;
    /// use xzardgz::watcher::matcher::WatcherMatcher;
    ///
    /// let mut config = MatcherConfig::default();
    /// config.event_types.clear();
    /// config.repositories.clear();
    /// config.plugins.clear();
    /// config.platforms.clear();
    /// config.metadata.clear();
    /// let matcher = WatcherMatcher::from_config(&config);
    /// assert!(matcher.is_empty());
    /// ```
    pub fn is_empty(&self) -> bool {
        self.event_types.is_empty()
            && self.repositories.is_empty()
            && self.plugins.is_empty()
            && self.platforms.is_empty()
            && self.metadata.is_empty()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::watcher::event_type::{EVENT_TECHNICAL_REVIEW_TASK, WatcherEventType};
    use crate::watcher::task::WATCHER_TASK_VERSION;

    /// Builds a minimal [`WatcherTaskMessage`] for use in matcher tests.
    fn make_task(plugin: &str, event_type: WatcherEventType) -> WatcherTaskMessage {
        WatcherTaskMessage {
            id: "task-m-001".to_string(),
            version: WATCHER_TASK_VERSION.to_string(),
            spec_version: "1.0".to_string(),
            event_type,
            source: "ci".to_string(),
            repository: "github.com/test/repo".to_string(),
            target_branch: None,
            provider: None,
            model: None,
            plugin: plugin.to_string(),
            plugin_config: serde_json::json!({}),
            dry_run: false,
            workspace_directory: None,
            metadata: HashMap::new(),
            requested_report_formats: vec![],
            correlation_id: "corr-m-001".to_string(),
            reply_topic_override: None,
        }
    }

    /// Builds an empty [`MatcherConfig`] (all lists cleared).
    fn empty_config() -> MatcherConfig {
        let mut config = MatcherConfig::default();
        config.event_types.clear();
        config.repositories.clear();
        config.plugins.clear();
        config.platforms.clear();
        config.metadata.clear();
        config
    }

    // ------------------------------------------------------------------
    // is_empty
    // ------------------------------------------------------------------

    #[test]
    fn test_watcher_matcher_is_empty_returns_true_when_all_lists_empty() {
        let matcher = WatcherMatcher::from_config(&empty_config());
        assert!(matcher.is_empty());
    }

    #[test]
    fn test_watcher_matcher_is_empty_returns_false_when_event_types_populated() {
        let mut config = empty_config();
        config
            .event_types
            .push(EVENT_TECHNICAL_REVIEW_TASK.to_string());
        let matcher = WatcherMatcher::from_config(&config);
        assert!(!matcher.is_empty());
    }

    #[test]
    fn test_watcher_matcher_is_empty_returns_false_when_plugins_populated() {
        let mut config = empty_config();
        config.plugins.push("technical_review".to_string());
        let matcher = WatcherMatcher::from_config(&config);
        assert!(!matcher.is_empty());
    }

    #[test]
    fn test_watcher_matcher_is_empty_returns_false_when_repositories_populated() {
        let mut config = empty_config();
        config.repositories.push("github.com/test/repo".to_string());
        let matcher = WatcherMatcher::from_config(&config);
        assert!(!matcher.is_empty());
    }

    // ------------------------------------------------------------------
    // matches - event_type filter
    // ------------------------------------------------------------------

    #[test]
    fn test_watcher_matcher_matches_returns_true_when_all_filters_empty() {
        let matcher = WatcherMatcher::from_config(&empty_config());
        let task = make_task("technical_review", WatcherEventType::TechnicalReviewTask);
        assert!(matcher.matches(&task));
    }

    #[test]
    fn test_watcher_matcher_matches_returns_true_when_event_type_in_list() {
        let mut config = empty_config();
        config
            .event_types
            .push(EVENT_TECHNICAL_REVIEW_TASK.to_string());
        let matcher = WatcherMatcher::from_config(&config);
        let task = make_task("technical_review", WatcherEventType::TechnicalReviewTask);
        assert!(matcher.matches(&task));
    }

    #[test]
    fn test_watcher_matcher_matches_returns_false_when_event_type_not_in_list() {
        let mut config = empty_config();
        config
            .event_types
            .push(EVENT_TECHNICAL_REVIEW_TASK.to_string());
        let matcher = WatcherMatcher::from_config(&config);
        let task = make_task("security_review", WatcherEventType::SecurityReviewTask);
        assert!(!matcher.matches(&task));
    }

    // ------------------------------------------------------------------
    // matches - plugin filter
    // ------------------------------------------------------------------

    #[test]
    fn test_watcher_matcher_matches_returns_true_when_plugin_in_list() {
        let mut config = empty_config();
        config.plugins.push("technical_review".to_string());
        let matcher = WatcherMatcher::from_config(&config);
        let task = make_task("technical_review", WatcherEventType::TechnicalReviewTask);
        assert!(matcher.matches(&task));
    }

    #[test]
    fn test_watcher_matcher_matches_returns_false_when_plugin_not_in_list() {
        let mut config = empty_config();
        config.plugins.push("technical_review".to_string());
        let matcher = WatcherMatcher::from_config(&config);
        let task = make_task("security_review", WatcherEventType::TechnicalReviewTask);
        assert!(!matcher.matches(&task));
    }

    // ------------------------------------------------------------------
    // matches - repository filter
    // ------------------------------------------------------------------

    #[test]
    fn test_watcher_matcher_matches_returns_true_when_repository_in_list() {
        let mut config = empty_config();
        config.repositories.push("github.com/test/repo".to_string());
        let matcher = WatcherMatcher::from_config(&config);
        let task = make_task("technical_review", WatcherEventType::TechnicalReviewTask);
        assert!(matcher.matches(&task));
    }

    #[test]
    fn test_watcher_matcher_matches_returns_false_when_repository_not_in_list() {
        let mut config = empty_config();
        config
            .repositories
            .push("github.com/other/repo".to_string());
        let matcher = WatcherMatcher::from_config(&config);
        let task = make_task("technical_review", WatcherEventType::TechnicalReviewTask);
        assert!(!matcher.matches(&task));
    }

    // ------------------------------------------------------------------
    // matches - metadata filter
    // ------------------------------------------------------------------

    #[test]
    fn test_watcher_matcher_matches_returns_true_when_metadata_matches() {
        let mut config = empty_config();
        config
            .metadata
            .insert("env".to_string(), "staging".to_string());
        let matcher = WatcherMatcher::from_config(&config);

        let mut task = make_task("technical_review", WatcherEventType::TechnicalReviewTask);
        task.metadata
            .insert("env".to_string(), "staging".to_string());
        assert!(matcher.matches(&task));
    }

    #[test]
    fn test_watcher_matcher_matches_returns_false_when_metadata_value_differs() {
        let mut config = empty_config();
        config
            .metadata
            .insert("env".to_string(), "production".to_string());
        let matcher = WatcherMatcher::from_config(&config);

        let mut task = make_task("technical_review", WatcherEventType::TechnicalReviewTask);
        task.metadata
            .insert("env".to_string(), "staging".to_string());
        assert!(!matcher.matches(&task));
    }

    #[test]
    fn test_watcher_matcher_matches_returns_false_when_metadata_key_missing() {
        let mut config = empty_config();
        config
            .metadata
            .insert("env".to_string(), "staging".to_string());
        let matcher = WatcherMatcher::from_config(&config);
        let task = make_task("technical_review", WatcherEventType::TechnicalReviewTask);
        // task.metadata is empty
        assert!(!matcher.matches(&task));
    }

    // ------------------------------------------------------------------
    // matches - platform filter (resolved from metadata["platform"])
    // ------------------------------------------------------------------

    #[test]
    fn test_watcher_matcher_matches_returns_true_when_platform_in_list_and_task_has_platform() {
        let mut config = empty_config();
        config.platforms.push("github".to_string());
        let matcher = WatcherMatcher::from_config(&config);

        let mut task = make_task("technical_review", WatcherEventType::TechnicalReviewTask);
        task.metadata
            .insert("platform".to_string(), "github".to_string());
        assert!(matcher.matches(&task));
    }

    #[test]
    fn test_watcher_matcher_matches_returns_false_when_platform_set_and_task_has_no_platform() {
        let mut config = empty_config();
        config.platforms.push("github".to_string());
        let matcher = WatcherMatcher::from_config(&config);

        let task = make_task("technical_review", WatcherEventType::TechnicalReviewTask);
        // No platform in metadata.
        assert!(!matcher.matches(&task));
    }

    // ------------------------------------------------------------------
    // from_config - default config
    // ------------------------------------------------------------------

    #[test]
    fn test_watcher_matcher_from_default_config_is_not_empty() {
        let config = MatcherConfig::default();
        let matcher = WatcherMatcher::from_config(&config);
        assert!(!matcher.is_empty());
    }
}
