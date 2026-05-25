//! Language detection by file extension.
//!
//! Provides [`detect_language`] for mapping file paths to human-readable
//! language names, and [`is_binary_content`] for detecting binary files
//! before attempting text analysis.

use std::path::Path;

// ---------------------------------------------------------------------------
// Language detection
// ---------------------------------------------------------------------------

/// Detects the programming language of a file from its extension.
///
/// Returns a language name string (e.g. `"Rust"`, `"Python"`) or `None` for
/// unrecognised extensions. The special file name `Dockerfile` (exact match,
/// no extension) is handled as its own language.
///
/// Detection is case-insensitive on the extension.
///
/// # Arguments
///
/// * `path` - Path to the file (only the name and extension are inspected).
///
/// # Returns
///
/// `Some(language_name)` for known file types, `None` otherwise.
///
/// # Examples
///
/// ```
/// use std::path::Path;
/// use xzardgz::scanner::language::detect_language;
///
/// assert_eq!(detect_language(Path::new("src/main.rs")).as_deref(), Some("Rust"));
/// assert_eq!(detect_language(Path::new("script.py")).as_deref(), Some("Python"));
/// assert_eq!(detect_language(Path::new("Dockerfile")).as_deref(), Some("Dockerfile"));
/// assert!(detect_language(Path::new("archive.bin")).is_none());
/// ```
pub fn detect_language(path: &Path) -> Option<String> {
    // Special case: exact file name "Dockerfile" (no extension)
    if let Some(name) = path.file_name().and_then(|n| n.to_str())
        && name == "Dockerfile"
    {
        return Some("Dockerfile".to_string());
    }

    let ext = path.extension()?.to_str()?.to_lowercase();
    extension_to_language(&ext).map(str::to_string)
}

