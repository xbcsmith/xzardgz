//! Plugin-specific content preselection scanner.
//!
//! [`PluginContentScanner`] lets individual plugins register a custom
//! [`PatternSet`] and scan file content for relevant signals before any AI
//! analysis takes place. Findings produced here feed the plugin preselection
//! layer so that plugins can prioritise files most likely to contain issues
//! within their domain.

use std::path::Path;

use crate::scanner::findings::{FindingSeverity, ScanFinding};
use crate::scanner::patterns::PatternSet;

// ---------------------------------------------------------------------------
// PluginContentScanner
// ---------------------------------------------------------------------------

/// Scans file content for plugin-specific patterns and produces findings.
///
/// Each plugin can construct a `PluginContentScanner` with its own
/// [`PatternSet`].  During a scan, the scanner calls
/// [`PluginContentScanner::scan_content`] for each non-binary file to
/// generate plugin-targeted pre-AI findings.
///
/// Findings use severity [`FindingSeverity::Low`] because at this stage there
/// is no AI analysis confirming that a match is truly problematic.
///
/// # Examples
///
/// ```
/// use std::path::Path;
/// use xzardgz::scanner::patterns::PatternSet;
/// use xzardgz::scanner::preselect::PluginContentScanner;
///
/// let mut ps = PatternSet::new("demo");
/// ps.keywords = vec!["TODO".to_string(), "FIXME".to_string()];
///
/// let scanner = PluginContentScanner::new("my_plugin", ps);
/// let findings = scanner.scan_content(
///     Path::new("src/lib.rs"),
///     "// TODO: fix this\nfn ok() {}",
/// );
/// assert_eq!(findings.len(), 1);
/// assert_eq!(findings[0].evidence, "TODO");
/// ```
#[derive(Debug, Clone)]
pub struct PluginContentScanner {
    /// Name of the plugin this scanner targets.
    pub plugin_name: String,
    /// Pattern set defining what to look for.
    pub pattern_set: PatternSet,
}

impl PluginContentScanner {
    /// Creates a new scanner for the named plugin with the given pattern set.
    ///
    /// # Arguments
    ///
    /// * `plugin_name` - Human-readable name of the plugin.
    /// * `pattern_set` - The [`PatternSet`] whose keywords will be searched.
    ///
    /// # Returns
    ///
    /// A `PluginContentScanner` ready to call [`scan_content`](Self::scan_content).
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::scanner::patterns::PatternSet;
    /// use xzardgz::scanner::preselect::PluginContentScanner;
    ///
    /// let scanner = PluginContentScanner::new("security_plugin", PatternSet::new("secrets"));
    /// assert_eq!(scanner.plugin_name, "security_plugin");
    /// ```
    pub fn new(plugin_name: impl Into<String>, pattern_set: PatternSet) -> Self {
        Self {
            plugin_name: plugin_name.into(),
            pattern_set,
        }
    }

