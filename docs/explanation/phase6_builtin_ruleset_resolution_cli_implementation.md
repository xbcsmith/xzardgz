# Phase 6: Builtin Ruleset Resolution and CLI Implementation

## Overview

Phase 6 delivers the builtin rule catalogue, the four-level ruleset resolution
path in `resolve.rs`, and the `xzardgz sast scan` CLI subcommand. It also wires
two new top-level configuration sections (`sast` and `sast_scan`) into `Config`
and lays down the fixture pairs that drive rule-level regression tests.

## Deliverables

| Deliverable                      | Location                                                                   |
| -------------------------------- | -------------------------------------------------------------------------- |
| `sast` field on `Config`         | `src/config.rs`                                                            |
| `sast_scan` field on `Config`    | `src/config.rs`                                                            |
| `config.example.yaml` additions  | `config.example.yaml`                                                      |
| Builtin fixture pairs (34 files) | `testdata/sast/builtin/`                                                   |
| Phase 6 documentation            | `docs/explanation/phase6_builtin_ruleset_resolution_cli_implementation.md` |

## Builtin Rule Catalogue

The builtin catalogue ships 17 rules split across two logical rulesets.

### crypto ruleset (`builtin:crypto`) - 10 rules

| Rule ID                          | Match Strategy | Description                                                          |
| -------------------------------- | -------------- | -------------------------------------------------------------------- |
| `crypto/rust_triple_des_cipher`  | AST            | Detects `TdesEde2::new_from_slice` or `TdesEde3::new_from_slice`     |
| `crypto/rust_blowfish_cipher`    | AST            | Detects `Blowfish::new_from_slice`                                   |
| `crypto/rust_weak_prng`          | AST            | Detects `SmallRng::seed_from_u64` or `SmallRng::from_entropy`        |
| `crypto/rust_hardcoded_iv`       | Regex          | Matches `let iv = [` or `let nonce = [`                              |
| `crypto/rust_hardcoded_salt`     | Regex          | Matches `let salt = [` or `let password_salt = [`                    |
| `crypto/rust_aes_ecb_mode`       | AST            | Detects `ecb::Encryptor::<AesN>::new`                                |
| `crypto/rust_pkcs1v15_encrypt`   | AST            | Detects use of `Pkcs1v15Encrypt`                                     |
| `crypto/rust_pbkdf2_low_rounds`  | AST            | Detects `pbkdf2_hmac` calls with rounds < 100 000                    |
| `crypto/rust_sha1_hmac`          | AST            | Detects `Hmac::<Sha1>::new_from_slice` or `HmacSha1::new_from_slice` |
| `crypto/rust_crc32_for_security` | Regex          | Matches `crc32fast::hash(` or `crc32fast::checksum(`                 |

### security ruleset (`builtin:security`) - 7 rules

| Rule ID                                   | Match Strategy | Description                                            |
| ----------------------------------------- | -------------- | ------------------------------------------------------ |
| `security/rust_reqwest_invalid_certs`     | AST            | Detects `danger_accept_invalid_certs(true)`            |
| `security/rust_reqwest_invalid_hostnames` | AST            | Detects `danger_accept_invalid_hostnames(true)`        |
| `security/regex_hardcoded_db_url`         | Regex          | Matches `postgres://user:pass@host` patterns           |
| `security/regex_hardcoded_const_secret`   | Regex          | Matches `const API_KEY: &str = "..."` patterns         |
| `security/rust_unsafe_transmute`          | AST            | Detects `std::mem::transmute`                          |
| `security/rust_shell_command`             | AST            | Detects `Command::new("sh")` or `Command::new("bash")` |
| `security/rust_openssl_no_verify`         | AST            | Detects `ctx.set_verify(SslVerifyMode::NONE)`          |

## Rule ID Namespacing

Rule identifiers follow the format `<category>/<rule_name>`:

- `category` is one of `crypto` or `security`.
- `rule_name` is a snake-case description prefixed with either `rust_`
  (AST-based) or `regex_` (regex-based).

This convention makes the match strategy visible in the identifier without
inspecting the rule file and mirrors the directory layout under
`testdata/sast/builtin/`.

## Four-Level Search Path in resolve.rs

When a ruleset identifier like `builtin:crypto` is encountered, `resolve.rs`
applies the following lookup sequence, stopping at the first hit:

1. **CLI override directory** - path supplied by `--rules-dir` on the command
   line.
2. **Project-local directory** - `.xzardgz/sast/rules/` relative to the
   workspace root.
3. **XDG config directory** - `$XDG_CONFIG_HOME/xzardgz/sast/rules/` (or
   `~/.config/xzardgz/sast/rules/` on platforms without XDG support).