/// Maps a lowercase file extension to a language name.
///
/// Returns `None` for unknown extensions.
fn extension_to_language(ext: &str) -> Option<&'static str> {
    match ext {
        "rs" => Some("Rust"),
        "py" => Some("Python"),
        "js" | "mjs" | "cjs" => Some("JavaScript"),
        "jsx" => Some("JavaScript"),
        "ts" => Some("TypeScript"),
        "tsx" => Some("TypeScript"),
        "go" => Some("Go"),
        "java" => Some("Java"),
        "c" | "h" => Some("C"),
        "cpp" | "cc" | "cxx" | "hpp" | "hxx" => Some("C++"),
        "rb" => Some("Ruby"),
        "php" => Some("PHP"),
        "swift" => Some("Swift"),
        "kt" | "kts" => Some("Kotlin"),
        "cs" => Some("C#"),
        "sh" | "bash" | "zsh" => Some("Shell"),
        "yaml" | "yml" => Some("YAML"),
        "json" => Some("JSON"),
        "toml" => Some("TOML"),
        "md" | "markdown" => Some("Markdown"),
        "html" | "htm" => Some("HTML"),
        "css" | "scss" | "sass" | "less" => Some("CSS"),
        "sql" => Some("SQL"),
        "zig" => Some("Zig"),
        "ex" | "exs" => Some("Elixir"),
        "xml" => Some("XML"),
        "txt" => Some("Text"),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Binary detection
// ---------------------------------------------------------------------------

/// Returns `true` if the byte slice appears to contain binary (non-text) data.
///
/// Inspects up to the first 8 192 bytes for null bytes (`0x00`), which are a
/// reliable indicator that a file is binary rather than text. This heuristic
/// matches the approach used by `git diff` and many other tools.
///
/// # Arguments
///
/// * `bytes` - Raw file bytes to inspect.
///
/// # Returns
///
/// `true` if a null byte is found in the first 8 192 bytes; `false` otherwise.
///
/// # Examples
///
/// ```
/// use xzardgz::scanner::language::is_binary_content;
///
/// assert!(is_binary_content(&[0x89, 0x50, 0x4e, 0x47, 0x00]));
/// assert!(!is_binary_content(b"fn main() {}"));
/// assert!(!is_binary_content(b""));
/// ```
pub fn is_binary_content(bytes: &[u8]) -> bool {
    bytes.iter().take(8_192).any(|&b| b == 0)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_language_rust_file_returns_rust() {
        assert_eq!(
            detect_language(Path::new("src/main.rs")).as_deref(),
            Some("Rust")
        );
    }

    #[test]
    fn test_detect_language_python_file_returns_python() {
        assert_eq!(
            detect_language(Path::new("script.py")).as_deref(),
            Some("Python")
        );
    }

    #[test]
    fn test_detect_language_typescript_file_returns_typescript() {
        assert_eq!(
            detect_language(Path::new("app.ts")).as_deref(),
            Some("TypeScript")
        );
        assert_eq!(
            detect_language(Path::new("component.tsx")).as_deref(),
            Some("TypeScript")
        );
    }

    #[test]
    fn test_detect_language_javascript_variants_return_javascript() {
        assert_eq!(
            detect_language(Path::new("app.js")).as_deref(),
            Some("JavaScript")
        );
        assert_eq!(
            detect_language(Path::new("module.mjs")).as_deref(),
            Some("JavaScript")
        );
        assert_eq!(
            detect_language(Path::new("common.cjs")).as_deref(),
            Some("JavaScript")
        );
        assert_eq!(
            detect_language(Path::new("ui.jsx")).as_deref(),
            Some("JavaScript")
        );
    }

    #[test]
    fn test_detect_language_yaml_file_returns_yaml() {
        assert_eq!(
            detect_language(Path::new("config.yaml")).as_deref(),
            Some("YAML")
        );
        assert_eq!(
            detect_language(Path::new("config.yml")).as_deref(),
            Some("YAML")
        );
    }

    #[test]
    fn test_detect_language_toml_file_returns_toml() {
        assert_eq!(
            detect_language(Path::new("Cargo.toml")).as_deref(),
            Some("TOML")
        );
    }

    #[test]
    fn test_detect_language_go_file_returns_go() {
        assert_eq!(detect_language(Path::new("main.go")).as_deref(), Some("Go"));
    }

    #[test]
    fn test_detect_language_unknown_extension_returns_none() {
        assert!(detect_language(Path::new("archive.bin")).is_none());
        assert!(detect_language(Path::new("data.xyz")).is_none());
    }

    #[test]
    fn test_detect_language_no_extension_returns_none() {
        // Files with no extension (except Dockerfile) return None
        assert!(detect_language(Path::new("Makefile")).is_none());
        assert!(detect_language(Path::new("README")).is_none());
    }

    #[test]
    fn test_detect_language_dockerfile_exact_name_returns_dockerfile() {
        assert_eq!(
            detect_language(Path::new("Dockerfile")).as_deref(),
            Some("Dockerfile")
        );
        assert_eq!(
            detect_language(Path::new("some/path/Dockerfile")).as_deref(),
            Some("Dockerfile")
        );
    }

    #[test]
    fn test_detect_language_case_insensitive_extension() {
        // Extensions are lowercased before matching
        assert_eq!(
            detect_language(Path::new("Main.RS")).as_deref(),
            Some("Rust")
        );
        assert_eq!(
            detect_language(Path::new("Script.PY")).as_deref(),
            Some("Python")
        );
    }

    #[test]
    fn test_detect_language_cpp_variants_return_cpp() {
        for ext in &["cpp", "cc", "cxx", "hpp", "hxx"] {
            let path = format!("file.{}", ext);
            assert_eq!(
                detect_language(Path::new(&path)).as_deref(),
                Some("C++"),
                "expected C++ for .{} extension",
                ext
            );
        }
    }

    #[test]
    fn test_is_binary_content_with_null_byte_returns_true() {
        let bytes = vec![b'h', b'e', b'l', b'l', b'o', 0x00, b'w'];
        assert!(is_binary_content(&bytes));
    }

    #[test]
    fn test_is_binary_content_with_text_returns_false() {
        assert!(!is_binary_content(b"fn main() { println!(\"hello\"); }"));
    }

    #[test]
    fn test_is_binary_content_empty_slice_returns_false() {
        assert!(!is_binary_content(b""));
    }

    #[test]
    fn test_is_binary_content_only_checks_first_8192_bytes() {
        // Null byte beyond the 8192-byte window should NOT trigger detection
        let mut bytes = vec![b'a'; 8_192];
        bytes.push(0x00); // null byte at position 8192 (outside window)
        assert!(!is_binary_content(&bytes));
    }

    #[test]
    fn test_is_binary_content_png_header_returns_true() {
        // A real PNG file includes null bytes in the IHDR chunk length.
        // We include the 4-byte IHDR length prefix (which is 0x00 0x00 0x00 0x0d)
        // after the standard 8-byte magic bytes.
        let png_with_null: &[u8] = &[
            0x89, 0x50, 0x4e, 0x47, // PNG magic
            0x0d, 0x0a, 0x1a, 0x0a, // continuation bytes
            0x00, 0x00, 0x00, 0x0d, // IHDR chunk length (contains null bytes)
        ];
        assert!(is_binary_content(png_with_null));
    }
}
