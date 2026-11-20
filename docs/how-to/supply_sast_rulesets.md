# How to Supply External SAST Rulesets

## Overview

xzardgz ships 25 built-in SAST rules (15 crypto, 10 security) embedded directly
in the binary. These rules run without any configuration or network access.
External rulesets extend coverage beyond the built-in set, but require explicit
configuration and licence approval before use.

This guide walks through the steps to add, configure, sync, and verify an
external ruleset source.

## Prerequisites

- `git` installed and available on `PATH`
- `xzardgz` v0.1.0 or later installed and configured
- Network access to the ruleset repository host (sync only; scans are offline)

## Step 1: Create a Sources Manifest

Copy the bundled example manifest to a location of your choice:

```bash
cp src/clients/rulesets/assets/sources.example.yaml .xzardgz/sources.yaml
```

The manifest is a YAML file that lists known ruleset repositories. Each entry
has the following fields:

| Field               | Required | Description                                             |
| ------------------- | -------- | ------------------------------------------------------- |
| `description`       | No       | Human-readable label for this source                    |
| `repository_url`    | Yes      | HTTPS URL of the Git repository                         |
| `repository_branch` | Yes      | Branch name to fetch                                    |
| `license`           | Yes      | SPDX licence identifier (e.g. `MIT`, `Apache-2.0`)      |
| `enabled`           | No       | Set to `true` to include this source (default: `false`) |

A minimal entry looks like:

```yaml
"0xdea":
  description: "Security-focused Semgrep rules"
  repository_url: "https://github.com/0xdea/semgrep-rules"
  repository_branch: "main"
  author: "0xdea"
  license: "MIT"
  enabled: true
```

Source IDs are arbitrary strings. Quote IDs that begin with a digit or special
character (for example, `"0xdea"`).

## Step 2: Configure the Manifest Path

Add the path to your `config.yaml` under the `sast:` key:

```yaml
sast:
  ruleset_sources_file: ".xzardgz/sources.yaml"
```

If this field is omitted, xzardgz looks for `~/.config/xzardgz/sources.yaml`.
You can also override the path at the command line by passing `--sources-file`
to any `ruleset` subcommand.

## Step 3: Review Licences

Before enabling a source, check its licence class. xzardgz enforces the
following default policy:

| Class           | Examples                      | Default policy |
| --------------- | ----------------------------- | -------------- |
| Permissive      | MIT, Apache-2.0, BSD-\*, ISC  | Allowed        |
| WeakCopyleft    | LGPL, MPL-2.0, EPL            | Allowed        |
| StrongCopyleft  | GPL-2.0, GPL-3.0              | Denied         |
| NetworkCopyleft | AGPL-3.0                      | Denied         |
| NonCommercial   | CC-BY-NC-\*                   | Denied         |
| Restricted      | "Commons Clause", proprietary | Denied         |
| Unknown         | No licence declared           | Denied         |

Sources with a denied licence class are skipped during sync. They never
contribute findings silently. Policy override is not supported.

## Step 4: Sync Rulesets

Run the sync command to fetch all enabled sources:

```bash
xzardgz ruleset sync
```

Sync performs these steps in order:

1. Fetches each enabled repository and pins the resolved commit SHA.
2. Verifies the declared licence against the licence policy.
3. Parses rules and records which are supported versus gated.
4. Writes a `BundleManifest` to the cache for each source.

To sync a single source by ID:

```bash
xzardgz ruleset sync 0xdea
```

Omitting the source ID syncs all enabled sources.

## Step 5: Verify

List all loaded sources with their rule counts and licence classes:

```bash
xzardgz ruleset list
```

Check for configuration issues without scanning any code:

```bash
xzardgz ruleset doctor
```

Doctor validates that URLs use HTTPS, branch names are non-empty, and detected
licences match the declared values. No files are written or modified.

## Step 6: Run a Scan

Once rulesets are synced, the SAST scan command uses both built-in and external
rules automatically:

```bash
xzardgz sast scan ./my-project
```

No additional flags are required. The scanner loads external rulesets from the
cache using the `BundleManifest` files written during sync.

## Troubleshooting

| Issue              | Solution                                                                                                                                |
| ------------------ | --------------------------------------------------------------------------------------------------------------------------------------- |
| "licence denied"   | The source has a non-permissive licence. Review the class table in Step 3 and use a different source, or accept the risk explicitly.    |
| "source not found" | Check that `repository_url` and `repository_branch` are correct. Run `xzardgz ruleset doctor` for details.                              |
| "no rules found"   | If rules live in a subdirectory, set the `subdir` field on the entry to the relative path of the rules directory within the repository. |

## Offline Operation

Once rulesets are synced to the local cache, all subsequent scans are fully
offline. The sync command only needs to run when you want to update to newer
rule versions or add a new source.
