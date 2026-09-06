//! Markdown parser for `AGENTS.md` governance rule extraction.
//!
//! This module implements [`parse_agents_md`], which converts a Markdown
//! `AGENTS.md` file into a list of [`GovernanceRule`]s by extracting list
//! items under any heading and classifying their enforcement level from
//! keywords in the item text.
//!
//! # Parsing Convention
//!
//! The parser recognises any bulleted or numbered list item that appears under
//! a Markdown heading.  The enforcement level is inferred from whole-word
//! keyword matches in the item text (case-insensitive):
//!
//! | Keyword                | Enforcement              |
//! |------------------------|--------------------------|
//! | `MUST` or `REQUIRED`   | [`Required`]             |
//! | `MAY` or `OPTIONAL`    | [`Optional`]             |
//! | All other items        | [`Recommended`] (default) |
//!
//! # Rule ID Generation
//!
//! Rule IDs follow the scheme `agents_md.<heading_slug>.<n>` where
//! `<heading_slug>` is derived from the heading text by lowercasing and
//! replacing consecutive non-alphanumeric characters with a single `_`,
//! and `<n>` is a 1-based counter that resets for each new heading.
//!
//! # Examples
//!
//! ```
//! use xzardgz::governance::parser::parse_agents_md;
//! use xzardgz::governance::EnforcementLevel;
//!
//! let md = "## Rules\n\n- You MUST wash your hands.\n- You MAY skip dessert.\n";
//! let rules = parse_agents_md(md);
//! assert_eq!(rules.len(), 2);
//! assert_eq!(rules[0].enforcement, EnforcementLevel::Required);
//! assert_eq!(rules[1].enforcement, EnforcementLevel::Optional);
//! ```

use std::path::PathBuf;

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

use crate::governance::rules::{EnforcementLevel, GovernanceRule, RuleSource};

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

