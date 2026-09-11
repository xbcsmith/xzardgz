//! Language enum for the SAST engine.
//!
//! [`Language`] discriminates between AST-backed parsing (Rust via
//! tree-sitter) and the two text-only scanning modes (Regex and Generic).
//! Only `Language::Rust` requires an AST parse; the other variants operate
//! directly on raw file bytes.

use ast_grep_language::SupportLang;
use std::path::Path;

// ---------------------------------------------------------------------------
// Language enum
// ---------------------------------------------------------------------------

/// Language discriminant for the SAST engine.
///
/// Only [`Language::Rust`] has AST-backed matching in Phase 1 of the engine.
/// [`Language::Regex`] and [`Language::Generic`] operate on raw file bytes
/// and do not require a tree-sitter parse.
///
/// # Examples
///
/// ```
/// use xzardgz::scanner::sast::ast::lang::Language;
///
/// assert_eq!(Language::from_extension("rs"), Some(Language::Rust));
/// assert!(Language::Rust.requires_ast());
/// assert!(!Language::Generic.requires_ast());
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Language {
    /// Rust source code, backed by tree-sitter-rust.
    Rust,
    /// Regex / line-based scanning mode.  No AST parsing.
    Regex,
    /// Generic text scanning mode.  No AST parsing.
    Generic,
}

// ---------------------------------------------------------------------------
// Methods
// ---------------------------------------------------------------------------

