# Project Main Plan

## Purpose
Build a `skill` CLI that browses, syncs, and applies Agent Skills stored in GitHub repos. The CLI is GitHub-only, uses per-source indexing, and pins installs to commit SHAs for reproducibility.

## Goals
- KISS UX: `browse`, `sync`, `apply` are the only commands.
- GitHub-only distribution with local indexing.
- Bandwidth-safe operations (mirror cache, fetch only required SHAs).
- Works with public and private repos using existing git credentials.

## Non-goals
- A separate registry service.
- Hosting a package server.
- Supporting non-GitHub git hosts in v1.

## User scenarios (acceptance tests)
1) Browse a repo and install one skill.
2) Browse a repo and install all skills.
3) Sync a trusted company repo to keep skills updated.
4) Browse a large community repo with search/filter.

## CLI surface
- `skill browse [<repo|@source>] [--search <term>] [--tags <tag>]`
- `skill sync <repo|@source>`
- `skill apply`

### Source formats
1. GitHub URL: `https://github.com/owner/repo`
2. GitHub shorthand: `owner/repo`
3. Saved source: `@source-id`

## Local filesystem layout
```
$HOME/.skill/
  sources/<source-id>/
    index.sqlite
    head.txt
  cache/
    repos/<slug>.git
  installed/
    <source-id>/<name>/<version-or-sha>/
  lock.json
```

## Data formats
### Config (config.json)
```json
{
  "sources": [
    {
      "id": "acme-skills",
      "url": "https://github.com/acme/skills.git",
      "selection": { "mode": "all" }
    }
  ]
}
```

### Lockfile (lock.json)
```json
{
  "skills": [
    {
      "source_id": "acme-skills",
      "name": "echo-skill",
      "resolved_version": "1.0.0",
      "resolved_commit": "9f3c...",
      "path": "skills/echo-skill",
      "install_dir": "/home/user/.skill/installed/acme-skills/echo-skill/1.0.0",
      "updated_at": "2026-01-21"
    }
  ]
}
```

## Core flows
### Browse
1. Resolve the repo or source.
2. Scan `SKILL.md` files and rebuild the index if needed.
3. Show a TUI list with search/filter.
4. Install selected skills and persist selection state.

### Sync
1. Fetch latest repo head and rebuild index if changed.
2. Install missing skills and update changed ones.

### Apply
1. Discover installed skills from the lockfile.
2. Apply to agent targets via TUI or CLI.
