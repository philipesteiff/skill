#!/usr/bin/env bash
set -euo pipefail

BINARY_NAME="skill"

INSTALL_DIR="${SKILL_INSTALL_DIR:-}"
if [[ -z "${INSTALL_DIR}" ]]; then
  if command -v "${BINARY_NAME}" >/dev/null 2>&1; then
    INSTALL_DIR="$(dirname "$(command -v "${BINARY_NAME}")")"
  else
    for candidate in /usr/local/bin /opt/homebrew/bin "${HOME}/.local/bin"; do
      if [[ -x "${candidate}/${BINARY_NAME}" ]]; then
        INSTALL_DIR="${candidate}"
        break
      fi
    done
  fi
fi

if [[ -z "${INSTALL_DIR}" || ! -e "${INSTALL_DIR}/${BINARY_NAME}" ]]; then
  echo "${BINARY_NAME} not found. Set SKILL_INSTALL_DIR if installed elsewhere." >&2
  exit 1
fi

rm -f "${INSTALL_DIR}/${BINARY_NAME}"
echo "Removed ${INSTALL_DIR}/${BINARY_NAME}"
