# External Data Clients Implementation

This document describes the implementation of the external data clients layer
across Phases 1, 3, and 4 of the
[external_data_clients_plan.md](external_data_clients_plan.md). It covers the
`src/clients/` module structure, the two supply-chain signal resolvers, the OSV
vulnerability client, and the CVSS scoring calculator.

## Overview

The `src/clients/` module provides a dedicated, non-AI external data fetching
layer for supply-chain signal resolution. It sits between the pipeline core and
the external world, fetching structured data from public APIs and local override
files so that analysis plugins receive real external signals rather than none.

- Phase 1 adds the `clients` module boundary, the OpenSSF Scorecard resolver,
  and the GitHub repository metadata resolver. Both are wired into
  `TechnicalReviewPlugin`.
- Phase 3 adds the OSV vulnerability client behind a `VulnerabilitySource`
  trait, wired into `SecurityReviewPlugin`.
- Phase 4 adds a CVSS v3.x and v4.0 scoring calculator that converts raw
  severity vectors into a normalised `OsvScore` and qualitative band, feeding
  the confidence-scoring pipeline.

Phase 2 (local Scorecard generation via direct GitHub API calls) is a stub only
and is not implemented. See the Phase 2 section below.

## Component Boundary Contract

`src/clients/` has an explicit, enforced component-boundary contract documented
in its module-level doc comment:

| Rule                    | Detail                          |
| ----------------------- | ------------------------------- |
| May depend on           | `auth`, `config`                |
| Must NOT call           | `scanner`, `providers`, `agent` |
| Must NOT be called from | `tools/`                        |

The boundary keeps the data-fetching layer a pure leaf in the dependency graph
and prevents circular imports. All credential resolution flows through the
`auth` module's `SecretStore` abstraction rather than direct `std::env::var`
calls.

## Phase 1: Scorecard and Repository Metadata

### Slug Parsing

`clients::parse_github_slug(repo: &str) -> Option<(String, String)>` converts a
repository identifier in any of the following formats into an `(owner, repo)`
pair:

| Format                              | Example                                 |
| ----------------------------------- | --------------------------------------- |
| `owner/repo`                        | `ossf/scorecard`                        |
| `github.com/owner/repo`             | `github.com/ossf/scorecard`             |
| `https://github.com/owner/repo`     | `https://github.com/ossf/scorecard`     |
| `https://github.com/owner/repo.git` | `https://github.com/ossf/scorecard.git` |
| `git@github.com:owner/repo.git`     | `git@github.com:ossf/scorecard.git`     |

Non-GitHub hosts, empty owner or repo segments, and unrecognised formats all
return `None`. The function does not depend on a URL-parser crate; host
detection is performed by examining the first path segment for a dot character
(e.g. `gitlab.com` is rejected) and by matching the SSH prefix exactly against
`git@github.com:`.

### Scorecard Resolver

`clients::scorecard::resolve_scorecard(repo, workspace_root)` implements a
fetch-then-local-file fallback chain:

1. Attempts to read `{workspace_root}/scorecard.json`. When the file is present
   it is parsed and returned immediately with no network call.
2. Calls `fetch_scorecard`, which queries the public OpenSSF Scorecard REST API
   at `https://api.securityscorecards.dev/projects/github.com/{owner}/{repo}`.
   No authentication is required.
3. Returns `ScorecardResolveError::AllSourcesExhausted` when both sources fail.

The internal `fetch_scorecard_from(repo, base_url)` function is `pub(crate)` and
accepts a configurable base URL, enabling wiremock tests without live network
access.

Note that the fallback order here is remote-preferred: the local file acts as a
manually-placed override, not a cache, so the remote result is used when no
local file is present. A corrupted local file surfaces an immediate parse error
rather than silently falling through to a network request.

### Repository Metadata Resolver

`clients::repodata::resolve_repodata(repo, workspace_root)` implements a
local-file-then-remote chain:

1. Attempts to read `{workspace_root}/repodata.json`. When the file is present
   it is parsed and returned immediately.
2. Calls the GitHub REST API at `GET /repos/{owner}/{repo}`. The `GITHUB_TOKEN`
   environment variable is resolved via `EnvVarStore::new("GITHUB_")` from the
   `auth` module. When the variable is absent, an unauthenticated request is
   made.
3. Returns `RepoDataResolveError::AllSourcesExhausted` when both sources fail.

### ExternalSignals

```rust
pub struct ExternalSignals {
    pub scorecard: Option<ScorecardResult>,
    pub repodata: Option<RepoMetadata>,
}
```

