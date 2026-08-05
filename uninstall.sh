#!/usr/bin/env sh
set -eu

INSTALL_DIR="${HOSTCTL_INSTALL_DIR:-/usr/local/bin}"
TARGET="${INSTALL_DIR}/hostctl"

if [ ! -e "$TARGET" ]; then
  echo "hostctl is not installed at $TARGET"
  exit 0
fi

if [ -w "$INSTALL_DIR" ]; then
  rm -f "$TARGET"
else
  sudo rm -f "$TARGET"
fi
echo "Uninstalled hostctl from $TARGET"
