#!/usr/bin/env bash
# le-vibe Unix/macOS Installer
# Usage: curl -fsSL https://raw.githubusercontent.com/amirulaliffcherahim/le-vibe/main/scripts/install.sh | bash
set -euo pipefail

REPO="amirulaliffcherahim/le-vibe"
INSTALL_DIR="${HOME}/.local/bin"
mkdir -p "${INSTALL_DIR}"

if command -v cargo >/dev/null 2>&1; then
  echo "Installing latest le-vibe via cargo (--no-default-features)..."
  cargo install --git "https://github.com/${REPO}.git" le-vibe-cli --no-default-features --root "${HOME}/.local"
  echo "Installed: $(${INSTALL_DIR}/le-vibe --version)"
else
  echo "Rust toolchain (cargo) is required on Unix/macOS: https://rustup.rs" >&2
  exit 1
fi
