//! Phase 9: Conformance Testing for the SAST engine.
//!
//! This integration test file validates that every built-in SAST rule produces
//! the results declared by its conformance fixture. It is structured in five
//! layers, each of which can run independently:
//!
//! | Module                   | What it tests                                              |
//! | ------------------------ | ---------------------------------------------------------- |
//! | `annotation_parsing_tests`  | `parse_annotations` unit contract                       |
//! | `conformance_check_tests`   | `check_conformance` unit contract                       |
//! | `baseline_tests`            | `ConformanceBaseline` load and regression detection      |
//! | `synthetic_conformance_tests` | End-to-end: compile synthetic YAML, run, assert result |
//! | `first_party_tests`         | All embedded rules vs. their checked-in fixtures         |
//! | `corpus_tests`              | External semgrep-rules corpus (feature-gated)            |

#[path = "helpers/sast_corpus.rs"]
mod sast_corpus;

// ---------------------------------------------------------------------------
// Annotation parsing unit tests
// ---------------------------------------------------------------------------

mod annotation_parsing_tests {
    use xzardgz::scanner::sast::conformance::{AnnotationKind, parse_annotations};

    #[test]
    fn test_parse_annotations_above_ruleid_target_is_next_line() {
        let src = "// ruleid: my-rule\nlet x = 1;\n";
        let annotations = parse_annotations(src);
        assert_eq!(annotations.len(), 1);
        assert_eq!(annotations[0].target_line, 2);
        assert!(
            matches!(&annotations[0].kind, AnnotationKind::RuleId(id) if id == "my-rule"),
            "expected RuleId('my-rule'), got {:?}",
            annotations[0].kind
        );
    }

    #[test]
    fn test_parse_annotations_inline_ruleid_target_is_same_line() {
        let src = "let x = 1; // ruleid: my-rule\n";
        let annotations = parse_annotations(src);
        assert_eq!(annotations.len(), 1);
        assert_eq!(annotations[0].target_line, 1);
        assert!(
            matches!(&annotations[0].kind, AnnotationKind::RuleId(id) if id == "my-rule"),
            "expected RuleId('my-rule')"
        );
    }

    #[test]
    fn test_parse_annotations_ok_annotation_recognized() {
        let src = "// ok: safe alternative\nlet y = 2;\n";
        let annotations = parse_annotations(src);
        assert_eq!(annotations.len(), 1);
        assert_eq!(annotations[0].target_line, 2);
        assert!(
            matches!(&annotations[0].kind, AnnotationKind::Ok),
            "expected Ok annotation"
        );
    }

    #[test]
    fn test_parse_annotations_todoruleid_recognized() {
        let src = "// todoruleid: my-rule\nsome_code();\n";
        let annotations = parse_annotations(src);
        assert_eq!(annotations.len(), 1);
        assert!(
            matches!(&annotations[0].kind, AnnotationKind::TodoRuleId(id) if id == "my-rule"),
            "expected TodoRuleId('my-rule')"
        );
    }

    #[test]
    fn test_parse_annotations_empty_source_returns_empty() {
        assert!(parse_annotations("").is_empty());
    }

    #[test]
    fn test_parse_annotations_no_annotations_returns_empty() {
        let src = "fn main() {\n    println!(\"hello\");\n}\n";
        assert!(parse_annotations(src).is_empty());
    }
}

// ---------------------------------------------------------------------------
// Conformance check logic tests
// ---------------------------------------------------------------------------

mod conformance_check_tests {
    use xzardgz::scanner::sast::conformance::{Annotation, AnnotationKind, check_conformance};

    fn make_rule_id_annotation(target_line: u32, rule_id: &str) -> Annotation {
        Annotation {
            annotation_line: target_line.saturating_sub(1).max(1),
            target_line,
            kind: AnnotationKind::RuleId(rule_id.to_string()),
        }
    }

    fn make_ok_annotation(target_line: u32) -> Annotation {
        Annotation {
            annotation_line: target_line.saturating_sub(1).max(1),
            target_line,
            kind: AnnotationKind::Ok,
        }
    }

    fn make_todo_annotation(target_line: u32, rule_id: &str) -> Annotation {
        Annotation {
            annotation_line: target_line.saturating_sub(1).max(1),
            target_line,
            kind: AnnotationKind::TodoRuleId(rule_id.to_string()),
        }
    }

    #[test]
    fn test_check_conformance_all_ruleid_matched_returns_pass() {
        let annotations = vec![make_rule_id_annotation(2, "my-rule")];
        let matched = vec![2];
        let result = check_conformance("my-rule", &annotations, &matched);
        assert!(result.is_pass(), "expected Pass, got {result:?}");
    }

