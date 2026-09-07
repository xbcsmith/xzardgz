# Security Review Report

Generated at: 2026-09-07T21:33:15Z

Repository: repo

Workspace: 01M1YWM927SPCJ491YMHNHSWB0

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

