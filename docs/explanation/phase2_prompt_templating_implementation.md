# Phase 2: Prompt Templating System Implementation

This document records what was built in Phase 2 of the prompt templating system,
as planned in
[`prompt_templating_system_plan.md`](prompt_templating_system_plan.md). Phase 2
completes the prompt templating system by wiring the real `PromptLoader` into
every `prompts` CLI subcommand, replacing the print stubs introduced in the
original scaffold.

## Overview

Phase 2 completes the prompt templating system by:

- Implementing all four previously-stubbed CLI subcommands in
  `src/commands/prompts.rs`.
- Updating the `PromptsCommands::Render` CLI variant to accept two positional
  arguments (`plugin` and `key`) matching the success criterion:
  `xzardgz prompts render security_review system --context '{}'`.
- Adding `PromptLoader::embedded_raw(plugin, key)` as a public static method for
  export functionality.
- Providing comprehensive tests for `prompts export` and `prompts render`.

Phase 1 already completed template migration: `DEFAULT_SYSTEM_PROMPT` constants
were removed, `.tera` files were created under `src/prompts/templates/`, and
both plugins were wired to `PromptLoader` via `PluginContext`. Phase 2 focuses
on CLI usability so that operators can inspect, export, and render prompts
without modifying source code.

## CLI Subcommands

| Command                                            | Description                                                   |
| -------------------------------------------------- | ------------------------------------------------------------- |
| `prompts export [--output-dir <dir>]`              | Writes embedded `.tera` files to disk for local customization |
| `prompts show-order`                               | Prints the three-level resolution chain                       |
| `prompts list-templates <plugin>`                  | Lists known template keys for a plugin                        |
| `prompts render <plugin> <key> [--context <json>]` | Renders a template with optional JSON context                 |

The `prompts validate` subcommand existed before Phase 2 and retains its
existing behavior; it is not changed in this phase.

## PromptsCommands::Render Update

The `Render` variant in `src/cli.rs` is changed from a single `template`
positional argument to two positional arguments, `plugin` and `key`, to match
the runtime resolution model used by `PromptLoader::render`.

Before Phase 2:

```rust
Render {
    /// Name of the template to render.
    template: String,

    /// JSON context string to apply during rendering.
    #[arg(long)]
    context: Option<String>,
},
```

After Phase 2:

```rust
Render {
    /// Plugin identifier whose template to render, e.g. "security_review".
    plugin: String,

    /// Template key within the plugin, e.g. "system".
    key: String,

    /// JSON context string to apply during rendering.
    #[arg(long)]
    context: Option<String>,
},
```

The companion test `test_prompts_render_parses_correctly` in `src/cli.rs` is
updated to pass two positional values and to destructure
`{ plugin, key, context }` instead of `{ template, context }`.

## prompts export

The `Export` handler iterates all statically registered templates and writes
each one to disk under the requested output directory. The default output
directory when `--output-dir` is omitted is `.xzardgz/prompts`.

The implementation:

1. Calls `PromptLoader::known_plugins()` to iterate every registered plugin.
2. For each plugin, calls `PromptLoader::known_keys(plugin)` to iterate every
   registered key.
3. Constructs the destination path `{output_dir}/{plugin}/{key}.tera` and
   creates parent directories with `std::fs::create_dir_all`.
4. Calls `PromptLoader::embedded_raw(plugin, key)` to obtain the raw template
   source and writes it to disk with `std::fs::write`.
5. Prints each exported path to stdout so operators know what was created.

The exported files are byte-for-byte copies of the compiled-in defaults. Editing
them and placing them in the directory listed under `prompts.directories` in
`config.yaml` causes `PromptLoader` to load the edited version at runtime,
because `allow_overrides` defaults to `true`.

## prompts show-order

The `ShowOrder` handler prints the three-level resolution chain with concrete
path examples and references to the configuration fields that control each
level.

The output explains:

1. **In-memory overrides** -- highest priority; set programmatically via
   `PromptLoader::with_in_memory_overrides`. Not user-configurable from the CLI;
   used by integration tests and future agent orchestration.
2. **File-based overrides** -- checked when `config.prompts.allow_overrides` is
   `true`; files are resolved at path `{dir}/{plugin}/{key}.tera` for each entry
   in `config.prompts.directories` (default: `.xzardgz/prompts`).
3. **Compiled-in embedded defaults** -- lowest priority; always present in the
   binary; sourced from `src/prompts/templates/{plugin}/{key}.tera` at build
   time via `include_str!`.