`ExternalSignals` aggregates the two resolved values and is threaded into
`TechnicalReviewPlugin`. When signals are present, `build_user_prompt` appends:

- The OpenSSF Scorecard numeric score and all checks scoring below 5.
- Repository flags (archived, fork) and topics from GitHub metadata.

`TechnicalReviewMarkdownReport::write_with_signals` appends a
`## Supply Chain Signals` section to the written report containing an
`### OpenSSF Scorecard` subsection and a `### Repository Metadata` subsection.

### TechnicalReviewConfig Fields

`TechnicalReviewConfig` gains two new boolean fields:

```rust
pub scorecard_enabled: bool,  // default: true
pub repodata_enabled: bool,   // default: true
```

Both are `serde(default = "default_true")` so existing configuration files
require no change.

### Graceful Degradation

Resolution failures are demoted to `Diagnostic::warning` entries by the plugin.
The plugin continues normally with `None` signals rather than failing the run.
This keeps the feature usable in air-gapped or network-restricted environments.

## Phase 3: OSV Vulnerability Client

### VulnerabilitySource Trait

```rust
pub trait VulnerabilitySource {
    async fn query(
        &self,
        dep: &VulnerabilityQuery,
    ) -> Result<Vec<VulnerabilityRecord>, VulnClientError>;
}
```

`VulnerabilitySource` is defined in `clients::vuln::mod` and is the interface
all vulnerability clients implement. `OsvClient` sits behind this trait so that
`SecurityReviewPlugin` depends on the trait, not the concrete HTTP client.

### OsvClient

`clients::vuln::osv::OsvClient` implements `VulnerabilitySource` against the
public `POST https://api.osv.dev/v1/query` endpoint. No API key is required.

The query implementation follows a three-level fallback algorithm:

1. PURL query -- when `dep.purl` is `Some`, send a PURL-based request. This is
   the preferred query shape because it encodes ecosystem and version precisely.
2. Name and ecosystem query -- when the PURL query returns an empty `vulns`
   array, or when no PURL is available, fall back to a query by
   `name + ecosystem` (with an optional `version` field when `dep.version` is
   known).
3. Commit hash query -- when `dep.commit` is `Some` and both prior queries
   returned empty results, send a commit-hash query as the final attempt.

An empty `vulns` array in the response is a valid, non-error result meaning no
known vulnerabilities were found for that query shape. It is distinct from an
HTTP error or a deserialization failure.

### Testdata Fixture

The canonical fixture for OSV deserialization and wiremock tests is
`testdata/osv.dev.results.json`. This file was renamed from
`testdata/osv.dev.resutls.json` (which had a typo in "resutls") as part of
Phase 3. It contains real OSV API response shapes including entries with both
CVSS_V3 and CVSS_V4 severity, entries with only CVSS_V4, and `PYSEC-*` entries
with no `severity` key at all.

### Response Shape

Each entry in `vulns[]` is deserialized into `VulnerabilityRecord` carrying:

- `id`, `summary`, `details`, `aliases`
- `database_specific.severity` (a coarse label string)
- `references[]`
- `affected[]`
- `severity[]` -- an array of
  `{ type: "CVSS_V3" | "CVSS_V4", score: <vector string> }`. This field is
  entirely absent from some entries (all `PYSEC-*` entries in the fixture);
  absence is treated as an empty array, not an error.

### SecurityReviewConfig Fields

`SecurityReviewConfig` gains:

```rust
pub osv_enabled: bool,  // default: true
```

The field defaults to `true` because the OSV endpoint requires no credentials.

### Integration

`SecurityReviewPlugin` creates an `OsvClient` instance when
`osv_enabled && dependency_scanning` are both true. When OSV signals are
present, a note is appended to the AI prompt context. Full per-package scanning
is gated on a future dependency manifest parser; the client itself is complete
and tested in isolation.

## Phase 4: CVSS Scoring

### score_severity

```rust
pub fn score_severity(severity: &[OsvSeverityEntry]) -> OsvScore
```

Defined in `clients::vuln::osv::scoring`, this function converts the raw
`severity[]` array from an OSV record into a normalised score.

### OsvScore

```rust
pub struct OsvScore {
    pub cvss_v3_score: Option<f64>,
    pub cvss_v4_score: Option<f64>,
    pub primary_score: f64,
    pub band: Option<CvssBand>,
}
```

`primary_score` is the single value used downstream in the confidence-scoring
pipeline. `cvss_v3_score` and `cvss_v4_score` are retained separately for audit
purposes.

### Scoring Rules

