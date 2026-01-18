# skills

A GitHub-only CLI for installing and publishing Agent Skills (`SKILL.md`) with optional registry indexing.

## Prerequisites
- Rust toolchain (stable)
- `git` in PATH
- `just` (optional, for task shortcuts)

## Build and run
```bash
cargo build
cargo run -- --help
```

## Linting and formatting
```bash
cargo fmt
cargo clippy --all-targets --all-features
```

## Tests
```bash
cargo test
```

## Development
```bash
# Run the CLI with local code changes
cargo run -- --help

# Check formatting + linting in one pass
just lint

# Build a release binary for manual testing
just release
```

## Just tasks (optional)
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

## Packaging and install scripts
```bash
scripts/package.sh
scripts/install.sh
```

## Common commands
```bash
# Add a registry repo and sync
cargo run -- add-registry https://github.com/your-org/skills-registry.git
cargo run -- sync

# Search and install
cargo run -- search aws-lambda
cargo run -- install aws/skills/aws-lambda

# Install with TUI picker when multiple skills exist
cargo run -- install owner/repo --pick

# Upgrade/remove/list
cargo run -- upgrade
cargo run -- remove aws/skills/aws-lambda
cargo run -- list

# Publish metadata PR (requires GitHub token)
GITHUB_TOKEN=... cargo run -- publish --registry https://github.com/your-org/skills-registry.git
```

## Configuration
- Skills are stored in `$HOME/.skills` by default.
- Override the base path with `SKILLS_HOME`.
- Publishing uses `GITHUB_TOKEN` (or `GH_TOKEN`).

## TUI
Interactive selection uses `ratatui` + `crossterm` and requires a compatible terminal.