The output includes the default value of `config.prompts.directories`
(`.xzardgz/prompts`) so operators understand where to place override files
without reading source code.

## prompts list-templates

The `ListTemplates` handler normalizes the incoming `plugin` argument from
kebab-case to underscore before passing it to `PromptLoader::known_keys`. This
allows both `security-review` and `security_review` to resolve to the registered
plugin identifier `security_review`.

Normalization is a simple `str::replace('-', "_")` applied before the lookup.

If the normalized name is not found in `PromptLoader::known_plugins()`, the
handler prints an error message that lists all known plugin identifiers so the
operator can correct the input. The command still returns `Ok(())` in this case;
the error is informational.

If the plugin is known, the handler prints each key returned by `known_keys` on
a separate line.

## prompts render

The `Render` handler accepts `plugin` and `key` as two required positional
arguments and an optional `--context <json>` flag.

The implementation:

1. Normalizes `plugin` from kebab-case to underscore (same logic as
   `list-templates`).
2. Parses the `--context` JSON string, if supplied, using `serde_json`. The JSON
   value must be an object; each top-level key is inserted into a
   `tera::Context` as a string value.
3. Constructs a `PromptLoader` from `PromptsConfig::default()`.
4. Calls `loader.render(plugin, key, &ctx)` and prints the result to stdout.
5. Returns `Err` if the rendered result is an empty string, because an empty
   render indicates that the plugin and key combination is not registered in the
   embedded template registry.

The `serde_json` crate is already a direct dependency (`serde_json = "1.0.145"`
in `Cargo.toml`), so no new dependencies are introduced.

## PromptLoader::embedded_raw

`embedded_raw` is a new public static method on `PromptLoader` that exposes the
raw (un-rendered) embedded template source for a `(plugin, key)` pair. It
delegates directly to the private `embedded_template` method:

````rust
/// Returns the statically embedded raw template source for `(plugin, key)`,
/// or `None` if no embedded template is registered for the pair.
///
/// This method is the public counterpart of the private `embedded_template`
/// helper. It exists to support the `prompts export` command, which needs
/// the unrendered Tera source to write to disk rather than a rendered string.
///
/// # Arguments
///
/// * `plugin` - Plugin identifier, e.g. `"security_review"`.
/// * `key` - Template key within the plugin, e.g. `"system"`.
///
/// # Returns
///
/// The raw Tera template source string, or `None` when the pair is not
/// registered.
///
/// # Examples
///
/// ```
/// use xzardgz::prompts::PromptLoader;
///
/// let raw = PromptLoader::embedded_raw("security_review", "system");
/// assert!(raw.is_some());
/// assert!(!raw.unwrap().is_empty());
/// ```
pub fn embedded_raw(plugin: &str, key: &str) -> Option<&'static str> {
    Self::embedded_template(plugin, key)
}
````

The return type is `Option<&'static str>` so callers can distinguish between a
registered template that happens to be empty and an unregistered pair. The
`prompts export` handler treats `None` as a programming error (the `PLUGIN_KEYS`
registry is out of sync with `EMBEDDED_TEMPLATES`) and emits a warning rather
than writing an empty file.

## Testing

All new tests follow the naming convention
`test_<command>_<condition>_<expected>` established by the project testing
standards. Tests that write files use the `tempfile` crate (already in
`dev-dependencies`) to create isolated temporary directories.

### Required Tests

The four tests mandated by the Phase 2 plan are:

| Test name                                                           | Location              | What it verifies                                              |
| ------------------------------------------------------------------- | --------------------- | ------------------------------------------------------------- |
| `test_execute_export_writes_template_files_to_directory`            | `commands/prompts.rs` | Files exist on disk at the expected paths after `export` runs |
| `test_execute_export_files_are_valid_tera_templates`                | `commands/prompts.rs` | Each exported file is parseable by `tera::Tera::one_off`      |
| `test_execute_render_with_default_config_returns_ok`                | `commands/prompts.rs` | `render security_review system` returns `Ok(())`              |
| `test_execute_render_security_review_system_contains_expected_text` | `commands/prompts.rs` | Rendered output contains the substring `"security"`           |

### Full Test Inventory

The following table lists all 15 new or updated tests introduced in Phase 2.

