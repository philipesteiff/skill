# Playground

This directory provides a local, offline sandbox for testing the CLI against
real git repos and a registry repo. The setup script creates two local repos:

- `skills-repo` with two sample skills
- `skills-registry` containing registry metadata

## Setup
```bash
just playground
```

## Usage
```bash
export SKILLS_HOME=playground/work/home

# Register the local registry repo
cargo run -- add-registry file://$PWD/playground/work/skills-registry
cargo run -- sync

# Search and install from the registry
cargo run -- search echo
cargo run -- install acme/skills/echo-skill

# Install directly from the repo
cargo run -- install "file://$PWD/playground/work/skills-repo#skills/notes-skill"
```

## Reset
```bash
CLEAN=1 just playground
```
