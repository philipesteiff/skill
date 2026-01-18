#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

cargo build --release

DEST="${DEST:-$HOME/.local/bin}"
mkdir -p "$DEST"

if [ ! -f target/release/skill ]; then
  echo "Missing target/release/skill. Did the build succeed?" >&2
  exit 1
fi

cp target/release/skill "$DEST/skill"

echo "Installed to $DEST/skill"
