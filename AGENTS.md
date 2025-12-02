# AGENTS.md - AI Agent Development Guidelines

**CRITICAL**: This file contains mandatory rules for AI agents working on XZardgz.
Non-compliance will result in rejected code.

---

## 1. Identity & Purpose

- **Name**: XZardgz
- **Purpose**: Autonomous AI agent CLI for repository documentation generation.
- **Stack**: Rust (stable), Tokio, Clap, GitHub Copilot/Ollama.

---

## 2. Critical Rules (The "Golden Rules")

**VIOLATION OF THESE RULES = IMMEDIATE REJECTION**

### Rule 1: File Extensions
- **MUST** use `.yaml` (NOT `.yml`)
- **MUST** use `.md` (NOT `.MD` or `.markdown`)
- **MUST** use `.rs` for Rust code

### Rule 2: Filenames
- **MUST** use `lowercase_with_underscores` for ALL files (docs, code, config).
- **EXCEPTION**: `README.md` is the ONLY uppercase filename allowed.
- **EXAMPLE**: `docs/explanation/implementation_plan.md` (✅), `docs/explanation/ImplementationPlan.md` (❌)

### Rule 3: No Emojis
- **NEVER** use emojis in code, documentation, or commit messages.
- **REASON**: Encoding issues and professional standards.

### Rule 4: Code Quality Gates
**ALL** of the following must pass before you claim a task is done:
1. `cargo fmt --all` (Formatting)
2. `cargo check --all-targets --all-features` (Compilation)
3. `cargo clippy --all-targets --all-features -- -D warnings` (Linting - ZERO warnings allowed)
4. `cargo test --all-features` (Testing - >80% coverage)

### Rule 5: Dependency Management
- **MUST** use `cargo add <crate>` to add dependencies.
- **NEVER** edit `Cargo.toml` manually to add dependencies.
- **REASON**: Ensures compatible versions are selected.

---

## 3. Development Workflow

Follow this exact sequence for every task:

1.  **Implement**: Write code with `///` doc comments.
2.  **Test**: Add unit tests covering success, failure, and edge cases.
3.  **Verify**: Run the 4 Quality Gates (fmt, check, clippy, test).
4.  **Document**: Create/Update `docs/explanation/{feature}_implementation.md`.

---

## 4. Documentation Standards (Diataxis)

We follow the [Diataxis Framework](https://diataxis.fr/).

| Category | Path | Purpose | Example |
| :--- | :--- | :--- | :--- |
| **Tutorials** | `docs/tutorials/` | Learning-oriented, step-by-step | `getting_started.md` |
| **How-To** | `docs/how_to/` | Task-oriented, specific goals | `configure_provider.md` |
| **Explanation** | `docs/explanation/` | Understanding-oriented, design/implementation | `architecture.md` |
| **Reference** | `docs/reference/` | Information-oriented, specs | `api_spec.md` |

**Implementation Summaries**:
Always create a summary in `docs/explanation/` for your work.
- **Filename**: `{feature}_implementation.md`
- **Content**: Overview, Components, Implementation Details, Testing Results.

---

## 5. Rust Coding Standards

### Error Handling
- Use `Result<T, E>` for recoverable errors.
- Use `thiserror` for custom error enums.
- **NEVER** use `unwrap()` or `expect()` without a `// SAFETY:` comment explaining why it cannot fail.

### Testing
- **Coverage**: >80% required.
- **Structure**:
  ```rust
  #[cfg(test)]
  mod tests {
      use super::*;

      #[test]
      fn test_success_case() { ... }

      #[test]
      fn test_failure_case() { ... }
  }
  ```

---

## 6. Quick Reference

```bash
# Quality Check Loop
cargo fmt --all
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```
