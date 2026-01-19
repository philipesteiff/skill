# Project Main Plan

## Purpose
Build a `skill` CLI that installs and updates Agent Skills stored in GitHub repos. The CLI is GitHub-only, uses a lightweight metadata registry, and pins every install to a commit SHA for reproducibility.

## Goals
- KISS UX: `search`, `install`, `upgrade`, `remove`, `list` are the daily commands.
- GitHub-only distribution with an optional metadata registry repo.
- Bandwidth-safe operations (cache, fetch only required SHAs, sparse checkout).
- Works with public and private repos using existing git credentials.

## Non-goals
- Hosting a custom registry service or package server.
- Implementing a separate auth system or token store.
- Supporting non-GitHub git hosts in v1.

## Policy and spec compliance
- GitHub acceptable use: avoid excessive bandwidth; do not clone whole repos when a single skill path is needed.
- GitHub API usage: no polling, serialize mutating calls, backoff on rate limits.
- Agent Skills spec: `SKILL.md` with YAML frontmatter containing `name` and `description`; `name` matches the directory and is lowercase alnum + hyphens; metadata fields (version, tags, author) are allowed. Only load full `SKILL.md` bodies when a skill is activated (progressive disclosure).

## User scenarios (acceptance tests)
A. Install CLI via `curl` or Homebrew and run `skill` from PATH.
B. Search then install from registry: `skill search aws-lambda` -> `skill install aws/skills/aws-lambda`.
C. Install from private GitHub with existing credentials: `skill install my-private-repo/skill-a`.
D. Upgrade all `@latest` installs: `skill upgrade`.
E. Remove a skill: `skill remove aws/skills/aws-lambda` or `skill remove --all`.

## CLI surface (v1)
- `skill search <query>`: search registry index.
- `skill install <ref>[@latest|@<version>]`: install a skill.
- `skill install`: install skills listed in `skills.toml` in the current directory.
- `skill upgrade`: refresh all skills installed with `@latest`.
- `skill remove <ref>`: remove installed skill and lock entry.
- `skill remove --all`: uninstall all skills and clear the lock.
- `skill list`: list installed skills.
- `skill add-registry <git-url>`: add a registry repo.
- `skill sync`: update registry index.

Example:
```bash
skill search aws-lambda
skill install aws/skills/aws-lambda@latest
skill upgrade
skill remove aws/skills/aws-lambda
skill remove --all
```

## Reference formats
Support exactly three ref forms:
1. Registry ref: `namespace/name/path[@latest|@1.2.0]`
2. GitHub shorthand: `owner/repo/skill-name[@latest]`
3. Full git URL: `https://github.com/owner/repo.git#path/to/skill[@latest]`

If a repo contains multiple skills and the ref does not specify a path, install all skills and print the list. Provide `--pick` for interactive selection.

## Local filesystem layout
```
$HOME/.skills/
  registry/
    <registry-id>/
      repo/              # cloned metadata repo
      index.sqlite       # SQLite FTS index
      head.txt           # last synced commit
  cache/
    repos/
      github.com__owner__repo.git   # bare mirror cache
  installed/
    <namespace>/
      <skill-name>/
        <version-or-latest>/
          SKILL.md
          scripts/...
          references/...
          assets/...
  lock.json
```

## Data formats
### Lockfile (lock.json)
- Records requested selector, resolved version, commit SHA, and source location.
- One entry per installed skill.

Example:
```json
{
  "skills": [
    {
      "namespace": "aws",
      "name": "aws-lambda",
      "requested": "@latest",
      "resolved_version": "1.2.0",
      "resolved_commit": "9f3c...",
      "repo_url": "https://github.com/aws/skills.git",
      "path": "skills/aws-lambda"
    }
  ]
}
```

### Registry metadata (per-skill JSON)
Stored at `skills/<namespace>/<name>.json` inside the registry repo.

Example:
```json
{
  "namespace": "aws",
  "name": "aws-lambda",
  "description": "Deploy and manage AWS Lambda with IaC patterns",
  "repo_url": "https://github.com/aws/skills.git",
  "path": "skills/aws-lambda",
  "tags": ["aws", "lambda"],
  "latest": { "version": "1.2.0", "commit": "9f3c..." },
  "versions": [
    { "version": "1.2.0", "commit": "9f3c...", "published_at": "2025-01-01T00:00:00Z" }
  ]
}
```

### Search index
Use SQLite FTS5 over `name`, `description`, and `tags`. Rebuild `index.sqlite` on registry HEAD change.

## Core flows
### Registry sync
1. `git fetch` registry repo.
2. If HEAD changed, rebuild `index.sqlite` from JSON files.

### Install algorithm
1. Resolve ref to `(repo_url, path, commit_sha)` using registry data or repo HEAD.
2. Fetch the SHA into a bare mirror cache.
3. Use sparse checkout for `path` only.
4. Validate `SKILL.md` frontmatter and directory name.
5. Copy the skill directory into `$HOME/.skills/installed/...`.
6. Write/update `lock.json`.

### Upgrade
- For each lock entry with `requested == @latest`, resolve the newest commit from the registry and reinstall if different.

### Remove
- Delete the installed directory and remove the lock entry. Keep cached mirrors unless `skill prune-cache` is added later.

## Auth and private repos
All git operations go through the system `git` binary so SSH agents, credential helpers, and `gh auth` work. If a fetch fails, print a single actionable hint (e.g., "Ensure `git clone <repo>` works in this terminal").

## Implementation approach (Rust)
- CLI: `clap`.
- Frontmatter parsing: `serde_yaml` with a small frontmatter splitter.
- JSON: `serde_json`.
- SQLite FTS: `rusqlite` (FTS5 enabled).
- FS and temp: `walkdir`, `tempfile`.
- Errors: `thiserror`, `anyhow`.
- Git: shell out to `git` CLI for consistency with user auth.

## Testing strategy
- Unit tests: ref parsing, frontmatter validation, lockfile read/write.
- Integration tests: install/upgrade/remove using local git fixtures (no network).
- Search tests: build index from fixture metadata and assert queries.

## Milestones
### Milestone 1: CLI skeleton + local install
- Command parsing and config paths.
- Direct GitHub install with commit pinning.
- Lockfile create/update.

### Milestone 2: Registry sync + search
- Registry repo clone/update.
- JSON ingestion + SQLite FTS index.
- `skill search` and `skill list`.

### Milestone 3: Upgrade + remove
- `skill upgrade` using registry latest.
- `skill remove` and lock maintenance.

### Milestone 4: Packaging
- Release packaging (tarballs, brew formula, install script).
