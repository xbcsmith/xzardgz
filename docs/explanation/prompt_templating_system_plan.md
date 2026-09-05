# Prompt Templating System Implementation Plan

## Overview

Every prompt in xzardgz today is a hardcoded Rust `const` string (e.g.
`DEFAULT_SYSTEM_PROMPT` in `plugins/security_review/plugin.rs:53-73`), despite
a designed-but-unimplemented override mechanism: a `PluginContext.prompts`
lookup slot, a `PromptsConfig { directories, allow_overrides }` config
struct, an unused `handlebars` dependency, and a fully stubbed `prompts` CLI
subcommand family. This plan builds a real template loader — switching to
Tera as the render engine — so prompts, including each plugin's system
prompt, can be customized without recompiling, while keeping the embedded
defaults strong enough that no external template is required for good
output out of the box.

## Current State Analysis

### Existing Infrastructure

- `Config.prompts: PromptsConfig` already exists with a `directories` list
  (default `.xzardgz/prompts`) and an `allow_overrides` flag, but nothing
  reads from it.
- `PluginContext.prompts: HashMap<String, String>`-style override slot exists
  and is already checked first by at least `security_review` before falling
  back to its compiled-in constant — but nothing ever populates it from disk.
- `commands/prompts.rs` already defines the CLI subcommands (`export`,
  `validate`, `show-order`, `list-templates`, `render`) but every handler is a
  print stub.
- `handlebars = "6.3.2"` is a declared Cargo dependency with zero usages in
  `src/` — dead weight to be removed as part of this plan.
- No `tera` dependency exists yet, and no template files (`.hbs`, `.tera`, or
  otherwise) exist anywhere in the repository.

### Identified Issues

- Every plugin's prompt — including its system prompt — is a single,
  compiled-in option. Changing prompt wording requires a new binary release.
  This is a deployment gap, not just a missing nicety.
- The `prompts` CLI subcommand family is entirely non-functional, so the
  documented customization workflow (`docs/reference/prompt_customization.md`)
  cannot actually be exercised today.
- There is no mechanism to fully replace a plugin's system prompt via
  configuration; only ad hoc variable interpolation into the hardcoded
  constant is possible, and even that is not standardized.

## Implementation Phases

### Phase 1: Build the Tera-Based PromptLoader

#### 1.1 Foundation Work

Add `tera` as a Cargo dependency and remove the unused `handlebars`
dependency.

#### 1.2 Add Foundation Functionality

Implement `src/prompts/loader.rs::PromptLoader`: embedded default templates
via `include_str!` under `src/prompts/templates/{plugin}/`, plus a per-plugin
YAML manifest (prompt key → template filename) mirroring a two-level
registry. Resolution order: if `PromptsConfig.allow_overrides` and a matching
file exists under one of `PromptsConfig.directories`, use it; otherwise fall
back to the embedded default. A malformed or unreadable override logs a
warning and falls back to the embedded default rather than failing the run.

#### 1.3 Integrate Foundation Work

Add a `prompt_loader: PromptLoader` handle to `PluginContext`, replacing the
existing ad hoc `prompts: HashMap<String, String>` lookup slot with calls
through the loader.

#### 1.4 Testing Requirements

Unit tests for: override present and used, override absent falls back to
embedded, malformed override falls back with a warning (not a hard error),
and Tera rendering with plugin-supplied context variables.

#### 1.5 Deliverables

A working `PromptLoader` with manifests migrated for `security_review` and
`technical_review`.

#### 1.6 Success Criteria

Deleting the on-disk override directory entirely still produces complete,
working prompts sourced purely from embedded defaults — external templates
are optional, never required.

### Phase 2: Migrate Prompts, Add System-Prompt Override, Implement the CLI

#### 2.1 Feature Work

Move `security_review`'s `DEFAULT_SYSTEM_PROMPT` and technical-review's
equivalent constant into Tera template files under
`src/prompts/templates/{plugin}/`. Since the embedded default is now the
primary path most deployments will use unmodified, invest in making it as
complete and well-tuned as the current hardcoded version, not a regression
of it.

#### 2.2 Integrate Feature

Add explicit system-prompt override support: a plugin's system prompt is
just another named template resolved through `PromptLoader`, so overriding it
requires no new mechanism beyond what Phase 1 built — only that the system
prompt actually be registered as a named, overridable template (it is
currently a bare constant, not routed through any lookup). Wire this template
into the `AgentSession`-based investigation call introduced by the agent
tool-calling integration plan.

#### 2.3 Configuration Updates

Implement the previously-stubbed `prompts export` (writes embedded templates
to a target directory), `prompts render` (renders a named template against
supplied JSON context and prints the result), `prompts list-templates`, and
`prompts show-order` (prints the resolution order for a given plugin/key)
commands in `commands/prompts.rs` against the real `PromptLoader`.

#### 2.4 Testing Requirements

CLI tests for `prompts export` (asserts files are written and are valid Tera
templates) and `prompts render` (asserts rendered output matches expected
text for a fixed context).

#### 2.5 Deliverables

A fully functional `prompts` subcommand family; both shipped plugins driven
by templated, not hardcoded, prompts, including their system prompts.

#### 2.6 Success Criteria

`xzardgz prompts render security_review system --context '{}'` produces the
real system prompt text used at runtime. Exporting, editing, and re-running
against the exported system prompt observably changes plugin behavior in a
test.
