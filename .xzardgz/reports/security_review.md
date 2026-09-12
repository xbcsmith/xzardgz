# Security Review Report

Generated at: 2026-09-12T22:16:41Z

Repository: repo

Workspace: 01M2BV3CXAF62K4R7K00MTK1VM

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

