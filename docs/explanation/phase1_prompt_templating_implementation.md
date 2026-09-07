# Phase 1: Prompt Templating System Implementation

This document records what was built in Phase 1 of the prompt templating system,
as planned in
[`prompt_templating_system_plan.md`](prompt_templating_system_plan.md). Phase 1
delivered the core `PromptLoader` infrastructure: a three-level template
resolution engine, embedded default templates for both shipped plugins,
per-plugin manifest files, and integration of the loader into `PluginContext`.

## Overview

Phase 1 built the following:

- The `src/prompts/` module, containing `mod.rs` and `loader.rs`.
- `PromptLoader`: a three-level, infallible template resolver backed by the Tera
  engine.
- Embedded default templates for the `security_review` and `technical_review`
  plugins, compiled into the binary via `include_str!`.
- Per-plugin `manifest.yaml` files that map prompt keys to template filenames.
- Integration of `PromptLoader` into `PluginContext`, replacing the ad hoc
  `prompts: HashMap<String, String>` lookup slot.
- Addition of `tera = "1"` to `Cargo.toml` and removal of the previously unused
  `handlebars = "6.3.2"` dependency.

## Architecture

`PromptLoader` resolves a `(plugin, key)` pair through three levels in strict
priority order:

1. **In-memory overrides** -- checked first. The override map is keyed by
   `"{plugin}/{key}"` (e.g. `"security_review/system"`). Values are raw Tera
   template strings rendered against the supplied context. A render failure
   emits `tracing::warn!` and falls through to the next level.

2. **File-based overrides** -- checked only when
   `PromptsConfig::allow_overrides` is `true`. For each path in
   `PromptsConfig::directories`, the loader looks for a file at
   `{dir}/{plugin}/{key}.tera`. I/O errors and render failures each emit a
   `tracing::warn!` and continue to the next directory rather than
   short-circuiting to the embedded default.

3. **Compiled-in embedded default** -- template content is compiled into the
   binary via `include_str!` from `src/prompts/templates/{plugin}/{key}.tera`.
   If no embedded template is registered for the pair, a warning is emitted and
   an empty string is returned. If the embedded template fails to render, a
   warning is emitted and the raw template content is returned unchanged.

The loader is fully infallible: `render` always returns a `String` and never
propagates an error to the caller.

## Module Structure

```text
src/prompts/
  mod.rs                     module root, re-exports PromptLoader
  loader.rs                  PromptLoader implementation and unit tests
  templates/
    security_review/
      system.tera            embedded default system prompt
      manifest.yaml          key-to-filename mapping
    technical_review/
      system.tera            embedded default system prompt
      manifest.yaml          key-to-filename mapping
```

`src/prompts/mod.rs` declares `pub mod loader;` and re-exports `PromptLoader` so
callers import from `xzardgz::prompts::PromptLoader`. It also carries the
module-level doc comment that describes the resolution order.

## Public API

| Method                     | Signature                                                                       | Purpose                                         |
| -------------------------- | ------------------------------------------------------------------------------- | ----------------------------------------------- |
| `new`                      | `fn new(config: PromptsConfig) -> Self`                                         | Constructs a loader with no in-memory overrides |
| `with_in_memory_overrides` | `fn with_in_memory_overrides(self, overrides: HashMap<String, String>) -> Self` | Builder: replaces the override map              |
| `has_in_memory_overrides`  | `fn has_in_memory_overrides(&self) -> bool`                                     | Returns `true` if the override map is non-empty |
| `render`                   | `fn render(&self, plugin: &str, key: &str, ctx: &tera::Context) -> String`      | Resolves and renders a template                 |
| `known_plugins`            | `fn known_plugins() -> &'static [&'static str]`                                 | Returns all statically registered plugin IDs    |
| `known_keys`               | `fn known_keys(plugin: &str) -> &'static [&'static str]`                        | Returns registered keys for a plugin, or `&[]`  |

`PromptLoader` derives `Clone` and `Debug`. `Default` constructs an instance via
`PromptsConfig::default()`, which sets `directories` to `[".xzardgz/prompts"]`
and `allow_overrides` to `true`.

## Embedded Templates

Both shipped plugins are registered in the static `EMBEDDED_TEMPLATES` array as
`(&str, &str, &str)` tuples of `(plugin, key, content)`. The content is compiled
into the binary at build time via `include_str!`:

| Plugin             | Key      | Template path                                        |
| ------------------ | -------- | ---------------------------------------------------- |
| `security_review`  | `system` | `src/prompts/templates/security_review/system.tera`  |
| `technical_review` | `system` | `src/prompts/templates/technical_review/system.tera` |

The two companion static arrays, `KNOWN_PLUGINS` and `PLUGIN_KEYS`, back the
`known_plugins()` and `known_keys()` methods respectively. All three arrays must
be kept in sync when a new plugin or key is added.

## Template File Structure

Each plugin directory under `src/prompts/templates/` contains two files.

`{key}.tera` is the Tera template source. It is compiled directly into the
binary and serves as the lowest-priority fallback for every deployment.

`manifest.yaml` is a YAML file mapping prompt key names to the corresponding
template filenames in the same directory:

```yaml
# Prompt key to template filename mapping for the security-review plugin.
templates:
  system: system.tera
```

The manifest is not read at runtime by `PromptLoader`; it exists as a
human-readable index and as the reference format for the `prompts export`
command planned in Phase 2. File-based overrides are located by constructing the
path `{override_dir}/{plugin}/{key}.tera` directly.

## Configuration