    #[test]
    fn test_check_conformance_missed_match_returns_fail() {
        let annotations = vec![make_rule_id_annotation(2, "my-rule")];
        let matched = vec![]; // no match on line 2
        let result = check_conformance("my-rule", &annotations, &matched);
        assert!(result.is_fail(), "expected Fail, got {result:?}");
    }

    #[test]
    fn test_check_conformance_false_positive_returns_fail() {
        let annotations = vec![make_ok_annotation(3)];
        let matched = vec![3]; // unexpected match on the ok line
        let result = check_conformance("my-rule", &annotations, &matched);
        assert!(result.is_fail(), "expected Fail, got {result:?}");
    }

    #[test]
    fn test_check_conformance_todoruleid_annotations_are_skipped_not_fail() {
        let annotations = vec![make_todo_annotation(2, "my-rule")];
        let matched = vec![]; // no match, but it's a todo so we do not fail
        let result = check_conformance("my-rule", &annotations, &matched);
        assert!(result.is_skipped(), "expected Skipped, got {result:?}");
    }

    #[test]
    fn test_check_conformance_no_annotations_for_rule_returns_skipped() {
        let annotations = vec![make_rule_id_annotation(2, "other-rule")];
        let matched = vec![2];
        let result = check_conformance("my-rule", &annotations, &matched);
        assert!(result.is_skipped(), "expected Skipped, got {result:?}");
    }

    #[test]
    fn test_check_conformance_ok_with_no_match_returns_pass() {
        let annotations = vec![make_ok_annotation(2)];
        let matched = vec![]; // no match, which is correct for ok annotation
        let result = check_conformance("my-rule", &annotations, &matched);
        assert!(result.is_pass(), "expected Pass, got {result:?}");
    }
}

// ---------------------------------------------------------------------------
// Baseline load and regression detection tests
// ---------------------------------------------------------------------------

mod baseline_tests {
    use xzardgz::scanner::sast::conformance::{ConformanceBaseline, RuleConformanceResult};

    #[test]
    fn test_conformance_baseline_detect_regressions_finds_pass_to_fail() {
        let json = r#"{"version":"1","generated_at":"2026-01-01","entries":{"my-rule":"pass"}}"#;
        let baseline = ConformanceBaseline::from_json_str(json).unwrap();
        let current = vec![(
            "my-rule".to_string(),
            RuleConformanceResult::Fail {
                missed_lines: vec![5],
                false_positive_lines: vec![],
            },
        )];
        let regressions = baseline.detect_regressions(&current);
        assert!(
            !regressions.is_empty(),
            "should detect regression from pass to fail"
        );
        assert!(regressions.contains(&"my-rule".to_string()));
    }

    #[test]
    fn test_conformance_baseline_detect_regressions_ignores_pass_to_pass() {
        let json = r#"{"version":"1","generated_at":"2026-01-01","entries":{"my-rule":"pass"}}"#;
        let baseline = ConformanceBaseline::from_json_str(json).unwrap();
        let current = vec![("my-rule".to_string(), RuleConformanceResult::Pass)];
        let regressions = baseline.detect_regressions(&current);
        assert!(regressions.is_empty(), "pass to pass is not a regression");
    }

    #[test]
    fn test_conformance_baseline_detect_regressions_ignores_new_failures() {
        let json = r#"{"version":"1","generated_at":"2026-01-01","entries":{}}"#;
        let baseline = ConformanceBaseline::from_json_str(json).unwrap();
        // Rule not in baseline at all; a new failure is not a regression.
        let current = vec![(
            "new-rule".to_string(),
            RuleConformanceResult::Fail {
                missed_lines: vec![3],
                false_positive_lines: vec![],
            },
        )];
        let regressions = baseline.detect_regressions(&current);
        assert!(
            regressions.is_empty(),
            "new failure not in baseline is not a regression"
        );
    }

    #[test]
    fn test_conformance_baseline_from_json_str_valid_parses_successfully() {
        let json =
            r#"{"version":"1","generated_at":"2026-11-25","entries":{"rust-md5-usage":"pass"}}"#;
        let baseline = ConformanceBaseline::from_json_str(json).unwrap();
        assert_eq!(baseline.version, "1");
        assert_eq!(
            baseline.entries.get("rust-md5-usage"),
            Some(&"pass".to_string())
        );
    }

    #[test]
    fn test_conformance_baseline_from_json_str_invalid_returns_error() {
        let result = ConformanceBaseline::from_json_str("not valid json");
        assert!(result.is_err());
    }

