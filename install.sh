#!/bin/sh
# Shaperail install script
# Usage: curl -fsSL https://shaperail.dev/install.sh | sh

set -e

VERSION="${SHAPERAIL_VERSION:-0.2.1}"
REPO="muhammadmahindar/shaperail"
INSTALL_DIR="${SHAPERAIL_INSTALL_DIR:-/usr/local/bin}"
TMP_DIR="$(mktemp -d)"

cleanup() {
  rm -rf "$TMP_DIR"
}

trap cleanup EXIT

OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)

case "$ARCH" in
  x86_64)       ARCH="x86_64" ;;
  arm64|aarch64) ARCH="aarch64" ;;
  *) echo "Unsupported architecture: $ARCH" && exit 1 ;;
esac

case "$OS" in
  linux)  TARGET="${ARCH}-unknown-linux-gnu" ;;
  darwin) TARGET="${ARCH}-apple-darwin" ;;
  *) echo "Unsupported OS: $OS. Use: cargo install shaperail-cli" && exit 1 ;;
esac

ARCHIVE="shaperail-${TARGET}.tar.gz"
URL="https://github.com/${REPO}/releases/download/v${VERSION}/${ARCHIVE}"

echo "Installing shaperail v${VERSION} for ${TARGET}..."
curl -fsSL "$URL" -o "${TMP_DIR}/${ARCHIVE}"
tar -xzf "${TMP_DIR}/${ARCHIVE}" -C "$TMP_DIR"
chmod +x "${TMP_DIR}/shaperail"

if [ -w "$INSTALL_DIR" ]; then
  mv "${TMP_DIR}/shaperail" "${INSTALL_DIR}/shaperail"
else
  sudo mv "${TMP_DIR}/shaperail" "${INSTALL_DIR}/shaperail"
fi

echo ""
echo "shaperail installed to ${INSTALL_DIR}/shaperail"
echo "   Run: shaperail --version"
echo ""
echo "Get started:"
echo "   shaperail init my-app"
echo "   cd my-app && shaperail serve"
