# Security Review Report

Generated at: 2026-09-07T16:27:11Z

Repository: repo

Workspace: 01M1YB3VVM1WK3FGP6FA2243DT

Risk band: CRITICAL

## Summary

Primary language: Rust

Frameworks: None detected

Total findings: 1

Findings by severity:

- CRITICAL: 1

## Findings by Category

### secrets

| Severity | CWE | OWASP | File | Symbol | Evidence | Impact | Remediation |
|----------|-----|-------|------|--------|----------|--------|-------------|
| CRITICAL | CWE-798 | A07:2021 | src/main.rs:10 |  | Possible credential reference | data exposure | use env vars |

## Confidence

Average confidence: 0.90

