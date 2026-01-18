#!/usr/bin/env bash
set -euo pipefail

REPO_SLUG="philipesteiff/skill"
BINARY_NAME="skill"

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "This installer is for macOS only." >&2
  exit 1
fi

ARCH="$(uname -m)"
case "${ARCH}" in
  arm64) ARCH="arm64" ;;
  x86_64) ARCH="x86_64" ;;
  *)
    echo "Unsupported architecture: ${ARCH}" >&2
    exit 1
    ;;
esac

TAG="${SKILL_VERSION:-}"
if [[ -z "${TAG}" ]]; then
  TAG="$(curl -fsSL "https://api.github.com/repos/${REPO_SLUG}/releases/latest" \
    | sed -n 's/^[[:space:]]*"tag_name":[[:space:]]*"\(v[^"]*\)".*/\1/p' \
    | head -n1)"
  if [[ -z "${TAG}" ]]; then
    echo "Unable to determine the latest release tag." >&2
    exit 1
  fi
fi

ASSET="skill-darwin-${ARCH}.tar.gz"
URL="https://github.com/${REPO_SLUG}/releases/download/${TAG}/${ASSET}"

TMP_DIR="$(mktemp -d)"
cleanup() {
  rm -rf "${TMP_DIR}"
}
trap cleanup EXIT

curl -fsSL "${URL}" -o "${TMP_DIR}/${ASSET}"
tar -xzf "${TMP_DIR}/${ASSET}" -C "${TMP_DIR}"

INSTALL_DIR="${SKILL_INSTALL_DIR:-}"
if [[ -z "${INSTALL_DIR}" ]]; then
  if [[ -w /usr/local/bin ]]; then
    INSTALL_DIR="/usr/local/bin"
  elif [[ -w /opt/homebrew/bin ]]; then
    INSTALL_DIR="/opt/homebrew/bin"
  else
    INSTALL_DIR="${HOME}/.local/bin"
  fi
fi

mkdir -p "${INSTALL_DIR}"
install -m 0755 "${TMP_DIR}/${BINARY_NAME}" "${INSTALL_DIR}/${BINARY_NAME}"

echo "Installed ${BINARY_NAME} to ${INSTALL_DIR}/${BINARY_NAME}"
if [[ ":${PATH}:" != *":${INSTALL_DIR}:"* ]]; then
  echo "Make sure ${INSTALL_DIR} is in your PATH."
fi
