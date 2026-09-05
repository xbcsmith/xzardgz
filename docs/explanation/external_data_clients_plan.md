# External Data Clients Implementation Plan

## Overview

xzardgz has no dedicated layer for non-AI external data sources: no
OpenSSF Scorecard integration, no GitHub repository metadata fetch, and no
`src/clients/` module at all. This plan adds that layer, starting with
Scorecard fetch-and-local-file resolution and a parallel GitHub repository
metadata resolution path, so `technical_review` (and, in future,
`security_review`) get real external supply-chain signal instead of none.
Local Scorecard *generation* (computing a Scorecard-shaped result directly
from the GitHub API when no published data exists) is included here only as
a stub to be iterated on in a follow-up review before any of it is
implemented — do not begin Phase 2 without that follow-up pass. This plan is
also the intended home for a later expansion adding built-in SAST and SCA
scanner client integrations; that expansion is not scoped in this document.

## Current State Analysis

### Existing Infrastructure

- `src/auth/store.rs::SecretStore` already provides the credential-resolution
  pattern (env var, then keychain) this layer will reuse for GitHub
  credentials.
- `wiremock` is already a test dependency, suitable for mocking the
  Scorecard and GitHub REST APIs in tests.
- `src/plugins/technical_review/` already models a dimension-based review
  structure that external Scorecard/repo-metadata data would plug into as
  additional input.

### Identified Issues

- No `src/clients/` module, or equivalent, exists.
- `technical_review` and `security_review` currently have no path to
  real OpenSSF Scorecard or GitHub repository metadata at all.

## Implementation Phases

### Phase 1: Client Boundary, Scorecard and Repo-Metadata Resolution

#### 1.1 Foundation Work

Create `src/clients/` with an explicit, documented component-boundary
contract: may depend on `auth` and `config`; must never be called from
`tools/`; must never call into `scanner`, `providers`, or `agent`.

#### 1.2 Add Foundation Functionality

Implement `clients::scorecard::fetch_scorecard(repo)` (public OpenSSF
Scorecard REST API via `reqwest`) and `clients::scorecard::resolve_scorecard(repo,
workspace_root)`, a fetch-then-local-file fallback chain (checking
`{workspace_root}/scorecard.json` as a manually-placed override before
giving up). Implement a parallel `clients::repodata::resolve_repodata(repo,
workspace_root)` (local file, then a direct GitHub REST API repository
metadata fetch) for the GitHub metadata `technical_review` currently lacks.
Local Scorecard *generation* is explicitly not implemented in this phase —
see Phase 2.

#### 1.3 Integrate Foundation Work

Wire both resolvers into `technical_review`'s configuration as optional
inputs. The client call happens through `WorkflowExecutor`'s existing
plugin-preparation/dispatch path (per the CLI-to-workflow-engine integration
plan's "executor owns everything" principle) — no ad hoc client calls from
CLI command code.

#### 1.4 Testing Requirements

Mocked HTTP tests (via `wiremock`) for the fetch step of both resolvers, and
a local-file-present test that bypasses the network call entirely for each.

#### 1.5 Deliverables

`clients::scorecard::{fetch_scorecard, resolve_scorecard}` and
`clients::repodata::resolve_repodata`, each with two working fallback
levels.

#### 1.6 Success Criteria

`technical_review` produces a materially different, populated report section
when Scorecard/repo-metadata resolution succeeds, and degrades gracefully
(typed resolve error, plugin continues rather than failing the whole run)
when every source fails.

### Phase 2 (STUB — requires a follow-up review before implementation): Local Scorecard Generation

#### 2.1 Feature Work (sketch only)

A `clients::scorecard::generate` module that computes a Scorecard-shaped
result locally via direct GitHub REST API calls, for repositories with no
published Scorecard data and no local override file.

#### 2.2 Integrate Feature (sketch only)

A third fallback level appended to `resolve_scorecard`'s chain. GitHub-only:
GitLab-hosted repositories with no local file present resolve immediately to
an `UnsupportedHost` error rather than attempting generation.

#### 2.3 Configuration Updates (sketch only)

None anticipated beyond the GitHub credential resolution already used
elsewhere in this plan.

#### 2.4 Testing Requirements (sketch only)

Not yet scoped — depends on which specific Scorecard checks are chosen for
local replication.

#### 2.5 Deliverables

None yet. This phase is a placeholder only, to be fleshed out in a follow-up
planning pass before any implementation work begins.

#### 2.6 Success Criteria

Not applicable until this phase is re-scoped.

## OSV Option for Vulnerabilities

`security_review`'s current "dependency_scanning" check category has no
real vulnerability-database integration behind it today. This section
scopes the first concrete SCA client: OSV.dev, queried via its public,
unauthenticated `POST https://api.osv.dev/v1/query` endpoint. A local
fixture already exists at `testdata/osv.dev.resutls.json` (note: the
on-disk filename has a typo — `resutls`, not `results` — fixing this is
folded into Phase 3 below) and was used to validate the response shapes and
severity variations referenced throughout this section.

### Phase 3: OSV Vulnerability Client

#### 3.1 Foundation Work

Rename `testdata/osv.dev.resutls.json` to `testdata/osv.dev.results.json`
and adopt it as the canonical fixture for deserialization and mocked-HTTP
tests. Define a small `VulnerabilitySource` trait
(`query(purl_or_package) -> Vec<VulnerabilityRecord>`) under a new
`src/clients/vuln/` module, so `OsvClient` sits behind a clean interface
rather than being called directly and inline from `security_review`. Before
finalizing the request builder, confirm the exact PURL request shape
(whether the version is embedded in the PURL string itself or supplied as a
sibling `version` field) against the linked OSV `post-v1-query`
documentation.

#### 3.2 Add Foundation Functionality

Implement `clients::vuln::osv::OsvClient`, posting to
`https://api.osv.dev/v1/query`. Support every query method OSV's endpoint
accepts, with PURL preferred and automatic fallback, per the confirmed
design: (1) build and send a PURL-based query when a PURL can be derived for
the dependency's ecosystem; (2) if that query returns an empty `vulns` array
— or no PURL can be derived, or OSV rejects the PURL type for that
ecosystem — fall back to a name-plus-ecosystem(-plus-version) query; (3) if
still empty and another supported query shape applies (e.g. a commit hash),
try that before concluding there are no results. Deserialize the response
shape confirmed against the local fixture: `vulns[]` entries carrying `id`,
`summary`, `details`, `aliases`, `database_specific.severity` (a coarse
label), `references[]`, `affected[]`, and `severity[]` (an array of
`{ type: "CVSS_V3" | "CVSS_V4", score: <vector string> }` — see Phase 4 for
how this is scored). Note from the fixture: `severity` is sometimes entirely
absent from a `vulns[]` entry (all `PYSEC-*` entries in the fixture have no
`severity` key at all), which Phase 4 must treat as "no score," not an
error.

#### 3.3 Integrate Foundation Work

Wire `OsvClient` behind the `VulnerabilitySource` trait and add a
`security_review.osv.enabled: bool` configuration field (default `true`,
since the endpoint requires no API key), following the same
configuration/CLI mechanism every other plugin-behavior toggle in this
codebase already uses.

#### 3.4 Testing Requirements

Mocked HTTP tests (via the existing `wiremock` dependency) using the
corrected fixture as the response body, covering: a PURL query that
succeeds directly; a PURL query that returns empty and triggers the
name-plus-ecosystem-plus-version fallback; and every method returning empty
(a real "no known vulnerabilities" result, not an error).

#### 3.5 Deliverables

`clients::vuln::{VulnerabilitySource, osv::OsvClient}`, wired into
`security_review` behind the new `osv.enabled` config flag.

#### 3.6 Success Criteria

Querying the fixture's own vulnerable package (`jinja2` at a vulnerable
version) via the OSV backend returns the same vulnerability set present in
the corrected fixture file.