    #[test]
    fn test_conformance_baseline_from_path_missing_file_returns_error() {
        let result =
            ConformanceBaseline::from_path(std::path::Path::new("/nonexistent/path/baseline.json"));
        assert!(result.is_err());
    }
}

// ---------------------------------------------------------------------------
// Synthetic rule end-to-end conformance tests
// ---------------------------------------------------------------------------

mod synthetic_conformance_tests {
    use xzardgz::scanner::sast::conformance::{
        ConformanceBaseline, RuleConformanceResult, run_rule_conformance,
    };
    use xzardgz::scanner::sast::rule::ir::CompileOutcome;
    use xzardgz::scanner::sast::rule::parse::parse_and_compile;

    const SYNTHETIC_PASS_RULE_YAML: &str =
        include_str!("../testdata/sast/conformance/synthetic_pass_rule.yaml");
    const SYNTHETIC_PASS_RULE_FIXTURE: &str =
        include_str!("../testdata/sast/conformance/synthetic_pass_rule.rs");

    const SYNTHETIC_FAIL_RULE_YAML: &str =
        include_str!("../testdata/sast/conformance/synthetic_fail_rule.yaml");
    const SYNTHETIC_FAIL_RULE_FIXTURE: &str =
        include_str!("../testdata/sast/conformance/synthetic_fail_rule.rs");

    const SYNTHETIC_TODO_RULE_YAML: &str =
        include_str!("../testdata/sast/conformance/synthetic_todo_rule.yaml");
    const SYNTHETIC_TODO_RULE_FIXTURE: &str =
        include_str!("../testdata/sast/conformance/synthetic_todo_rule.rs");

    fn compile_first_rule_from_yaml(yaml: &str) -> xzardgz::scanner::sast::rule::ir::RuleIr {
        let outcomes = parse_and_compile(yaml).expect("YAML must be valid");
        let mut iter = outcomes.into_iter();
        match iter.next() {
            Some(outcome) => match outcome.expect("rule must compile without error") {
                CompileOutcome::Compiled(ir) => ir,
                CompileOutcome::Skipped { rule_id, .. } => {
                    panic!("synthetic rule '{rule_id}' was unexpectedly skipped")
                }
            },
            None => panic!("no rules found in YAML"),
        }
    }

    #[test]
    fn test_synthetic_pass_rule_scores_pass() {
        let rule = compile_first_rule_from_yaml(SYNTHETIC_PASS_RULE_YAML);
        let result = run_rule_conformance(&rule, SYNTHETIC_PASS_RULE_FIXTURE);
        assert!(
            result.is_pass(),
            "synthetic pass rule should produce Pass, got {result:?}"
        );
    }

    #[test]
    fn test_synthetic_fail_rule_scores_fail() {
        let rule = compile_first_rule_from_yaml(SYNTHETIC_FAIL_RULE_YAML);
        let result = run_rule_conformance(&rule, SYNTHETIC_FAIL_RULE_FIXTURE);
        assert!(
            result.is_fail(),
            "synthetic fail rule should produce Fail, got {result:?}"
        );
    }

    #[test]
    fn test_synthetic_todo_rule_scores_skipped_not_fail() {
        let rule = compile_first_rule_from_yaml(SYNTHETIC_TODO_RULE_YAML);
        let result = run_rule_conformance(&rule, SYNTHETIC_TODO_RULE_FIXTURE);
        assert!(
            !result.is_fail(),
            "todoruleid annotation must not cause a failure; got {result:?}"
        );
        assert!(
            result.is_skipped(),
            "fixture with only todoruleid annotations should produce Skipped; got {result:?}"
        );
    }

    #[test]
    fn test_baseline_drift_detected_when_pass_becomes_fail() {
        // Artificially create a baseline with synthetic-pass-rule as "pass".
        let json = concat!(
            r#"{"version":"1","generated_at":"2026-11-25","entries":{"#,
            r#""synthetic-pass-rule":"pass","#,
            r#""synthetic-fail-rule":"fail","#,
            r#""synthetic-todo-rule":"skipped(no annotations for rule 'synthetic-todo-rule')"}}"#
        );
        let baseline = ConformanceBaseline::from_json_str(json)
            .expect("synthetic baseline JSON must be valid");

        // Artificially make the pass rule "fail" (simulating a regression).
        let current = vec![(
            "synthetic-pass-rule".to_string(),
            RuleConformanceResult::Fail {
                missed_lines: vec![2],
                false_positive_lines: vec![],
            },
        )];

        let regressions = baseline.detect_regressions(&current);
        assert!(
            !regressions.is_empty(),
            "a rule that was 'pass' in the baseline but is now 'fail' must be detected as a regression"
        );
        assert!(
            regressions.contains(&"synthetic-pass-rule".to_string()),
            "synthetic-pass-rule should appear in the regression list"
        );
    }
}

