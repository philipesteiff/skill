# Playground

This directory provides a local, offline sandbox for testing the current CLI
flows (`sync`, `apply`) against local git repos. The setup script creates two repos:

- `skills-repo` with two sample skills
- `skills-registry` containing legacy metadata (kept for compatibility experiments)

## Setup
```bash
just playground
```

## Usage
```bash
export SKILLS_HOME=playground/work/home

# Seed a local source config entry for the local repo.
cat > "$SKILLS_HOME/config.json" <<EOF
{
  "sources": [
    {
      "id": "local-skills",
      "url": "file://$PWD/playground/work/skills-repo",
      "selection": { "mode": "all" }
    }
  ]
}
EOF

# Install/update from the local source.
cargo run -- sync @local-skills

# Apply a synced skill into the current project (no TUI).
cargo run -- apply --no-tui --targets codex:project --skills local-skills/echo-skill
```

## Reset
```bash
CLEAN=1 just playground
```