### Phase 4: CVSS Scoring Calculator (cvss-rs)

#### 4.1 Foundation Work

Add the `cvss-rs` crate (https://github.com/scm-rs/cvss-rs) as a dependency
for parsing CVSS v3.1 and v4.0 vector strings into numeric base scores. Note
from the fixture: OSV's `severity[].type == "CVSS_V3"` covers both
`CVSS:3.0` and `CVSS:3.1` vector strings (both appear in the fixture under
the same `type` tag), so the parser must branch on the vector string's own
version prefix rather than trusting `type` alone to select a v3.0 versus
v3.1 parser.

#### 4.2 Add Foundation Functionality

Implement `clients::vuln::osv::scoring::score_severity(severity: &[OsvSeverity]) -> OsvScore`,
where `OsvScore` carries the individually-parsed `cvss_v3_score: Option<f64>`,
`cvss_v4_score: Option<f64>`, and one selected `primary_score: f64` plus a
qualitative band. Rules, per the confirmed design: if `severity` is empty or
the key is absent entirely, `primary_score = 0.0` and no band is assigned.
When both a `CVSS_V3` and a `CVSS_V4` entry are present, `primary_score` is
always the CVSS v3-derived base score — CVSS v3 is preferred for the
top-level LOW/MEDIUM/HIGH band regardless of v4's presence; the v4 score is
retained as a separate audit-only field, never blended with v3. When only
one CVSS version is present, `primary_score` is that version's base score.
A malformed or unparseable vector string degrades that single entry to
`0.0` rather than failing the whole query.

#### 4.3 Integrate Feature

Feed `OsvScore.primary_score` into the same `ScoringSignal`/`ConfidenceScorer`
pipeline established by the confidence scoring integration plan, mapping the
qualitative band to a `Negative` signal of corresponding weight, consistent
with how every other deterministic `security_review` signal (pattern-registry
hits, SAST findings) is weighted.

#### 4.4 Testing Requirements

Unit tests against every distinct `severity[]` shape actually present in
the fixture: both `CVSS_V3` and `CVSS_V4` present; only `CVSS_V4` present
(the fixture's `GHSA-cpwx-vrp4-4pq7` entry); only `CVSS_V3` present; and
`severity` entirely absent (the fixture's `PYSEC-*` entries). Assert
`primary_score` and its band match hand-computed expected values for at
least one vector string of each type.

#### 4.5 Deliverables

`clients::vuln::osv::scoring::score_severity`, covered by fixture-derived
tests for every `severity` shape observed in real OSV data.

#### 4.6 Success Criteria

A record with an empty or missing `severity` array always scores `0.0`. A
record with both CVSS versions present always reports its CVSS v3 base
score as `primary_score`, with the v4 score retained separately for audit
purposes only.

## Planned Expansion (Not Yet Scoped)

SCA vulnerability lookup is now scoped above via OSV (Phases 3-4). A
first-party SAST (static application security testing) tool is scoped
separately in
[`sast_scanning_tool_plan.md`](sast_scanning_tool_plan.md), since it is a
first-party analysis engine rather than an external data client and does
not fit this document's `src/clients/` boundary. Any further SCA source
beyond OSV remains unscoped and will be added to this document in a later
pass.
