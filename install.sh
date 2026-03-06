#!/bin/sh
# SteelAPI install script
# Usage: curl -fsSL https://steelapi.dev/install.sh | sh

set -e

VERSION="${STEEL_VERSION:-0.2.0}"
REPO="your-org/steel-api"
INSTALL_DIR="${STEEL_INSTALL_DIR:-/usr/local/bin}"

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
  *) echo "Unsupported OS: $OS. Use: cargo install steel-cli" && exit 1 ;;
esac

URL="https://github.com/${REPO}/releases/download/v${VERSION}/steel-${TARGET}"

echo "Installing steel v${VERSION} for ${TARGET}..."
curl -fsSL "$URL" -o /tmp/steel-download
chmod +x /tmp/steel-download

if [ -w "$INSTALL_DIR" ]; then
  mv /tmp/steel-download "${INSTALL_DIR}/steel"
else
  sudo mv /tmp/steel-download "${INSTALL_DIR}/steel"
fi

echo ""
echo "✅ steel installed to ${INSTALL_DIR}/steel"
echo "   Run: steel --version"
echo ""
echo "Get started:"
echo "   steel init my-app"
echo "   cd my-app && steel serve"
