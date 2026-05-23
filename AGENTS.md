# AGENTS.md - AI Agent Development Guidelines

**CRITICAL**: Mandatory rules for AI agents working on XZatoma. Non-compliance will result in rejected code.

---

## Critical Rules

### Rule 1: File Extensions

- Use `.yaml` for ALL YAML files (NOT `.yml`)
- Use `.md` for ALL Markdown files (NOT `.MD`, `.markdown`)
- Use `.rs` for ALL Rust files

CI/CD pipelines expect `.yaml`. Using `.yml` causes build failures.

### Rule 2: Markdown File Naming

- Use `lowercase_with_underscores.md` for all documentation files
- `README.md` is the ONLY exception to the lowercase rule
- Never use CamelCase, kebab-case, spaces, or uppercase

Inconsistent naming breaks documentation links.

### Rule 3: No Emojis

- No emojis in code, comments, documentation, or commit messages
- Exception: This AGENTS.md file only

Emojis cause encoding issues and break tooling.

### Rule 4: Quality Gates (ALL Must Pass)

Run in this order before claiming any task complete:

```bash
cargo fmt --all
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

**MANDATORY**: All Markdown files must pass linting and formatting checks:

```bash
markdownlint --fix --config .markdownlint.json "${FILE}"
prettier --write --parser markdown --prose-wrap always "${FILE}"
```

Stop immediately and fix if any command fails.

### Rule 5: Documentation is Mandatory

- Create `docs/explanation/<feature_name>_implementation.md` for every feature or task
- Add `///` doc comments to every public function, struct, enum, and module
- Include runnable examples in doc comments (they are compiled by `cargo test`)
- Never skip documentation because "code is self-documenting"

### Rule 6: Use the Agent Harness Tools

Do not write custom scripts for tasks that can be accomplished with the agent tools.

---

## Rust Coding Standards

### Error Handling

- Use `Result<T, E>` for all recoverable errors
- Use `?` for error propagation
- Use `thiserror` for custom error types
- Never use `unwrap()` or `expect()` without a justification comment
- Never ignore errors with `let _ =`
- Never use `panic!` for recoverable errors

```rust
// Correct pattern
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("Failed to read config file: {0}")]
    ReadError(String),
    #[error("Invalid YAML syntax: {0}")]
    ParseError(String),
}

pub fn load_config(path: &str) -> Result<Config, ConfigError> {
    let contents = std::fs::read_to_string(path)
        .map_err(|e| ConfigError::ReadError(e.to_string()))?;
    let config: Config = serde_yaml::from_str(&contents)
        .map_err(|e| ConfigError::ParseError(e.to_string()))?;
    config.validate()?;
    Ok(config)
}

// Acceptable: unwrap with explicit justification
pub fn get_app_version() -> String {
    // SAFETY: Set at compile time, cannot fail
    env!("CARGO_PKG_VERSION").to_string()
}
```

### Doc Comments

Every public function, struct, enum, and module must have a `///` doc comment:

````rust
/// One-line description.
///
/// Longer explanation of behavior and purpose.
///
/// # Arguments
///
/// * `param` - Description
///
/// # Returns
///
/// Description of return value
///
/// # Errors
///
/// Returns `ErrorType` if condition
///
/// # Examples
///
/// ```
/// use xzatoma::module::function;
///
/// let result = function(arg);
/// assert_eq!(result, expected);
/// ```
pub fn function(param: Type) -> Result<ReturnType, Error> {
    // Implementation
}
````

### Testing Standards

- Write tests for ALL public functions
- Test success, failure, and edge cases
- Achieve >80% code coverage
- Use descriptive names: `test_<function>_<condition>_<expected>`

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_config_with_valid_yaml() {
        let result = parse_config("key: value");
        assert!(result.is_ok());
        assert_eq!(result.unwrap().key, "value");
    }

    #[test]
    fn test_parse_config_with_invalid_yaml() {
        let result = parse_config("invalid: : yaml");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_config_with_empty_string() {
        let result = parse_config("");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_config_propagates_validation_error() {
        let result = parse_config("invalid_field: value");
        assert!(matches!(result, Err(ConfigError::ValidationError(_))));
    }
}
```

---

## Documentation Organization (Diataxis)

Place documentation in the correct category:

| Directory           | Purpose                                        | Examples                                 |
| ------------------- | ---------------------------------------------- | ---------------------------------------- |
| `docs/tutorials/`   | Learning-oriented, step-by-step lessons        | `getting_started.md`                     |
| `docs/how-to/`      | Task-oriented, problem-solving recipes         | `setup_monitoring.md`                    |
| `docs/explanation/` | Understanding-oriented, conceptual discussion  | `phase4_observability_implementation.md` |
| `docs/reference/`   | Information-oriented, technical specifications | `api_specification.md`                   |

Implementation summaries created by AI agents belong in `docs/explanation/`.

---

## Git Conventions

Do not run git commands. The user handles all git interactions.

---

## Living Document

This file is updated as new patterns emerge.

You are a master Rust developer. Follow these rules precisely. All implementation summaries go in `docs/explanation/` with lowercase filenames.