// ---------------------------------------------------------------------------
// First-party (embedded) rules conformance tests
// ---------------------------------------------------------------------------

mod first_party_tests {
    use std::path::PathBuf;

    use xzardgz::scanner::sast::conformance::{
        ConformanceBaseline, RuleConformanceResult, run_rule_conformance,
    };
    use xzardgz::scanner::sast::embedded::embedded_rules;
    use xzardgz::scanner::sast::rule::ir::CompileOutcome;
    use xzardgz::scanner::sast::rule::parse::parse_and_compile;

    use super::sast_corpus;

    fn conformance_fixture_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("testdata")
            .join("sast")
            .join("conformance")
    }

    fn baseline_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("testdata")
            .join("sast")
            .join("conformance_baseline.json")
    }

    #[test]
    fn test_first_party_rules_with_fixtures_all_pass() {
        let fixture_dir = conformance_fixture_dir();
        let rules = embedded_rules();
        let mut failures: Vec<String> = Vec::new();

        for yaml in rules.values() {
            let outcomes = match parse_and_compile(yaml) {
                Ok(o) => o,
                Err(e) => {
                    eprintln!("YAML parse error (should not happen for embedded rules): {e}");
                    continue;
                }
            };

            for outcome in outcomes {
                match outcome {
                    Ok(CompileOutcome::Compiled(rule)) => {
                        let fixture_path = fixture_dir.join(format!("{}.rs", rule.id));
                        if fixture_path.exists() {
                            let src = std::fs::read_to_string(&fixture_path).unwrap_or_else(|e| {
                                panic!("failed to read fixture {}: {e}", fixture_path.display())
                            });
                            let result = run_rule_conformance(&rule, &src);
                            if result.is_fail() {
                                failures.push(format!(
                                    "rule '{}' failed conformance: {result:?}",
                                    rule.id
                                ));
                            }
                        }
                    }
                    Ok(CompileOutcome::Skipped { rule_id, .. }) => {
                        eprintln!("[skip] embedded rule '{rule_id}' was not compiled");
                    }
                    Err(e) => {
                        eprintln!("compile error for embedded rule: {e}");
                    }
                }
            }
        }

        assert!(
            failures.is_empty(),
            "first-party conformance failures:\n{}",
            failures.join("\n")
        );
    }

    #[test]
    fn test_first_party_baseline_has_no_regressions() {
        let fixture_dir = conformance_fixture_dir();
        let baseline_path = baseline_path();

        let baseline = ConformanceBaseline::from_path(&baseline_path)
            .expect("conformance_baseline.json must be readable and valid JSON");

        let rules = embedded_rules();
        let mut current: Vec<(String, RuleConformanceResult)> = Vec::new();

        for yaml in rules.values() {
            let outcomes = match parse_and_compile(yaml) {
                Ok(o) => o,
                Err(_) => continue,
            };

            for outcome in outcomes {
                match outcome {
                    Ok(CompileOutcome::Compiled(rule)) => {
                        let fixture_path = fixture_dir.join(format!("{}.rs", rule.id));
                        let result = if fixture_path.exists() {
                            let src = std::fs::read_to_string(&fixture_path).unwrap_or_else(|e| {
                                panic!("failed to read fixture {}: {e}", fixture_path.display())
                            });
                            run_rule_conformance(&rule, &src)
                        } else {
                            RuleConformanceResult::Skipped("no fixture".to_string())
                        };
                        current.push((rule.id, result));
                    }
                    Ok(CompileOutcome::Skipped { rule_id, .. }) => {
                        current.push((
                            rule_id,
                            RuleConformanceResult::Skipped("compile-skipped".to_string()),
                        ));
                    }
                    Err(_) => {}
                }
            }
        }

        let regressions = baseline.detect_regressions(&current);
        assert!(
            regressions.is_empty(),
            "baseline regressions detected (rule was 'pass' in baseline but is now 'fail'):\n{}",
            regressions.join("\n")
        );
    }

    /// Verify the corpus helper returns `None` and does not panic when
    /// `XZARDGZ_SEMGREP_RULES_DIR` is unset, allowing corpus tests to skip
    /// gracefully without failing the suite.
    #[test]
    fn test_corpus_helper_prints_skip_message_when_env_var_unset() {
        let result =
            temp_env::with_var_unset("XZARDGZ_SEMGREP_RULES_DIR", sast_corpus::semgrep_rules_dir);
        assert!(
            result.is_none(),
            "corpus helper must return None when env var is unset, allowing test to skip gracefully"
        );
    }
}