`PromptsConfig` already existed in `src/config.rs` before Phase 1. No structural
changes were required to support the loader:

```rust
pub struct PromptsConfig {
    /// Directories to search for prompt template files.
    pub directories: Vec<String>,
    /// Whether user-supplied prompt overrides are permitted.
    pub allow_overrides: bool,
}
```

The default value sets `directories` to `[".xzardgz/prompts"]` and
`allow_overrides` to `true`. When the override directory does not exist on disk,
`PromptLoader::render` silently falls through to the embedded default for every
template. No error is raised and no configuration change is needed.

## PluginContext Integration

`PluginContext` previously exposed `prompts: HashMap<String, String>` as a bare
ad hoc lookup slot populated by callers from outside the templating system.
Phase 1 replaced that field with a `prompt_loader: PromptLoader` handle so that
the three-level resolution chain is always available inside a plugin run.

The existing `with_prompts(HashMap<String, String>)` builder method was retained
for call-site compatibility. Its implementation now delegates to
`with_in_memory_overrides()` on the internal loader:

```rust
pub fn with_prompts(mut self, prompts: HashMap<String, String>) -> Self {
    self.prompt_loader = self.prompt_loader.with_in_memory_overrides(prompts);
    self
}
```

Callers that previously supplied a pre-rendered string via `with_prompts` now
supply a raw Tera template string with the same key convention
(`"{plugin}/{key}"`). The loader renders it against the context at the point of
the `render` call, enabling context-variable substitution that was not possible
with a bare string map.

## Dependencies

| Crate        | Change  | Version | Purpose                                      |
| ------------ | ------- | ------- | -------------------------------------------- |
| `tera`       | added   | `1`     | Tera template engine for rendering           |
| `handlebars` | removed | `6.3.2` | Previously declared but never used in `src/` |

## Testing

Sixteen unit tests are in `src/prompts/loader.rs` under `mod tests`:

| Test name                                                              | What it verifies                                                                           |
| ---------------------------------------------------------------------- | ------------------------------------------------------------------------------------------ |
| `test_render_security_review_system_returns_nonempty_string`           | Embedded `security_review/system` renders to a non-empty string                            |
| `test_render_technical_review_system_returns_nonempty_string`          | Embedded `technical_review/system` renders to a non-empty string                           |
| `test_render_embedded_default_contains_expected_keyword`               | `security_review/system` output contains the word `"security"`                             |
| `test_render_with_in_memory_override_uses_override`                    | In-memory override is returned when present                                                |
| `test_render_without_override_falls_back_to_embedded`                  | Empty directory list with `allow_overrides: true` falls back to embedded                   |
| `test_render_with_malformed_in_memory_override_falls_back_to_embedded` | Invalid Tera syntax in an override falls through to the embedded default                   |
| `test_render_with_file_override_uses_file_when_allow_overrides_true`   | A valid `.tera` file in an override directory is used when `allow_overrides` is `true`     |
| `test_render_with_allow_overrides_false_ignores_file_override`         | A valid `.tera` file in an override directory is ignored when `allow_overrides` is `false` |
| `test_render_with_missing_plugin_returns_empty_string`                 | An unregistered plugin name returns an empty string                                        |
| `test_render_with_context_variables_substituted`                       | Tera variables in an in-memory override are interpolated from the supplied context         |
| `test_known_plugins_contains_both`                                     | `known_plugins()` includes both `"security_review"` and `"technical_review"`               |
| `test_known_keys_security_review_contains_system`                      | `known_keys("security_review")` includes `"system"`                                        |
| `test_known_keys_technical_review_contains_system`                     | `known_keys("technical_review")` includes `"system"`                                       |
| `test_known_keys_unknown_plugin_returns_empty`                         | `known_keys("unknown")` returns an empty slice                                             |
| `test_has_in_memory_overrides_returns_false_when_empty`                | Predicate returns `false` on a freshly constructed loader                                  |
| `test_has_in_memory_overrides_returns_true_when_set`                   | Predicate returns `true` after `with_in_memory_overrides` is called                        |

## Files Changed

| File                                                   | Change                                                                         |
| ------------------------------------------------------ | ------------------------------------------------------------------------------ |
| `Cargo.toml`                                           | Added `tera = "1"`; removed `handlebars = "6.3.2"`                             |
| `src/lib.rs`                                           | Added `pub mod prompts;`                                                       |
| `src/prompts/mod.rs`                                   | New: module root; re-exports `PromptLoader`                                    |
| `src/prompts/loader.rs`                                | New: `PromptLoader` implementation and 16 unit tests                           |
| `src/prompts/templates/security_review/system.tera`    | New: embedded default system prompt                                            |
| `src/prompts/templates/security_review/manifest.yaml`  | New: key-to-filename mapping                                                   |
| `src/prompts/templates/technical_review/system.tera`   | New: embedded default system prompt                                            |
| `src/prompts/templates/technical_review/manifest.yaml` | New: key-to-filename mapping                                                   |
| `src/plugins/context.rs`                               | Replaced `prompts: HashMap<String, String>` with `prompt_loader: PromptLoader` |

## Success Criteria

The success criterion from the plan:

> Deleting the on-disk override directory entirely still produces complete,
> working prompts sourced purely from embedded defaults -- external templates
> are optional, never required.

This holds because `PromptLoader::render` falls through silently to the
compiled-in default whenever no file exists at any of the configured override
paths. Removing `.xzardgz/prompts` from disk raises no error and produces no
empty output; the embedded template is returned as if no override had ever been
configured.