    /// Scans `content` at `path` and returns findings for matched keywords.
    ///
    /// For each keyword in `self.pattern_set.keywords` that appears in a line
    /// of `content`, one [`ScanFinding`] is produced:
    ///
    /// - `kind` = `"pattern_match:{pattern_set_name}"`
    ///   (e.g. `"pattern_match:secrets"`)
    /// - `file` = `path.to_string_lossy().to_string()`
    /// - `line` = 1-based line number of the first match for that keyword
    /// - `evidence` = the matching keyword
    /// - `severity` = [`FindingSeverity::Low`]
    ///
    /// The returned findings are sorted by line number for deterministic
    /// output.
    ///
    /// # Arguments
    ///
    /// * `path` - Path of the file being scanned (used to populate `file`).
    /// * `content` - UTF-8 text content of the file.
    ///
    /// # Returns
    ///
    /// A [`Vec<ScanFinding>`], sorted by line number.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::path::Path;
    /// use xzardgz::scanner::patterns::PatternSet;
    /// use xzardgz::scanner::preselect::PluginContentScanner;
    ///
    /// let mut ps = PatternSet::new("notes");
    /// ps.keywords = vec!["FIXME".to_string()];
    ///
    /// let scanner = PluginContentScanner::new("checker", ps);
    /// let findings = scanner.scan_content(Path::new("src/util.rs"), "// FIXME later\nfn run() {}");
    /// assert_eq!(findings.len(), 1);
    /// assert_eq!(findings[0].line, Some(1));
    /// ```
    pub fn scan_content(&self, path: &Path, content: &str) -> Vec<ScanFinding> {
        let file = path.to_string_lossy().to_string();
        let kind = format!("pattern_match:{}", self.pattern_set.name);
        let mut findings: Vec<ScanFinding> = Vec::new();

        for keyword in &self.pattern_set.keywords {
            if let Some((idx, _)) = content
                .lines()
                .enumerate()
                .find(|(_, line)| line.contains(keyword.as_str()))
            {
                findings.push(ScanFinding {
                    kind: kind.clone(),
                    file: file.clone(),
                    line: Some((idx + 1) as u32),
                    evidence: keyword.clone(),
                    severity: FindingSeverity::Low,
                });
            }
        }

        findings.sort_by_key(|f| f.line);
        findings
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_scanner(keywords: Vec<&str>) -> PluginContentScanner {
        let mut ps = PatternSet::new("test_set");
        ps.keywords = keywords.into_iter().map(str::to_string).collect();
        PluginContentScanner::new("test_plugin", ps)
    }

    #[test]
    fn test_plugin_content_scanner_new_sets_fields() {
        let ps = PatternSet::new("my_patterns");
        let scanner = PluginContentScanner::new("my_plugin", ps);
        assert_eq!(scanner.plugin_name, "my_plugin");
        assert_eq!(scanner.pattern_set.name, "my_patterns");
    }

    #[test]
    fn test_scan_content_finds_keyword_in_content() {
        let scanner = make_scanner(vec!["TODO"]);
        let findings = scanner.scan_content(Path::new("src/lib.rs"), "// TODO: fix this");
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].evidence, "TODO");
    }

    #[test]
    fn test_scan_content_returns_empty_for_no_match() {
        let scanner = make_scanner(vec!["FIXME"]);
        let findings = scanner.scan_content(Path::new("clean.rs"), "fn hello() {}");
        assert!(findings.is_empty());
    }

    #[test]
    fn test_scan_content_produces_correct_kind_prefix() {
        let scanner = make_scanner(vec!["secret"]);
        let findings = scanner.scan_content(Path::new("config.rs"), "let secret = \"abc\";");
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].kind, "pattern_match:test_set");
    }

    #[test]
    fn test_scan_content_finds_multiple_keywords() {
        let scanner = make_scanner(vec!["TODO", "FIXME", "HACK"]);
        let content = "// TODO: one\n// FIXME: two\nfn ok() {}";
        let findings = scanner.scan_content(Path::new("main.rs"), content);
        assert_eq!(findings.len(), 2);
        let evidences: Vec<&str> = findings.iter().map(|f| f.evidence.as_str()).collect();
        assert!(evidences.contains(&"TODO"));
        assert!(evidences.contains(&"FIXME"));
    }

    #[test]
    fn test_scan_content_findings_have_low_severity() {
        let scanner = make_scanner(vec!["password"]);
        let findings = scanner.scan_content(Path::new("auth.rs"), "let password = \"secret\";");
        assert_eq!(findings.len(), 1);
        assert_eq!(
            findings[0].severity,
            crate::scanner::findings::FindingSeverity::Low
        );
    }

    #[test]
    fn test_scan_content_line_numbers_are_one_based() {
        let scanner = make_scanner(vec!["MATCH"]);
        // MATCH is on line 3 (1-based)
        let content = "line one\nline two\nMATCH here\nline four";
        let findings = scanner.scan_content(Path::new("file.rs"), content);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].line, Some(3));
    }

    #[test]
    fn test_scan_content_findings_sorted_by_line_number() {
        // Use two keywords that match at different lines
        let scanner = make_scanner(vec!["BETA", "ALPHA"]);
        // ALPHA is on line 1, BETA on line 2
        let content = "ALPHA first\nBETA second\nneither";
        let findings = scanner.scan_content(Path::new("order.rs"), content);
        assert_eq!(findings.len(), 2);
        // Results should be sorted: ALPHA (line 1) before BETA (line 2)
        assert!(findings[0].line <= findings[1].line);
    }
}
