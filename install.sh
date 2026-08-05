#!/usr/bin/env bash
# install.sh — hostctl installer for Linux and macOS
# Usage: curl -fsSL https://raw.githubusercontent.com/daddasoft/hostctl/main/install.sh | bash

set -euo pipefail

REPO="daddasoft/hostctl"
BIN_NAME="hostctl"
INSTALL_DIR="${HOSTCTL_INSTALL_DIR:-/usr/local/bin}"

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

ASSET_NAME="hostctl-${PLATFORM}-${ARCH_TAG}"

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
CHECKSUMS_URL="https://github.com/${REPO}/releases/download/${LATEST_TAG}/SHA256SUMS"

# ── Download ──────────────────────────────────────────────────────────────────
TMP_DIR="$(mktemp -d)"
TMP_BIN="${TMP_DIR}/${BIN_NAME}"
TMP_SUMS="${TMP_DIR}/SHA256SUMS"

info "Downloading ${ASSET_NAME} …"
curl -fsSL --progress-bar "$DOWNLOAD_URL" -o "$TMP_BIN" \
  || error "Download failed. URL: $DOWNLOAD_URL"
curl -fsSL "$CHECKSUMS_URL" -o "$TMP_SUMS" \
  || error "Could not download release checksums. URL: $CHECKSUMS_URL"

# ── Verify release checksum ───────────────────────────────────────────────────
EXPECTED_HASH="$(awk -v asset="$ASSET_NAME" '$2 == asset { print $1; exit }' "$TMP_SUMS")"
if [ -z "$EXPECTED_HASH" ]; then
  error "Release checksums do not contain $ASSET_NAME."
fi

if command -v sha256sum >/dev/null 2>&1; then
  ACTUAL_HASH="$(sha256sum "$TMP_BIN" | awk '{ print $1 }')"
elif command -v shasum >/dev/null 2>&1; then
  ACTUAL_HASH="$(shasum -a 256 "$TMP_BIN" | awk '{ print $1 }')"
else
  error "SHA-256 verification requires sha256sum or shasum."
fi

if [ "$ACTUAL_HASH" != "$EXPECTED_HASH" ]; then
  error "Checksum verification failed for $ASSET_NAME. The download was not installed."
fi
success "Verified SHA-256: $ACTUAL_HASH"

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
  echo -e "  Run ${BOLD}hostctl --help${RESET} to get started."
  echo ""
else
  warn "Binary installed to $INSTALL_DIR but it is not in your PATH."
  warn "Add this to your shell profile:"
  echo ""
  echo "    export PATH=\"\$PATH:${INSTALL_DIR}\""
  echo ""
fi
