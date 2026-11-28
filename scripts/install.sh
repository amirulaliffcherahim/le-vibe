#!/usr/bin/env bash
# lazybug Unix/macOS Installer
# Usage: curl -fsSL https://raw.githubusercontent.com/amirulaliffcherahim/lazybug/main/scripts/install.sh | bash
set -euo pipefail

REPO="amirulaliffcherahim/lazybug"
INSTALL_DIR="${HOME}/.local/bin"
mkdir -p "${INSTALL_DIR}"

if command -v cargo >/dev/null 2>&1; then
  echo "Installing latest lazybug via cargo (--no-default-features)..."
  cargo install --git "https://github.com/${REPO}.git" lazybug-cli --no-default-features --root "${HOME}/.local"
  echo "Installed: $(${INSTALL_DIR}/lazybug --version)"
else
  echo "Rust toolchain (cargo) is required on Unix/macOS: https://rustup.rs" >&2
  exit 1
fi
