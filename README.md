# skill

A GitHub-only CLI for browsing, syncing, and applying Agent Skills (`SKILL.md`). Installs are pinned to commit SHAs for reproducibility.

## 10-minute mental model
Think of the tool as three simple loops:
1) **Browse** a repo: discover skills and select what to install.
2) **Sync** a source: install missing skills and update existing ones.
3) **Apply** skills: copy installed skills into agent-specific directories (TUI or CLI).

Everything lives in git. There is no central service.

## Architecture at a glance
```mermaid
flowchart LR
  CLI[skill CLI]
  CLI -->|browse/sync| Source[Source resolver]
  Source --> Mirror[Local git mirror cache]
  Mirror --> Indexer[Source index]
  Indexer --> Installer[Installer]
  Installer --> SkillsHome[$HOME/.skills/installed]
```

## Core components (what they do)
- **Source config**: trusted repo URLs saved locally with selection state.
- **Source index**: local SQLite index built from scanned `SKILL.md` files for fast browse/search.
- **Git mirror cache**: local bare mirrors to avoid re-cloning and to keep bandwidth low.
- **Installer**: extracts a single skill folder at a specific commit and writes it to `$HOME/.skills/installed`.
- **Lockfile**: tracks what commit/version was installed and where it lives.

## End-to-end flow (what happens under the hood)
### Browse
1) Resolve the repo URL (first browse implicitly trusts the source).
2) Scan `SKILL.md` files and build the local index if needed.
3) Show a TUI list; user selects skills or “Install all”.
4) Extract each selected skill directory at its commit.
5) Copy into `$HOME/.skills/installed/<source-id>/<name>/<version-or-sha>`.
6) Update `lock.json`.

### Sync
1) Fetch latest repo HEAD and rebuild the index if it changed.
2) Install missing skills and update changed skills from this source.

## Source formats
```text
repo url: https://github.com/owner/repo
shorthand: owner/repo
saved id: @source-id
```
If a repo contains multiple skills, the browse UI allows multi-select or “Install all”.

## Local data layout
```
$HOME/.skills/
  sources/<source-id>/index.sqlite     # per-source index
  sources/<source-id>/head.txt         # last indexed commit
  cache/repos/<slug>.git               # bare mirror cache
  installed/<source-id>/<name>/<ver>/  # installed skill folders
  lock.json
```

## Skill format (what the CLI validates)
`SKILL.md` must start with YAML frontmatter and include:
- `name` (lowercase, hyphenated, matches folder name)
- `description`
Optional `metadata` can include `version`, `tags`, and `namespace`.
Invalid skills are skipped during repo scans and reported in the logs/TUI.

## Installation
macOS (curl installer):
```bash
curl -fsSL https://raw.githubusercontent.com/philipesteiff/skill/main/scripts/install.sh | bash
```

Optional:
```bash
SKILL_VERSION=v0.0.9 curl -fsSL https://raw.githubusercontent.com/philipesteiff/skill/main/scripts/install.sh | bash
SKILL_INSTALL_DIR="$HOME/.local/bin" curl -fsSL https://raw.githubusercontent.com/philipesteiff/skill/main/scripts/install.sh | bash
```

Uninstall:
```bash
curl -fsSL https://raw.githubusercontent.com/philipesteiff/skill/main/scripts/uninstall.sh | bash
```

Homebrew:
```bash
brew tap philipesteiff/tap
brew install skill
```

## Usage
```bash
# Browse and install
skill browse https://github.com/your-org/skills
skill browse @your-org --search observability

# Browse installed skills
skill browse

# Sync updates from a source
skill sync @your-org

# Apply installed skills to agent directories (TUI)
skill apply
```

## Configuration
- Skills are stored in `$HOME/.skills` by default.
- Override the base path with `SKILLS_HOME`.

## Playground (offline, realistic testing)
```bash
just playground
export SKILLS_HOME=playground/work/home
skill browse https://github.com/acme/skills
skill sync @acme-skills
```

## Development
Prereqs: Rust toolchain, `git`, and optionally `just`.

```bash
just
just build
just run -- --help
just fmt
just clippy
just lint
just test
just release
just package
just install
just clean
```

## TUI
All commands use a `ratatui` + `crossterm` interface by default and require a compatible terminal.
