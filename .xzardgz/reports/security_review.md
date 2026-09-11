# Security Review Report

Generated at: 2026-09-11T18:30:29Z

Repository: repo

Workspace: 01M28VRH6SJYXHYSQPE63J37FF

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
