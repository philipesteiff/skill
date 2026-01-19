#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WORK="$ROOT/playground/work"
SKILLS_REPO="$WORK/skills-repo"
REGISTRY_REPO="$WORK/skills-registry"
HOME_DIR="$WORK/home"

if [ -d "$WORK" ] && [ "${CLEAN:-}" != "1" ]; then
  echo "Playground already exists at $WORK"
  echo "Delete it or re-run with CLEAN=1"
  exit 0
fi

if [ "${CLEAN:-}" = "1" ]; then
  rm -rf "$WORK"
fi

mkdir -p "$WORK"
mkdir -p "$HOME_DIR"

mkdir -p "$SKILLS_REPO/skills/echo-skill"
mkdir -p "$SKILLS_REPO/skills/notes-skill"

cat <<'SKILL' > "$SKILLS_REPO/skills/echo-skill/SKILL.md"
---
name: echo-skill
description: Echo input with basic validation.
metadata:
  version: 1.0.0
  tags: [cli, example]
  namespace: acme
---

# Echo Skill

Responds with the input string and validates length.
SKILL

cat <<'SKILL' > "$SKILLS_REPO/skills/notes-skill/SKILL.md"
---
name: notes-skill
description: Manage plain-text notes locally.
metadata:
  version: 0.2.0
  tags: [notes, example]
  namespace: acme
---

# Notes Skill

Creates and lists local notes files.
SKILL

cd "$SKILLS_REPO"
git init -q
git config user.email "playground@example.com"
git config user.name "Playground"
git add .
git commit -m "Add sample skills" -q

SKILLS_COMMIT="$(git rev-parse HEAD)"

mkdir -p "$REGISTRY_REPO/skills/acme"

cat <<JSON > "$REGISTRY_REPO/skills/acme/echo-skill.json"
{
  "namespace": "acme",
  "name": "echo-skill",
  "description": "Echo input with basic validation.",
  "repo_url": "file://$SKILLS_REPO",
  "path": "skills/echo-skill",
  "tags": ["cli", "example"],
  "latest": { "version": "1.0.0", "commit": "$SKILLS_COMMIT" },
  "versions": [
    { "version": "1.0.0", "commit": "$SKILLS_COMMIT" }
  ]
}
JSON

cat <<JSON > "$REGISTRY_REPO/skills/acme/notes-skill.json"
{
  "namespace": "acme",
  "name": "notes-skill",
  "description": "Manage plain-text notes locally.",
  "repo_url": "file://$SKILLS_REPO",
  "path": "skills/notes-skill",
  "tags": ["notes", "example"],
  "latest": { "version": "0.2.0", "commit": "$SKILLS_COMMIT" },
  "versions": [
    { "version": "0.2.0", "commit": "$SKILLS_COMMIT" }
  ]
}
JSON

cd "$REGISTRY_REPO"
git init -q
git config user.email "playground@example.com"
git config user.name "Playground"
git add .
git commit -m "Add registry metadata" -q

echo "Playground ready:"
echo "- Skills repo:   $SKILLS_REPO"
echo "- Registry repo: $REGISTRY_REPO"
echo "- Skills home:   $HOME_DIR"
