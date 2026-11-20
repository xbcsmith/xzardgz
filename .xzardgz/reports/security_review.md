# Security Review Report

Generated at: 2026-11-20T18:52:32Z

Repository: repo

Workspace: 01M7X4V70B27N9WWFQECS8JSQX

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

