#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

cargo build --release

mkdir -p dist
OS="$(uname -s | tr '[:upper:]' '[:lower:]')"
ARCH="$(uname -m)"
ARCHIVE="dist/skill-${OS}-${ARCH}.tar.gz"

if [ ! -f target/release/skill ]; then
  echo "Missing target/release/skill. Did the build succeed?" >&2
  exit 1
fi

tar -czf "$ARCHIVE" -C target/release skill

echo "Wrote $ARCHIVE"
