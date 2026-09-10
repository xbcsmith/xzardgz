# Security Review Report

Generated at: 2026-09-10T18:29:49Z

Repository: repo

Workspace: 01M269AJ5XAKEGWBBHR55TW36E

Risk band: HIGH

## Summary

Primary language: Rust

Frameworks: None detected

Total findings: 1

Findings by severity:

- HIGH: 1

## Findings by Category

### secrets

| Severity | CWE | OWASP | File | Symbol | Evidence | Impact | Remediation |
|----------|-----|-------|------|--------|----------|--------|-------------|
| HIGH | CWE-798 | A07:2021 | src/main.rs:10 |  | Possible credential reference | data exposure | use env vars |

## Confidence

Average confidence: 0.90