| Test name                                                             | Location                | What it verifies                                                                  |
| --------------------------------------------------------------------- | ----------------------- | --------------------------------------------------------------------------------- |
| `test_execute_export_writes_template_files_to_directory`              | `commands/prompts.rs`   | `export` creates `{tmp}/{plugin}/{key}.tera` for each registered template         |
| `test_execute_export_files_are_valid_tera_templates`                  | `commands/prompts.rs`   | Every exported file is accepted by `tera::Tera::one_off` without error            |
| `test_execute_export_with_custom_output_dir_writes_to_specified_path` | `commands/prompts.rs`   | `--output-dir /custom/path` places files under the custom path                    |
| `test_execute_export_creates_plugin_subdirectories`                   | `commands/prompts.rs`   | Parent directories are created when they do not exist before export               |
| `test_execute_render_with_default_config_returns_ok`                  | `commands/prompts.rs`   | `render security_review system` returns `Ok(())` with no context                  |
| `test_execute_render_security_review_system_contains_expected_text`   | `commands/prompts.rs`   | Rendered `security_review/system` output contains `"security"`                    |
| `test_execute_render_with_context_json_injects_variables`             | `commands/prompts.rs`   | A JSON context with a known variable key is substituted in the output             |
| `test_execute_render_unknown_plugin_key_returns_err`                  | `commands/prompts.rs`   | An unregistered plugin/key pair returns `Err` (empty render result)               |
| `test_execute_render_kebab_plugin_name_is_normalized`                 | `commands/prompts.rs`   | `security-review` (kebab) resolves identically to `security_review`               |
| `test_execute_list_templates_known_plugin_returns_ok`                 | `commands/prompts.rs`   | `list-templates security_review` prints keys and returns `Ok(())`                 |
| `test_execute_list_templates_unknown_plugin_returns_ok`               | `commands/prompts.rs`   | An unknown plugin name prints available plugins and returns `Ok(())`              |
| `test_execute_list_templates_kebab_name_normalizes`                   | `commands/prompts.rs`   | `list-templates security-review` resolves to the `security_review` plugin         |
| `test_embedded_raw_known_pair_returns_some`                           | `src/prompts/loader.rs` | `embedded_raw("security_review", "system")` returns `Some` with non-empty content |
| `test_embedded_raw_unknown_plugin_returns_none`                       | `src/prompts/loader.rs` | `embedded_raw("nonexistent", "system")` returns `None`                            |
| `test_prompts_render_with_plugin_and_key_parses_correctly`            | `src/cli.rs`            | CLI parser accepts two positional args and populates `plugin` and `key` fields    |

The existing `test_prompts_render_parses_correctly` test in `src/cli.rs` is
replaced by `test_prompts_render_with_plugin_and_key_parses_correctly`, which
destructures `PromptsCommands::Render { plugin, key, context }` and asserts both
positional values.

## Files Changed

| File                                                          | Change                                                                                                                                                |
| ------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------- |
| `src/cli.rs`                                                  | `PromptsCommands::Render` changed from `{ template, context }` to `{ plugin, key, context }`; `test_prompts_render_parses_correctly` updated to match |
| `src/commands/prompts.rs`                                     | All four stub handlers replaced with real implementations; 12 new unit tests added                                                                    |
| `src/prompts/loader.rs`                                       | `embedded_raw` public static method added; 2 new unit tests added                                                                                     |
| `docs/explanation/phase2_prompt_templating_implementation.md` | This file                                                                                                                                             |

No changes to `Cargo.toml` are required. `tera = "1"` (template rendering) and
`serde_json = "1.0.145"` (JSON context parsing) are both already present as
direct dependencies. `tempfile = "3.23.0"` is already present in
`dev-dependencies` and is used by the export tests.

## Success Criteria

The success criterion from the plan:

> `xzardgz prompts render security_review system --context '{}'` produces the
> real system prompt text used at runtime.

This holds because:

1. The `Render` variant now carries `plugin` and `key` as separate positional
   arguments, so the shell invocation
   `prompts render security_review system --context '{}'` routes correctly to
   `plugin = "security_review"` and `key = "system"`.
2. An empty JSON object `'{}'` parses to a zero-entry `tera::Context` with no
   variable substitutions, which is valid input for any embedded template that
   contains no variable references.
3. `PromptLoader::render("security_review", "system", &ctx)` returns the
   compiled-in embedded default sourced from
   `src/prompts/templates/security_review/system.tera`, which is the same
   content used by the plugin at runtime.
4. The handler prints the rendered string to stdout and returns `Ok(())`,
   producing observable output identical to what the `security_review` plugin
   sends to the LLM.
