# Git Write Operations and Pull Request Creation Demo

This demo walks through the XZardgz git write primitives (`create_branch`,
`commit_paths`, `push_branch`) and the GitHub pull request creation workflow. It
has three stages of increasing complexity:

1. **Local git operations only** (no GitHub token required): create a branch and
   commit files to a local repository.
2. **Push to a remote** (requires access to the remote): push the branch to an
   `origin` remote.
3. **Full PR creation** (requires `XZARDGZ_GITHUB_TOKEN`): run a complete
   workflow that scans a repository, reviews it, and opens a pull request with
   the generated reports.

Run all commands from the **repository root**.

## What This Demo Shows

- How the `git::write` module separates write operations from the read-only scan
  surface so plugins can never obtain a write-capable handle.
- How `default_branch_name()` produces timestamped, collision-free branch names
  (`xzardgz/<ulid>`).
- How credential resolution works: `XZARDGZ_GITHUB_TOKEN` first, then OS
  keyring, then SSH agent.
- How to configure pull request creation with the `pr.enabled` opt-in guard.

## Prerequisites

- `xzardgz` installed and on your `PATH`:

  ```bash
  cargo install --path .
  ```

- For Stage 2: a git remote named `origin` pointing at an accessible repository.
- For Stage 3: a GitHub Personal Access Token with the `repo` scope:

  ```bash
  export XZARDGZ_GITHUB_TOKEN="ghp_your_token_here"
  ```

  Store it permanently in the OS keyring as an alternative:

  ```bash
  # macOS (Keychain)
  security add-generic-password \
    -s "xzardgz-github" \
    -a "github_token" \
    -w "ghp_your_token_here"
  ```

## Fixture Repository

The `fixture-repo/` subdirectory demonstrates the kind of repository XZardgz
would write report files into before branching and creating a PR:

```text
fixture-repo/
  README.md       Project description
  findings.md     Placeholder for generated report content
  requirements.txt   Dependency list (empty for this demo)
```

---

## Stage 1: Dry-Run Validation (no API key required)

Validate the workflow plan and configuration without making any AI calls,
writing any files, or touching the git repository:

```bash
xzardgz run \
  --plan demo/git-pr/workflow.yaml \
  --config demo/git-pr/config.yaml \
  --dry-run
```

Replace `YOUR_ORG/YOUR_REPO` in `demo/git-pr/workflow.yaml` with a real
repository URL before running. Expected output:

```text
Workspace: <workspace-id>
Correlation ID: <ulid>
Dry run: validation only, no side effects performed.
Success: true
```

---

## Stage 2: Full Workflow Run Without PR Creation

Run the technical review workflow. Reports are written to `.xzardgz/reports/`
but no branch, commit, or PR is created because `pr.enabled` is `false` in
`demo/git-pr/config.yaml`.

Requires `OPENAI_API_KEY`:

```bash
xzardgz run \
  --plan demo/git-pr/workflow.yaml \
  --config demo/git-pr/config.yaml
```

Expected output:

```text
Workspace: <workspace-id>
Correlation ID: <ulid>
Report (technical_review): .xzardgz/reports/<step-id>/report.md
Report (technical_review): .xzardgz/reports/<step-id>/report.json
Success: true
```

---

## Stage 3: Full Workflow With Pull Request Creation

Enable pull request creation by setting `pr.enabled: true` in
`demo/git-pr/config.yaml` and exporting `XZARDGZ_GITHUB_TOKEN`.

Open `demo/git-pr/config.yaml`, change `pr.enabled: false` to
`pr.enabled: true`, then run:

```bash
export XZARDGZ_GITHUB_TOKEN="ghp_your_token_here"
xzardgz run \
  --plan demo/git-pr/workflow.yaml \
  --config demo/git-pr/config.yaml
```

XZardgz will:

1. Clone or open the repository.
2. Scan the working tree.
3. Run the technical review plugin and write reports.
4. Create a branch named `xzardgz/<ulid>` from the current HEAD.
5. Stage the generated report files and commit them.
6. Push the branch to `origin`.
7. Open a pull request against `main` via the GitHub REST API.

Expected output:

```text
Workspace: <workspace-id>
Correlation ID: <ulid>
Report (technical_review): .xzardgz/reports/<step-id>/report.md
Report (technical_review): .xzardgz/reports/<step-id>/report.json
Pull request: https://github.com/YOUR_ORG/YOUR_REPO/pull/<number>
Success: true
```

### Safety Guard

Pull request creation never targets the repository default branch automatically.
If the computed head branch matches the configured base branch, XZardgz rejects
the operation with a governance error before any network call is made.

---

## Branch Naming

XZardgz generates branch names of the form `xzardgz/<ulid>`, where the ULID is a
26-character, time-sortable, case-insensitive identifier. This means:

- Branch names are unique across concurrent runs.
- Branches sort chronologically in repository browsers.
- No two runs produce the same branch name even when running in parallel.

Example: `xzardgz/01hw3x9r4m5pn6qr2st8uvwxy0`

---

## Credential Lookup Order

When pushing or creating a PR, XZardgz resolves credentials in this order:

1. `XZARDGZ_GITHUB_TOKEN` environment variable.
2. OS keyring entry: service `xzardgz-github`, key `github_token`.
3. SSH agent (for SSH remotes).
4. libgit2 default credential callback.

`file://` remotes (used in local tests) bypass all credential resolution.

---

## Further Reading

- [Git Write Operations Reference](../../docs/reference/cli.md)
- [GitHub PR Creation How-To](../../docs/how-to/create_workflows.md)
- [Configuration Reference](../../docs/reference/configuration.md)
- [Workflow Format Reference](../../docs/reference/workflow_format.md)
