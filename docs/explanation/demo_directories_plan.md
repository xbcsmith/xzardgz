# Demo Directories Implementation Plan

## Overview

`docs/README.md` and `docs/how-to/setup_watcher.md` reference an `examples/`
tree that does not exist anywhere in the repository. This plan replaces that
reference with a `demo/` directory (per current project naming) containing
one subdirectory per major capability, each runnable end-to-end from its own
`README.md` against a bundled fixture repository, so every capability landed
by the other plans in this series has a working, self-contained
demonstration.

## Current State Analysis

### Existing Infrastructure

- `docs/README.md` already documents the intended Diataxis-organized
  documentation layout (`tutorials`, `how-to`, `explanation`, `reference`),
  giving this plan a consistent place to link from.
- MCP client support (`src/mcp/`) is already functional today, independent
  of the CLI-wiring work in the other plans, and is a good first candidate
  for an immediately-working demo.

### Identified Issues

- `docs/README.md` and `docs/how-to/setup_watcher.md` both link to an
  `examples/` directory that does not exist at the repository root at all.
- No end-to-end, runnable demonstration exists for any capability today.

## Implementation Phases

### Phase 1: Structure and First Demos

#### 1.1 Foundation Work

Create `demo/` at the repository root. Update `docs/README.md` and
`docs/how-to/setup_watcher.md` to reference `demo/` instead of the
nonexistent `examples/` tree.

#### 1.2 Add Foundation Functionality

Build initial demo directories for capabilities that already work today —
starting with `demo/mcp/` (MCP client is functional independent of the other
plans) — each containing a small bundled fixture repository under
`demo/<name>/fixture-repo/` and a `README.md` walking through setup,
command, and expected output.

#### 1.3 Integrate Foundation Work

Add a top-level `demo/README.md` indexing every demo directory.

#### 1.4 Testing Requirements

Consider a lightweight CI smoke check that runs each demo's documented
command against its fixture and confirms it does not error, though this is
secondary to the documentation deliverable itself.

#### 1.5 Deliverables

A `demo/` directory with at least one working, self-contained demo and an
index `README.md`.

#### 1.6 Success Criteria

A new contributor can follow the demo's `README.md` top-to-bottom with no
other context and observe real output matching what is documented.

### Phase 2: Grow Alongside Each Feature Plan

#### 2.1 Feature Work

Add one new `demo/<feature>/` directory as each companion plan in this
series lands — for example `demo/scan/` and `demo/security-review/` with the
CLI-to-workflow-engine integration plan, `demo/git-pr/` with the git write
operations plan, and `demo/watcher/` with the watcher/XZepr integration
plan — rather than attempting to build all demos upfront.

#### 2.2 Integrate Feature

Keep `demo/README.md`'s index current as each new demo is added.

#### 2.3 Configuration Updates

Not applicable.

#### 2.4 Testing Requirements

Extend the Phase 1 smoke check, if implemented, to cover each new demo as it
is added.

#### 2.5 Deliverables

Full demo coverage across every capability delivered by the companion plans.

#### 2.6 Success Criteria

At least one demo (for example a live security-review run) additionally
targets a real public GitHub repository rather than a bundled fixture, to
validate behavior against realistic scale and content, alongside the
bundled-fixture demos used for deterministic, offline verification.
