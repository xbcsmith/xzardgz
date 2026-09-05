# Governance AGENTS.md Parsing Implementation Plan

## Overview

`GovernanceConfig::default().rules_path` points at `"AGENTS.md"`, but
`governance/loader.rs::load_for_config` expects that file to be a YAML
`RulesFile` (`overrides` + `additional_rules`), while xzardgz's own
`AGENTS.md` is prose contributor guidelines in Markdown. The mismatch is
currently masked because internal tests set `rules_path: String::new()` to
avoid exercising the real default. This plan resolves the inconsistency by
parsing the target repository's actual `AGENTS.md` as the source of
governance rules, and removes the YAML-sidecar schema entirely.

## Current State Analysis

### Existing Infrastructure

- `governance/rules.rs` defines `GovernanceRule { id, description,
  enforcement, source }`, `EnforcementLevel::{Required, Recommended,
  Optional}`, and `RuleSource`.
- `governance/loader.rs::embedded_defaults()` ships roughly ten hardcoded
  rules covering path traversal, plugin naming, event types, endpoint
  HTTPS enforcement, secret-pattern content, and branch naming.
- `governance/validator.rs` / `GovernanceChecker` already implement
  per-check methods (`check_branch`, `check_file_path`, `check_content_safety`,
  etc.) and a batch `check_workflow_inputs` — none of this changes; only the
  rule *source* changes.

### Identified Issues

- `governance/loader.rs::RulesFile`, `RuleOverride`, and `CustomRule` define
  a YAML schema that the shipped default `rules_path` value cannot possibly
  satisfy, since a real `AGENTS.md` is Markdown prose, not YAML.
  A YAML sidecar file is an unnecessary second file for every scanned
  repository to maintain; parsing the `AGENTS.md` that already exists in
  most repositories is the more useful default.
- Tests bypass this entirely via `rules_path: String::new()`
  (`src/workflow/executor.rs:1088`, `src/plugins/security_review/plugin.rs:551`),
  meaning the documented default has never actually been exercised
  end-to-end.

## Implementation Phases

### Phase 1: AGENTS.md Markdown Parser

#### 1.1 Foundation Work

Define the Markdown convention this parser recognizes: rules are list items
(numbered or bulleted) under recognized headings, with enforcement level
inferred from keywords in the item text — "MUST" / "Required" maps to
`Required`; "SHOULD" / "Recommended" maps to `Recommended`; "MAY" / "Optional"
maps to `Optional`; absence of a keyword defaults to `Recommended`.

#### 1.2 Add Foundation Functionality

Implement `governance::parser::parse_agents_md(content: &str) ->
Vec<GovernanceRule>` using a Markdown parsing crate (reuse one if already a
dependency; otherwise add a minimal one such as `pulldown-cmark`) to extract
list items under recognized headings and classify them per the Phase 1.1
convention.

#### 1.3 Integrate Foundation Work

Replace `load_for_config`'s YAML-merge logic entirely: read the repository's
`AGENTS.md` at `rules_path` (default remains `"AGENTS.md"`, now meaningfully
connected to real content) → parse via the new parser → merge with
`embedded_defaults()` by rule id (parsed rules take precedence; embedded
rules fill any gap). On a read or parse failure, fall back to
`embedded_defaults()` alone with a logged warning — never a hard failure.
Delete `RulesFile`, `RuleOverride`, and `CustomRule` and their YAML-merge
code path from `governance/loader.rs`.

#### 1.4 Testing Requirements

Parse this repository's own real `AGENTS.md` as a live fixture and assert a
non-empty, sane rule set results. Add a test asserting a missing or
unreadable `AGENTS.md` falls back to embedded defaults without error.

#### 1.5 Deliverables

A new `governance::parser` module; the YAML `RulesFile` schema and its
merge path removed from `governance/loader.rs`.

#### 1.6 Success Criteria

Running governance loading with the default configuration against this
repository succeeds and yields rules derived from the real, current
`AGENTS.md` — the exact regression path the tests currently dodge via
`rules_path: String::new()` is closed and covered by a real test.

### Phase 2: Derived/Enrichment Rules

#### 2.1 Feature Work

Add a `derive_from_context(language, frameworks)` enrichment step, appended
only when no repository `AGENTS.md` is found at all (the pure
embedded-defaults case) — for example, a starter set of Rust-specific rules
when the scanned repository's primary language is Rust.

#### 2.2 Integrate Feature

Call this enrichment from the same `load_for_config` path, tagging enriched
rules with `RuleSource::Derived` so their origin remains distinguishable from
both `RepositoryFile` and plain `EmbeddedDefaults` rules.

#### 2.3 Configuration Updates

None beyond the existing `GovernanceConfig`.

#### 2.4 Testing Requirements

Assert a repository with a real `AGENTS.md` does not receive derived
enrichment (parsed rules take precedence), and one without a repository
`AGENTS.md` receives embedded defaults plus derived enrichment.

#### 2.5 Deliverables

`derive_from_context` with at least one language's starter rule set.

#### 2.6 Success Criteria

Governance rule count and `RuleSource` are asserted correctly across both
the "has AGENTS.md" and "no AGENTS.md" scenarios in tests.
