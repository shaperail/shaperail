# Release Process

## What Gets Released
1. **crates.io** — `steel-core`, `steel-codegen`, `steel-runtime`, `steel-cli`
2. **GitHub Releases** — pre-built binaries for macOS, Linux, Windows
3. **Install script** — `curl -fsSL https://steelapi.dev/install.sh | sh`

---

## Step 1 — Pre-release Checklist
Run these before any release. All must pass:

```bash
# Full quality gate
cargo fmt --check
cargo clippy --workspace -- -D warnings
cargo test --workspace

# Security audit
cargo install cargo-audit
cargo audit

# Check all crates publish correctly (dry run)
cargo publish -p steel-core --dry-run
cargo publish -p steel-codegen --dry-run
cargo publish -p steel-runtime --dry-run
cargo publish -p steel-cli --dry-run

# Performance benchmark — must meet PRD targets
cargo bench --workspace
```

---

## Step 2 — Version Bump
All 4 crates must have identical versions.

```bash
# Update version in every Cargo.toml
# workspace.package.version = "0.2.0"
# Also update each crate's dependency on other steel-* crates

# Commit the version bump
git add .
git commit -m "chore: bump version to 0.2.0"
git tag v0.2.0
git push && git push --tags
```

---

## Step 3 — Publish to crates.io
Publish in dependency order (core first, cli last):

```bash
# Login once
cargo login   # paste your crates.io API token

# Publish in order — wait for each to propagate before next
cargo publish -p steel-core
sleep 30
cargo publish -p steel-codegen
sleep 30
cargo publish -p steel-runtime
sleep 30
cargo publish -p steel-cli
```

After publishing, users can install with:
```bash
cargo install steel-cli
```

---

## Step 4 — Build Release Binaries
Cross-compile for all platforms using GitHub Actions (see .github/workflows/release.yml):

```yaml
# .github/workflows/release.yml
name: Release
on:
  push:
    tags: ['v*']

jobs:
  build:
    strategy:
      matrix:
        include:
          - os: ubuntu-latest
            target: x86_64-unknown-linux-gnu
            binary: steel
          - os: ubuntu-latest
            target: aarch64-unknown-linux-gnu
            binary: steel
          - os: macos-latest
            target: x86_64-apple-darwin
            binary: steel
          - os: macos-latest
            target: aarch64-apple-darwin
            binary: steel
          - os: windows-latest
            target: x86_64-pc-windows-msvc
            binary: steel.exe

    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          targets: ${{ matrix.target }}
      - run: cargo build -p steel-cli --release --target ${{ matrix.target }}
      - name: Upload binary
        uses: actions/upload-artifact@v4
        with:
          name: steel-${{ matrix.target }}
          path: target/${{ matrix.target }}/release/${{ matrix.binary }}

  release:
    needs: build
    runs-on: ubuntu-latest
    steps:
      - uses: actions/download-artifact@v4
      - uses: softprops/action-gh-release@v1
        with:
          files: steel-*/**
```

---

## Step 5 — Install Script
Create `install.sh` at the repo root (served at steelapi.dev/install.sh):

```bash
#!/bin/sh
# Detects OS + arch, downloads the right binary, installs to /usr/local/bin

set -e

VERSION="0.2.0"
REPO="your-org/steel-api"

OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)

case "$ARCH" in
  x86_64) ARCH="x86_64" ;;
  arm64|aarch64) ARCH="aarch64" ;;
  *) echo "Unsupported arch: $ARCH"; exit 1 ;;
esac

case "$OS" in
  linux)  TARGET="${ARCH}-unknown-linux-gnu" ;;
  darwin) TARGET="${ARCH}-apple-darwin" ;;
  *) echo "Unsupported OS: $OS"; exit 1 ;;
esac

URL="https://github.com/${REPO}/releases/download/v${VERSION}/steel-${TARGET}"
echo "Downloading steel ${VERSION} for ${TARGET}..."
curl -fsSL "$URL" -o /tmp/steel
chmod +x /tmp/steel
sudo mv /tmp/steel /usr/local/bin/steel
echo "steel installed successfully. Run: steel --version"
```

Users can install with:
```bash
curl -fsSL https://steelapi.dev/install.sh | sh
# or directly:
cargo install steel-cli
```

---

## Version Policy
- `0.x.0` — minor releases with new features (new milestones)
- `0.x.y` — patch releases with bug fixes only
- `1.0.0` — when all v2 milestones complete + performance targets validated
- `2.0.0` — when all v3 milestones complete
- `3.0.0` — when all v4 milestones complete

---

## Rules for Claude
- Never publish without all checks passing
- Always publish in order: core → codegen → runtime → cli
- The GitHub Actions release workflow lives in `.github/workflows/release.yml`
- The install script lives in `install.sh` at the repo root
- Benchmark results must be committed to `BENCHMARKS.md` before any major release