/// Byte-level whole-word search in an already-uppercased string.
///
/// A word boundary is defined as the character before or after the match
/// position not being ASCII alphabetic (or the position being the start or
/// end of the string).  This prevents partial matches such as "MAY" inside
/// "MANDATORY" or "MUST" inside "MUSTARD".
fn contains_word(text_upper: &str, word: &str) -> bool {
    let bytes = text_upper.as_bytes();
    let word_bytes = word.as_bytes();
    let wlen = word_bytes.len();
    let tlen = bytes.len();
    let mut i = 0;
    while i + wlen <= tlen {
        if bytes[i..i + wlen] == *word_bytes {
            let before_ok = i == 0 || !bytes[i - 1].is_ascii_alphabetic();
            let after_ok = i + wlen == tlen || !bytes[i + wlen].is_ascii_alphabetic();
            if before_ok && after_ok {
                return true;
            }
        }
        i += 1;
    }
    false
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Infers an [`EnforcementLevel`] from whole-word, case-insensitive keyword
/// matches in the provided text.
///
/// The scan is performed on the uppercased form of `text` so that keywords
/// written in any case (e.g. "must", "Must", "MUST") are all detected.
///
/// # Arguments
///
/// * `text` - The item text to scan for enforcement keywords.
///
/// # Returns
///
/// - [`EnforcementLevel::Required`] when the text contains the whole word
///   `MUST` or `REQUIRED`.
/// - [`EnforcementLevel::Optional`] when the text contains the whole word
///   `MAY` or `OPTIONAL`.
/// - [`EnforcementLevel::Recommended`] otherwise (the default).
///
/// # Examples
///
/// ```
/// use xzardgz::governance::parser::infer_enforcement;
/// use xzardgz::governance::EnforcementLevel;
///
/// assert_eq!(infer_enforcement("You MUST do this."), EnforcementLevel::Required);
/// assert_eq!(infer_enforcement("You MAY skip this."), EnforcementLevel::Optional);
/// assert_eq!(infer_enforcement("Consider doing this."), EnforcementLevel::Recommended);
/// ```
pub fn infer_enforcement(text: &str) -> EnforcementLevel {
    let upper = text.to_uppercase();
    if contains_word(&upper, "MUST") || contains_word(&upper, "REQUIRED") {
        EnforcementLevel::Required
    } else if contains_word(&upper, "MAY") || contains_word(&upper, "OPTIONAL") {
        EnforcementLevel::Optional
    } else {
        EnforcementLevel::Recommended
    }
}

/// Converts a Markdown heading string into a stable lowercase slug.
///
/// The conversion rules are:
/// 1. Lowercase the entire string.
/// 2. Walk each character: if it is alphanumeric emit it as-is; otherwise
///    emit a single `_` only if the last emitted character was not already `_`
///    (collapsing consecutive non-alphanumeric runs into one separator).
/// 3. Strip any trailing `_`.
///
/// # Arguments
///
/// * `heading` - The raw heading text to slugify.
///
/// # Returns
///
/// A `String` containing the slug suitable for use in a rule ID.
///
/// # Examples
///
/// ```
/// use xzardgz::governance::parser::heading_to_slug;
///
/// assert_eq!(heading_to_slug("Rule 1: File Extensions"), "rule_1_file_extensions");
/// assert_eq!(heading_to_slug("Critical Rules"), "critical_rules");
/// assert_eq!(heading_to_slug("  Spaces  "), "spaces");
/// ```
pub fn heading_to_slug(heading: &str) -> String {
    let mut slug = String::with_capacity(heading.len());
    for ch in heading.to_lowercase().chars() {
        if ch.is_alphanumeric() {
            slug.push(ch);
        } else if !slug.ends_with('_') {
            slug.push('_');
        }
    }
    // Strip trailing underscore.
    if slug.ends_with('_') {
        slug.pop();
    }
    // Strip leading underscore.
    if slug.starts_with('_') {
        slug.remove(0);
    }
    slug
}

/// Parses an `AGENTS.md` Markdown string into a list of [`GovernanceRule`]s.
///
/// The parser walks all pulldown-cmark events and emits one rule per
/// top-level list item that appears under a Markdown heading.  Nested list
/// items are ignored.  The enforcement level of each rule is inferred from
/// whole-word keyword matches in the item text via [`infer_enforcement`].
///
/// Rule IDs follow the scheme `agents_md.<heading_slug>.<n>` where
/// `<heading_slug>` comes from [`heading_to_slug`] and `<n>` is a 1-based
/// counter that resets for every new heading encountered.
///
/// All rules are tagged with
/// `RuleSource::RepositoryFile { path: PathBuf::from("AGENTS.md") }`.
///
/// # Arguments
///
/// * `content` - The full Markdown text of an `AGENTS.md` file.
///
/// # Returns
///
/// A `Vec<GovernanceRule>` — possibly empty when the document contains no
/// headings or no list items under any heading.
///
/// # Examples
///
/// ```
/// use xzardgz::governance::parser::parse_agents_md;
/// use xzardgz::governance::EnforcementLevel;
///
/// let md = "## Rules\n\n- You MUST wash your hands.\n- You MAY skip dessert.\n";
/// let rules = parse_agents_md(md);
/// assert_eq!(rules.len(), 2);
/// assert_eq!(rules[0].enforcement, EnforcementLevel::Required);
/// assert_eq!(rules[1].enforcement, EnforcementLevel::Optional);
/// ```
pub fn parse_agents_md(content: &str) -> Vec<GovernanceRule> {
    let mut rules: Vec<GovernanceRule> = Vec::new();

    let mut current_heading: Option<String> = None;
    let mut heading_text = String::new();
    let mut in_heading = false;
    let mut item_depth: usize = 0;
    let mut item_text = String::new();
    let mut heading_item_count: usize = 0;

    let parser = Parser::new_ext(content, Options::empty());

    for event in parser {
        match event {
            Event::Start(Tag::Heading { .. }) => {
                in_heading = true;
                heading_text.clear();
            }
            Event::End(TagEnd::Heading(_)) => {
                in_heading = false;
                current_heading = Some(heading_text.trim().to_string());
                heading_item_count = 0;
            }
            Event::Start(Tag::Item) => {
                item_depth += 1;
                if item_depth == 1 {
                    item_text.clear();
                }
            }
            Event::End(TagEnd::Item) => {
                if item_depth == 1
                    && let Some(ref heading) = current_heading
                {
                    let text = item_text.trim().to_string();
                    if !text.is_empty() {
                        heading_item_count += 1;
                        let slug = heading_to_slug(heading);
                        let id = format!("agents_md.{}.{}", slug, heading_item_count);
                        rules.push(GovernanceRule {
                            id,
                            enforcement: infer_enforcement(&text),
                            description: text,
                            source: RuleSource::RepositoryFile {
                                path: PathBuf::from("AGENTS.md"),
                            },
                        });
                    }
                }
                item_depth = item_depth.saturating_sub(1);
            }
            Event::Text(t) => {
                if in_heading {
                    heading_text.push_str(&t);
                } else if item_depth == 1 {
                    item_text.push_str(&t);
                }
            }
            Event::Code(c) => {
                if item_depth == 1 {
                    item_text.push('`');
                    item_text.push_str(&c);
                    item_text.push('`');
                }
            }
            Event::SoftBreak | Event::HardBreak if item_depth == 1 => {
                item_text.push(' ');
            }
            _ => {}
        }
    }

    rules
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::governance::rules::RuleSource;

    // --- parse_agents_md ---

    #[test]
    fn test_parse_agents_md_with_empty_string_returns_empty() {
        let rules = parse_agents_md("");
        assert!(rules.is_empty());
    }

    #[test]
    fn test_parse_agents_md_with_no_headings_returns_empty() {
        let md = "- Some item without a heading.\n- Another item.\n";
        let rules = parse_agents_md(md);
        assert!(rules.is_empty());
    }

    #[test]
    fn test_parse_agents_md_with_heading_but_no_list_returns_empty() {
        let md = "## My Heading\n\nThis is a paragraph, not a list.\n";
        let rules = parse_agents_md(md);
        assert!(rules.is_empty());
    }

    #[test]
    fn test_parse_agents_md_with_single_rule_returns_one_rule() {
        let md = "## Rules\n\n- Follow the coding style.\n";
        let rules = parse_agents_md(md);
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].description, "Follow the coding style.");
    }

    #[test]
    fn test_parse_agents_md_rule_id_includes_heading_slug_and_index() {
        let md = "## Critical Rules\n\n- First rule.\n- Second rule.\n";
        let rules = parse_agents_md(md);
        assert_eq!(rules.len(), 2);
        assert_eq!(rules[0].id, "agents_md.critical_rules.1");
        assert_eq!(rules[1].id, "agents_md.critical_rules.2");
    }

    #[test]
    fn test_parse_agents_md_counter_resets_at_each_new_heading() {
        let md = "## Section A\n\n- Rule one.\n\n## Section B\n\n- Another rule.\n";
        let rules = parse_agents_md(md);
        assert_eq!(rules.len(), 2);
        assert_eq!(rules[0].id, "agents_md.section_a.1");
        assert_eq!(rules[1].id, "agents_md.section_b.1");
    }

    #[test]
    fn test_parse_agents_md_infers_must_as_required() {
        let md = "## R\n\n- You MUST do this.\n";
        let rules = parse_agents_md(md);
        assert_eq!(rules[0].enforcement, EnforcementLevel::Required);
    }

    #[test]
    fn test_parse_agents_md_infers_required_keyword_as_required() {
        let md = "## R\n\n- This is REQUIRED.\n";
        let rules = parse_agents_md(md);
        assert_eq!(rules[0].enforcement, EnforcementLevel::Required);
    }

    #[test]
    fn test_parse_agents_md_infers_may_as_optional() {
        let md = "## R\n\n- You MAY skip this.\n";
        let rules = parse_agents_md(md);
        assert_eq!(rules[0].enforcement, EnforcementLevel::Optional);
    }

    #[test]
    fn test_parse_agents_md_infers_optional_keyword_as_optional() {
        let md = "## R\n\n- This is OPTIONAL.\n";
        let rules = parse_agents_md(md);
        assert_eq!(rules[0].enforcement, EnforcementLevel::Optional);
    }

    #[test]
    fn test_parse_agents_md_defaults_to_recommended_without_keyword() {
        let md = "## R\n\n- Consider doing this.\n";
        let rules = parse_agents_md(md);
        assert_eq!(rules[0].enforcement, EnforcementLevel::Recommended);
    }

    #[test]
    fn test_parse_agents_md_mandatory_word_does_not_trigger_optional() {
        // "MANDATORY" must not match the whole word "MAY".
        let md = "## R\n\n- This is MANDATORY.\n";
        let rules = parse_agents_md(md);
        assert_eq!(rules[0].enforcement, EnforcementLevel::Recommended);
    }

    #[test]
    fn test_parse_agents_md_rule_source_is_repository_file() {
        let md = "## R\n\n- A rule.\n";
        let rules = parse_agents_md(md);
        assert!(matches!(rules[0].source, RuleSource::RepositoryFile { .. }));
    }

    #[test]
    fn test_parse_agents_md_numbered_list_items_are_extracted() {
        let md = "## R\n\n1. First rule.\n2. Second rule.\n";
        let rules = parse_agents_md(md);
        assert_eq!(rules.len(), 2);
    }

    #[test]
    fn test_parse_agents_md_inline_code_preserved_in_description() {
        let md = "## R\n\n- Use `.yaml` not `.yml`.\n";
        let rules = parse_agents_md(md);
        assert_eq!(rules.len(), 1);
        assert!(rules[0].description.contains("`.yaml`"));
        assert!(rules[0].description.contains("`.yml`"));
    }

    #[test]
    fn test_parse_agents_md_nested_list_items_not_included() {
        // The outer item becomes a rule; the nested item must not.
        let md = "## R\n\n- Outer rule.\n  - Nested item.\n";
        let rules = parse_agents_md(md);
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].description, "Outer rule.");
    }

    #[test]
    fn test_parse_agents_md_with_real_agents_md_returns_nonempty() {
        let content = include_str!("../../AGENTS.md");
        let rules = parse_agents_md(content);
        assert!(!rules.is_empty());
        for rule in &rules {
            assert!(!rule.id.is_empty(), "rule id must not be empty");
            assert!(
                !rule.description.is_empty(),
                "rule description must not be empty"
            );
        }
    }

    // --- heading_to_slug ---

    #[test]
    fn test_heading_to_slug_lowercases_text() {
        assert_eq!(heading_to_slug("Critical Rules"), "critical_rules");
    }

    #[test]
    fn test_heading_to_slug_replaces_special_chars_with_underscore() {
        assert_eq!(
            heading_to_slug("Rule 1: File Extensions"),
            "rule_1_file_extensions"
        );
    }

    #[test]
    fn test_heading_to_slug_trims_leading_and_trailing_underscores() {
        assert_eq!(heading_to_slug("  Spaces  "), "spaces");
    }

    #[test]
    fn test_heading_to_slug_collapses_consecutive_separators() {
        assert_eq!(heading_to_slug("A -- B"), "a_b");
    }

    // --- infer_enforcement ---

    #[test]
    fn test_infer_enforcement_must_returns_required() {
        assert_eq!(
            infer_enforcement("You MUST do this."),
            EnforcementLevel::Required
        );
    }

    #[test]
    fn test_infer_enforcement_required_keyword_returns_required() {
        assert_eq!(
            infer_enforcement("This is REQUIRED."),
            EnforcementLevel::Required
        );
    }

    #[test]
    fn test_infer_enforcement_may_returns_optional() {
        assert_eq!(
            infer_enforcement("You MAY skip this."),
            EnforcementLevel::Optional
        );
    }

    #[test]
    fn test_infer_enforcement_optional_keyword_returns_optional() {
        assert_eq!(
            infer_enforcement("This is OPTIONAL."),
            EnforcementLevel::Optional
        );
    }

    #[test]
    fn test_infer_enforcement_no_keyword_returns_recommended() {
        assert_eq!(
            infer_enforcement("Consider doing this."),
            EnforcementLevel::Recommended
        );
    }

    #[test]
    fn test_infer_enforcement_mandatory_not_may() {
        // "MANDATORY" contains "MAY" as a substring but not as a whole word.
        assert_eq!(
            infer_enforcement("This is MANDATORY."),
            EnforcementLevel::Recommended
        );
    }

    #[test]
    fn test_infer_enforcement_case_insensitive() {
        assert_eq!(
            infer_enforcement("you must do this."),
            EnforcementLevel::Required
        );
        assert_eq!(
            infer_enforcement("you may skip this."),
            EnforcementLevel::Optional
        );
    }
}
