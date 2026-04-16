---
name: trigger-prepare-release
description: Trigger the prepare-release workflow on GitHub Actions. Supports optional arguments for version, bump type, channel, and base branch. Defaults to bump=patch, channel=alpha, base_branch=current branch.
metadata:
  author: mark
  version: "1.1.0"
allowed-tools: Bash(gh:*), Bash(git:*)
---

# Trigger Prepare Release

Trigger the `prepare-release.yml` workflow on GitHub Actions.

## Arguments

All arguments are optional. Parse from user input (e.g. `/trigger-prepare-release channel=beta bump=minor`):

| Parameter     | Default               | Description                                      |
| ------------- | --------------------- | ------------------------------------------------ |
| `version`     | *(empty)*             | Exact semver version (e.g. `0.5.0`), no `v` prefix. If set, `bump` is ignored. |
| `bump`        | `patch`               | Version bump type: `patch`, `minor`, `major`     |
| `channel`     | `alpha`               | Release channel: `stable`, `alpha`, `beta`, `rc` |
| `base_branch` | current git branch    | Branch to base the release off                   |

## Workflow

### 1. Determine Parameters

- Parse any user-provided arguments, fill the rest with defaults.
- For `base_branch`, if not explicitly provided, detect via `git branch --show-current`.

### 2. Confirm with User

Before triggering, display the resolved parameters and ask for confirmation:

```
Will trigger prepare-release with:
  version:     (empty)
  bump:        patch
  channel:     alpha
  base_branch: dev
Proceed? (y/n)
```

### 3. Trigger the Workflow

```bash
gh workflow run prepare-release.yml \
  --repo UniClipboard/UniClipboard \
  --ref <base_branch> \
  -f bump=<bump> \
  -f channel=<channel> \
  -f base_branch=<base_branch>
```

If `version` is provided, add `-f version=<version>`.

### 4. Confirm Dispatch

After triggering, wait a few seconds then show the latest run:

```bash
gh run list --repo UniClipboard/UniClipboard --workflow=prepare-release.yml --limit 1
```

Report the run URL to the user so they can monitor progress.