4. **Embedded fallback** - rules compiled into the binary via `include_str!` in
   `src/scanner/sast/embedded.rs`.

Level 4 guarantees that builtin rulesets are always available even in
environments with no writable filesystem paths.

## `xzardgz sast scan` CLI Subcommand

```text
xzardgz sast scan [OPTIONS] [PATH]
```

### Arguments

| Argument | Default | Description                     |
| -------- | ------- | ------------------------------- |
| `PATH`   | `.`     | Root directory or file to scan. |

### Options

| Flag                       | Config key                     | Description                                  |
| -------------------------- | ------------------------------ | -------------------------------------------- |
| `--ruleset <ID>`           | `sast_scan.rulesets`           | Override rulesets (repeatable).              |
| `--severity <LEVEL>`       | `sast_scan.severity_threshold` | Minimum severity to report.                  |
| `--max-findings <N>`       | `sast_scan.max_findings`       | Cap on total findings returned.              |
| `--format <FORMAT>`        | `sast_scan.report_formats`     | Output format: `json`, `sarif`, `cyclonedx`. |
| `--output <FILE>`          | -                              | Write report to file instead of stdout.      |
| `--rules-dir <DIR>`        | -                              | Prepend a directory to the search path.      |
| `--jobs <N>`               | `sast.jobs`                    | Parallel worker count (0 = auto-detect).     |
| `--max-file-bytes <BYTES>` | `sast.max_file_bytes`          | Skip files larger than this value.           |
| `--config <FILE>`          | -                              | Load a non-default config file.              |

### Exit Codes

| Code | Meaning                                                     |
| ---- | ----------------------------------------------------------- |
| 0    | Scan completed; no findings at or above the threshold.      |
| 1    | Scan completed; one or more findings at or above threshold. |
| 2    | Scan aborted due to configuration or I/O error.             |

## Configuration Fields

### `sast` section (`SastEngineConfig`)

Controls the low-level scanning engine. Corresponds to
`src/scanner/sast/config.rs`.

| Field                  | Type  | Default   | Description                                  |
| ---------------------- | ----- | --------- | -------------------------------------------- |
| `max_file_bytes`       | u64   | 5 242 880 | Files larger than this are skipped.          |
| `rule_timeout_ms`      | u64   | 5 000     | Per-rule evaluation timeout in milliseconds. |
| `max_matches_per_file` | usize | 100       | Maximum matches per (file, rule) pair.       |
| `jobs`                 | usize | 0         | Worker threads; 0 means use available CPUs.  |

### `sast_scan` section (`SastScanConfig`)

Controls the scan pipeline and report generation. Corresponds to
`src/plugins/sast_scan/config.rs`.

| Field                | Type        | Default                                 | Description                             |
| -------------------- | ----------- | --------------------------------------- | --------------------------------------- |
| `enabled`            | bool        | `true`                                  | Disables the pipeline when `false`.     |
| `rulesets`           | Vec<String> | `["builtin:crypto","builtin:security"]` | Ordered list of ruleset identifiers.    |
| `severity_threshold` | String      | `"warning"`                             | Minimum severity to include in output.  |
| `max_findings`       | usize       | 500                                     | Total findings cap across all rulesets. |
| `report_formats`     | Vec<String> | `["json"]`                              | Output formats produced per scan.       |

Both sections carry `#[serde(default)]` on the `Config` struct, so they are
completely optional in `config.yaml`.

## Testing Strategy

### Fixture Pairs

Each of the 17 rules has two companion files under `testdata/sast/builtin/`:

- `<rule_slug>_positive.rs` - code the rule must match.
- `<rule_slug>_negative.rs` - code the rule must not match.

Rule tests load each fixture, run the engine against it with the target rule
enabled and all others disabled, and then assert:

- Positive: `findings.len() >= 1`
- Negative: `findings.is_empty()`

This prevents both false negatives (rule misses a real issue) and false
positives (rule fires on safe code).

### Search-Path Precedence Tests

A dedicated integration test covers all four resolution levels:

1. Create a temporary directory with a modified `crypto` ruleset.
2. Pass it as `--rules-dir` and verify the modified ruleset is loaded instead of
   the embedded one.
3. Place the same directory under `.xzardgz/sast/rules/` and verify level 2
   takes precedence over level 4.
4. Clear the workspace-local directory and verify the embedded fallback is used.

### Config Field Tests in `src/config.rs`

Two tests validate the new `Config` fields:

- `test_config_sast_field_has_correct_default_max_file_bytes` - asserts that
  `Config::default().sast.max_file_bytes == 5_242_880`.
- `test_config_sast_scan_field_has_correct_default_rulesets` - asserts that
  `Config::default().sast_scan.rulesets` contains both `"builtin:crypto"` and
  `"builtin:security"`.