impl Language {
    /// Detect a language from a file extension (case-insensitive).
    ///
    /// Returns `None` if the extension is not recognised by this engine
    /// version.  Unknown extensions are intentionally left as `None` so
    /// callers can decide whether to fall back to [`Language::Generic`].
    ///
    /// # Arguments
    ///
    /// * `ext` - File extension without a leading dot (e.g. `"rs"`).
    ///
    /// # Returns
    ///
    /// `Some(Language)` when the extension is recognised, `None` otherwise.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::scanner::sast::ast::lang::Language;
    ///
    /// assert_eq!(Language::from_extension("rs"), Some(Language::Rust));
    /// assert_eq!(Language::from_extension("RS"), Some(Language::Rust));
    /// assert_eq!(Language::from_extension("py"), None);
    /// ```
    #[must_use]
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_lowercase().as_str() {
            "rs" => Some(Language::Rust),
            _ => None,
        }
    }

    /// Detect a language from a full file path.
    ///
    /// The detection strategy is applied in this order:
    ///
    /// 1. If the path has a file extension, attempt [`Language::from_extension`].
    /// 2. If the path has no extension, read the first 64 bytes of the file
    ///    and check for a shebang that names a Rust interpreter
    ///    (`/usr/bin/env rust` or `/usr/bin/rust`).
    /// 3. Default to [`Language::Generic`] if nothing matched.
    ///
    /// IO errors during shebang detection are silently ignored and cause the
    /// function to fall through to the `Generic` default.
    ///
    /// # Arguments
    ///
    /// * `path` - Full or relative path to the file.
    ///
    /// # Returns
    ///
    /// A [`Language`] variant.  Never fails; returns `Generic` as a safe
    /// fallback.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::path::Path;
    /// use xzardgz::scanner::sast::ast::lang::Language;
    ///
    /// assert_eq!(Language::from_path(Path::new("src/main.rs")), Language::Rust);
    /// ```
    #[must_use]
    pub fn from_path(path: &Path) -> Self {
        if let Some(ext) = path.extension() {
            let ext_str = ext.to_string_lossy();
            if let Some(lang) = Self::from_extension(&ext_str) {
                return lang;
            }
            // Extension exists but is not recognised; fall through to Generic.
            return Language::Generic;
        }

        // No extension: attempt shebang detection.
        if let Ok(bytes) = std::fs::read(path) {
            let prefix = &bytes[..64.min(bytes.len())];
            if let Ok(text) = std::str::from_utf8(prefix)
                && text.starts_with("#!")
            {
                // `lines()` on a non-empty string always yields at least one item.
                let first_line = text.lines().next().unwrap_or("");
                if first_line.contains("/usr/bin/env rust") || first_line.contains("/usr/bin/rust")
                {
                    return Language::Rust;
                }
            }
        }

        Language::Generic
    }

    /// Map this language to the corresponding ast-grep [`SupportLang`].
    ///
    /// Returns `None` for [`Language::Regex`] and [`Language::Generic`]
    /// because those modes do not use the tree-sitter AST.
    ///
    /// # Returns
    ///
    /// `Some(SupportLang)` for AST-backed languages, `None` for text-only
    /// modes.
    ///
    /// # Examples
    ///
    /// ```
    /// use ast_grep_language::SupportLang;
    /// use xzardgz::scanner::sast::ast::lang::Language;
    ///
    /// assert!(Language::Rust.to_ast_grep().is_some());
    /// assert!(Language::Generic.to_ast_grep().is_none());
    /// ```
    #[must_use]
    pub fn to_ast_grep(self) -> Option<SupportLang> {
        match self {
            Language::Rust => Some(SupportLang::Rust),
            Language::Regex | Language::Generic => None,
        }
    }

    /// Returns `true` if this language variant requires an AST parse.
    ///
    /// Only [`Language::Rust`] returns `true` in Phase 1.
    ///
    /// # Returns
    ///
    /// `true` when the engine must call the tree-sitter parser for this
    /// language; `false` for text-only modes.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::scanner::sast::ast::lang::Language;
    ///
    /// assert!(Language::Rust.requires_ast());
    /// assert!(!Language::Regex.requires_ast());
    /// assert!(!Language::Generic.requires_ast());
    /// ```
    #[must_use]
    pub fn requires_ast(self) -> bool {
        self.to_ast_grep().is_some()
    }

    /// Parse a language name as used in Semgrep rule `languages:` lists.
    ///
    /// Matching is case-insensitive.  Returns `None` for names not recognised
    /// by this engine version.
    ///
    /// # Arguments
    ///
    /// * `name` - Language name string from a Semgrep rule YAML file.
    ///
    /// # Returns
    ///
    /// `Some(Language)` when the name is recognised, `None` otherwise.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzardgz::scanner::sast::ast::lang::Language;
    ///
    /// assert_eq!(Language::from_semgrep_name("rust"), Some(Language::Rust));
    /// assert_eq!(Language::from_semgrep_name("RUST"), Some(Language::Rust));
    /// assert_eq!(Language::from_semgrep_name("generic"), Some(Language::Generic));
    /// assert_eq!(Language::from_semgrep_name("java"), None);
    /// ```
    #[must_use]
    pub fn from_semgrep_name(name: &str) -> Option<Self> {
        match name.to_lowercase().as_str() {
            "rust" => Some(Language::Rust),
            "regex" => Some(Language::Regex),
            "generic" => Some(Language::Generic),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_from_extension_rust_lower() {
        assert_eq!(Language::from_extension("rs"), Some(Language::Rust));
    }

    #[test]
    fn test_from_extension_uppercase_returns_rust() {
        assert_eq!(Language::from_extension("RS"), Some(Language::Rust));
    }

    #[test]
    fn test_from_extension_unknown_returns_none() {
        assert_eq!(Language::from_extension("py"), None);
    }

    #[test]
    fn test_from_extension_empty_string_returns_none() {
        assert_eq!(Language::from_extension(""), None);
    }

    #[test]
    fn test_from_path_rs_file_detects_rust() {
        let path = Path::new("src/main.rs");
        assert_eq!(Language::from_path(path), Language::Rust);
    }

    #[test]
    fn test_from_path_extensionless_defaults_to_generic() {
        let mut file = tempfile::Builder::new()
            .prefix("sast_lang_test_")
            .suffix("")
            .tempfile()
            .expect("tempfile creation must succeed in tests");
        write!(file, "hello world").expect("write must succeed in tests");
        let lang = Language::from_path(file.path());
        assert_eq!(lang, Language::Generic);
    }

    #[test]
    fn test_from_path_extensionless_with_rust_shebang_detects_rust() {
        let mut file = tempfile::Builder::new()
            .prefix("sast_lang_shebang_")
            .suffix("")
            .tempfile()
            .expect("tempfile creation must succeed in tests");
        write!(file, "#!/usr/bin/env rust\nfn main() {{}}").expect("write must succeed in tests");
        let lang = Language::from_path(file.path());
        assert_eq!(lang, Language::Rust);
    }

    #[test]
    fn test_from_path_nonexistent_extensionless_defaults_to_generic() {
        let path = Path::new("/nonexistent/no_extension_file");
        assert_eq!(Language::from_path(path), Language::Generic);
    }

    #[test]
    fn test_from_path_unknown_extension_defaults_to_generic() {
        let path = Path::new("script.py");
        assert_eq!(Language::from_path(path), Language::Generic);
    }

    #[test]
    fn test_to_ast_grep_rust_returns_some() {
        let result = Language::Rust.to_ast_grep();
        assert!(result.is_some());
        assert_eq!(result, Some(SupportLang::Rust));
    }

    #[test]
    fn test_to_ast_grep_regex_returns_none() {
        assert_eq!(Language::Regex.to_ast_grep(), None);
    }

    #[test]
    fn test_to_ast_grep_generic_returns_none() {
        assert_eq!(Language::Generic.to_ast_grep(), None);
    }

    #[test]
    fn test_requires_ast_rust_is_true() {
        assert!(Language::Rust.requires_ast());
    }

    #[test]
    fn test_requires_ast_generic_is_false() {
        assert!(!Language::Generic.requires_ast());
    }

    #[test]
    fn test_requires_ast_regex_is_false() {
        assert!(!Language::Regex.requires_ast());
    }

    #[test]
    fn test_from_semgrep_name_all_variants() {
        assert_eq!(Language::from_semgrep_name("rust"), Some(Language::Rust));
        assert_eq!(Language::from_semgrep_name("RUST"), Some(Language::Rust));
        assert_eq!(Language::from_semgrep_name("Rust"), Some(Language::Rust));
        assert_eq!(Language::from_semgrep_name("regex"), Some(Language::Regex));
        assert_eq!(Language::from_semgrep_name("REGEX"), Some(Language::Regex));
        assert_eq!(
            Language::from_semgrep_name("generic"),
            Some(Language::Generic)
        );
        assert_eq!(
            Language::from_semgrep_name("GENERIC"),
            Some(Language::Generic)
        );
        assert_eq!(Language::from_semgrep_name("java"), None);
        assert_eq!(Language::from_semgrep_name("python"), None);
        assert_eq!(Language::from_semgrep_name(""), None);
    }
}