// ---------------------------------------------------------------------------
// External corpus integration tests (feature-gated)
// ---------------------------------------------------------------------------

#[cfg(feature = "sast-integration-tests")]
mod corpus_tests {
    use super::sast_corpus;
    use std::path::Path;
    use xzardgz::scanner::sast::conformance::{
        RuleConformanceResult, parse_annotations, run_rule_conformance,
    };
    use xzardgz::scanner::sast::rule::ir::CompileOutcome;
    use xzardgz::scanner::sast::rule::parse::parse_and_compile;

    // Languages supported by the v1 engine.
    const V1_LANGUAGES: &[&str] = &["rust", "regex", "generic"];

    fn fixture_extensions() -> &'static [&'static str] {
        &[
            "rs", "go", "py", "js", "ts", "java", "c", "cpp", "txt", "rb",
        ]
    }

    fn find_sibling_fixture(rule_yaml_path: &Path) -> Option<std::path::PathBuf> {
        let stem = rule_yaml_path.file_stem()?.to_str()?;
        let dir = rule_yaml_path.parent()?;
        for ext in fixture_extensions() {
            let candidate = dir.join(format!("{stem}.{ext}"));
            if candidate.exists() {
                return Some(candidate);
            }
        }
        None
    }

    fn fixture_has_annotations(src: &str) -> bool {
        let annotations = parse_annotations(src);
        !annotations.is_empty()
    }

    /// Assert that at least 90% of v1-language corpus rules with sibling
    /// fixtures pass conformance. Rules whose fixture has no annotations are
    /// skipped (not counted toward the bar).
    ///
    /// The test exits gracefully (without failure) when
    /// `XZARDGZ_SEMGREP_RULES_DIR` is unset or points at a non-directory.
    #[test]
    fn test_external_corpus_at_least_90_percent_of_v1_rules_pass() {
        let corpus_dir = match sast_corpus::semgrep_rules_dir() {
            Some(d) => d,
            None => return, // corpus not available, skip gracefully
        };

        let mut total = 0usize;
        let mut passed = 0usize;
        let mut failed_rules: Vec<String> = Vec::new();

        for entry in walkdir::WalkDir::new(&corpus_dir)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().map(|x| x == "yaml").unwrap_or(false))
        {
            let rule_path = entry.path();
            let yaml = match std::fs::read_to_string(rule_path) {
                Ok(s) => s,
                Err(_) => continue,
            };

            let outcomes = match parse_and_compile(&yaml) {
                Ok(o) => o,
                Err(_) => continue,
            };

            for outcome in outcomes {
                let rule = match outcome {
                    Ok(CompileOutcome::Compiled(r)) => r,
                    Ok(CompileOutcome::Skipped { .. }) => continue,
                    Err(_) => continue,
                };

                // Only process v1 languages.
                let has_v1_lang = rule
                    .languages
                    .iter()
                    .any(|l| V1_LANGUAGES.iter().any(|v| l.eq_ignore_ascii_case(v)));
                if !has_v1_lang {
                    continue;
                }

                // Find sibling fixture file.
                let fixture_path = match find_sibling_fixture(rule_path) {
                    Some(p) => p,
                    None => continue,
                };

                let fixture_src = match std::fs::read_to_string(&fixture_path) {
                    Ok(s) => s,
                    Err(_) => continue,
                };

                // Only process fixtures with at least one annotation.
                if !fixture_has_annotations(&fixture_src) {
                    continue;
                }

                total += 1;
                let result = run_rule_conformance(&rule, &fixture_src);
                match result {
                    RuleConformanceResult::Pass => {
                        passed += 1;
                    }
                    RuleConformanceResult::Fail { .. } => {
                        failed_rules.push(rule.id.clone());
                    }
                    RuleConformanceResult::Skipped(_) => {
                        // Skipped rules do not count toward the 90% bar.
                    }
                }
            }
        }

        if total == 0 {
            println!(
                "SKIP: no v1 rules with annotated fixtures found in corpus; skipping 90% bar check"
            );
            return;
        }

        let pass_rate = (passed as f64) / (total as f64) * 100.0;
        println!("Corpus conformance: {passed}/{total} rules pass ({pass_rate:.1}%)");
        if !failed_rules.is_empty() {
            println!("Failed rules (first 20):");
            for r in failed_rules.iter().take(20) {
                println!("  - {r}");
            }
        }

        assert!(
            pass_rate >= 90.0,
            "corpus conformance bar not met: {pass_rate:.1}% < 90.0% ({passed}/{total} rules passed)"
        );
    }
}