| Condition                                 | primary_score        | band                   |
| ----------------------------------------- | -------------------- | ---------------------- |
| `severity` is empty or absent             | `0.0`                | `None`                 |
| Both CVSS_V3 and CVSS_V4 present          | CVSS v3 score        | from CVSS v3           |
| Only CVSS_V4 present                      | CVSS v4 score        | from CVSS v4           |
| Only CVSS_V3 present                      | CVSS v3 score        | from CVSS v3           |
| Vector string is malformed or unparseable | `0.0` for that entry | does not fail the call |

When both CVSS v3 and CVSS v4 entries are present, CVSS v3 is always used as
`primary_score`. The v4 score is retained in `cvss_v4_score` for audit purposes
only and is never blended with the v3 value.

### CVSS v3.x Scoring

CVSS v3.x scoring uses the standard base score formula. The formula is identical
for both v3.0 and v3.1 vectors. The implementation branches on the vector
string's own version prefix (`CVSS:3.0/` vs `CVSS:3.1/`) rather than trusting
the `type` field alone, because OSV's `type: "CVSS_V3"` tag covers both v3.0 and
v3.1 vector strings (both appear in the fixture under the same type tag).

### CVSS v4.0 Scoring

CVSS v4.0 scoring uses the EQ-level based approach, operating on the exploit and
impact sub-dimensions extracted from the vector string.

### Band Thresholds

| Band     | Range          |
| -------- | -------------- |
| None     | `0.0`          |
| Low      | `0.1` - `3.9`  |
| Medium   | `4.0` - `6.9`  |
| High     | `7.0` - `8.9`  |
| Critical | `9.0` - `10.0` |

### Signal Mapping

```rust
pub fn osv_score_to_signal(score: &OsvScore) -> Option<ScoringSignal>
```

Maps the qualitative `band` to a `ScoringSignal::Negative` weight consistent
with the weights used by other deterministic `security_review` signals (pattern
registry hits, SAST findings). Returns `None` when `band` is `None` (score of
`0.0`).

## Phase 2 (Stub)

Phase 2 -- local Scorecard generation via direct GitHub API calls -- is not
implemented. It is a placeholder only. The plan documents it as a sketch
requiring a dedicated follow-up design review before any implementation begins.
The `resolve_scorecard` fallback chain has exactly two levels; no third level
for local generation is wired. Do not begin Phase 2 without the follow-up
planning pass described in the plan.

## Testing Strategy

### Phase 1 Tests

| Test group                    | Count | Coverage                                               |
| ----------------------------- | ----- | ------------------------------------------------------ |
| `clients::mod`                | 16    | All `parse_github_slug` formats and edge cases         |
| `clients::scorecard`          | 7     | Wiremock success/404, local file present, invalid repo |
| `clients::repodata`           | 6     | Wiremock success/404, local file present, invalid repo |
| `report::render_with_signals` | 4     | None/empty/scorecard/repodata cases                    |
| `report::write_with_signals`  | 2     | File creation with and without signals                 |
| `TechnicalReviewConfig`       | 2     | Default values for new fields                          |

Wiremock tests inject the mock server URI via the `_from(repo, base_url)` helper
variants, so no live network calls are made during `cargo test`.

Local file tests write a fixture JSON file to a `tempfile::tempdir` and confirm
that the resolver reads the file without making any HTTP request.

### Phase 3 Tests

Wiremock-backed tests using `testdata/osv.dev.results.json` as the response body
cover:

- A PURL query that returns results directly.
- A PURL query that returns an empty `vulns` array, triggering the
  name-plus-ecosystem fallback.
- All three query methods returning empty results (a valid "no vulnerabilities"
  result, not an error).

### Phase 4 Tests

Unit tests against every distinct `severity[]` shape present in the fixture:

| Fixture entry         | Shape tested                     |
| --------------------- | -------------------------------- |
| `GHSA-462w-v97r-4m45` | Both CVSS_V3 and CVSS_V4 present |
| `GHSA-cpwx-vrp4-4pq7` | Only CVSS_V4 present             |
| (constructed case)    | Only CVSS_V3 present             |
| `PYSEC-*` entries     | `severity` key entirely absent   |

Each test asserts that `primary_score` and `band` match hand-computed expected
values for at least one vector string of each type.

## Module Structure

```text
src/clients/
    mod.rs              - component boundary contract, ExternalSignals, parse_github_slug
    scorecard.rs        - Scorecard fetch and resolve
    repodata.rs         - GitHub repo metadata fetch and resolve
    vuln/
        mod.rs          - VulnerabilitySource trait, query/record types, VulnClientError
        osv/
            mod.rs      - OsvClient, OSV query implementation
            scoring.rs  - CVSS v3.x and v4.0 scoring, OsvScore, score_severity
```
