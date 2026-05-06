#!/usr/bin/env bash
# install.sh — addhost installer for Linux and macOS
# Usage: curl -fsSL https://raw.githubusercontent.com/daddasoft/addHost/main/install.sh | bash

set -euo pipefail

REPO="daddasoft/addHost"
BIN_NAME="addhost"
INSTALL_DIR="/usr/local/bin"

# ── Colours ───────────────────────────────────────────────────────────────────
RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'
CYAN='\033[0;36m'; BOLD='\033[1m'; RESET='\033[0m'

info()    { echo -e "${CYAN}${BOLD}info${RESET}  $*"; }
success() { echo -e "${GREEN}${BOLD}ok${RESET}    $*"; }
warn()    { echo -e "${YELLOW}${BOLD}warn${RESET}  $*"; }
error()   { echo -e "${RED}${BOLD}error${RESET} $*" >&2; exit 1; }

# ── Detect OS ─────────────────────────────────────────────────────────────────
OS="$(uname -s)"
case "$OS" in
  Linux)  PLATFORM="linux" ;;
  Darwin) PLATFORM="macos" ;;
  *)      error "Unsupported OS: $OS" ;;
esac

# ── Detect architecture ───────────────────────────────────────────────────────
ARCH="$(uname -m)"
case "$ARCH" in
  x86_64 | amd64)  ARCH_TAG="x86_64" ;;
  arm64  | aarch64) ARCH_TAG="aarch64" ;;
  *)                error "Unsupported architecture: $ARCH" ;;
esac

# macOS x86_64 and aarch64 are both supported; Linux only x86_64 for now
if [ "$PLATFORM" = "linux" ] && [ "$ARCH_TAG" = "aarch64" ]; then
  error "Linux arm64 is not yet supported. Please build from source: https://github.com/$REPO"
fi

ASSET_NAME="${BIN_NAME}-${PLATFORM}-${ARCH_TAG}"

# ── Resolve latest release tag ────────────────────────────────────────────────
info "Fetching latest release from github.com/$REPO …"

LATEST_TAG=$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" \
  | grep '"tag_name"' \
  | sed -E 's/.*"tag_name": *"([^"]+)".*/\1/')

if [ -z "$LATEST_TAG" ]; then
  error "Could not determine the latest release tag. Check your internet connection."
fi

info "Latest version: ${BOLD}${LATEST_TAG}${RESET}"

DOWNLOAD_URL="https://github.com/${REPO}/releases/download/${LATEST_TAG}/${ASSET_NAME}"

# ── Download ──────────────────────────────────────────────────────────────────
TMP_DIR="$(mktemp -d)"
TMP_BIN="${TMP_DIR}/${BIN_NAME}"

info "Downloading ${ASSET_NAME} …"
curl -fsSL --progress-bar "$DOWNLOAD_URL" -o "$TMP_BIN" \
  || error "Download failed. URL: $DOWNLOAD_URL"

chmod +x "$TMP_BIN"

# ── Install ───────────────────────────────────────────────────────────────────
if [ -w "$INSTALL_DIR" ]; then
  mv "$TMP_BIN" "${INSTALL_DIR}/${BIN_NAME}"
else
  warn "Need sudo to install to $INSTALL_DIR"
  sudo mv "$TMP_BIN" "${INSTALL_DIR}/${BIN_NAME}"
fi

rm -rf "$TMP_DIR"

# ── Verify ────────────────────────────────────────────────────────────────────
if command -v "$BIN_NAME" &>/dev/null; then
  success "Installed ${BOLD}${BIN_NAME}${RESET} $(${BIN_NAME} --version) → ${INSTALL_DIR}/${BIN_NAME}"
  echo ""
  echo -e "  Run ${BOLD}addhost --help${RESET} to get started."
  echo ""
else
  warn "Binary installed to $INSTALL_DIR but it is not in your PATH."
  warn "Add this to your shell profile:"
  echo ""
  echo "    export PATH=\"\$PATH:${INSTALL_DIR}\""
  echo ""
fi
