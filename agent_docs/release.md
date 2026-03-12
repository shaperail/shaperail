# Release Process

## What Gets Released
1. **crates.io** — `shaperail-core`, `shaperail-codegen`, `shaperail-runtime`, `shaperail-cli`
2. **GitHub Releases** — pre-built binaries for macOS, Linux, Windows
3. **Install script** — `curl -fsSL https://shaperail.dev/install.sh | sh`

---

## Step 1 — Pre-release Checklist
Run these before any release. All must pass:

```bash
# Full quality gate
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace

# Security audit
cargo install cargo-audit --locked
cargo audit

# Validate the publish/install path
# `shaperail-core` can be dry-run directly; dependent crates are validated
# through the CLI install path and the staged publish script below.
cargo publish -p shaperail-core --dry-run --locked
cargo install --path shaperail-cli --locked --root /tmp/shaperail-install --force

# Compile benchmark targets and refresh BENCHMARKS.md before a tagged release
cargo bench --workspace --no-run
```

---

## Step 2 — Version Bump
All 4 crates must have identical versions.

```bash
# Update version in every Cargo.toml
# workspace.package.version = "0.2.1"
# Also update each crate's dependency on other shaperail-* crates

# Commit the version bump
git add .
git commit -m "chore: bump version to 0.2.1"
git tag v0.2.1
git push && git push --tags
```

---

## Step 3 — Publish to crates.io
Publish in dependency order (core first, cli last):

```bash
# Login once
cargo login   # paste your crates.io API token

# Publish in order and wait for each exact version to propagate before the next
VERSION=0.2.1
bash .github/scripts/publish-crates.sh "$VERSION"
```

After publishing, users can install with:
```bash
cargo install shaperail-cli
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
            binary: shaperail
          - os: ubuntu-latest
            target: aarch64-unknown-linux-gnu
            binary: shaperail
          - os: macos-latest
            target: x86_64-apple-darwin
            binary: shaperail
          - os: macos-latest
            target: aarch64-apple-darwin
            binary: shaperail
          - os: windows-latest
            target: x86_64-pc-windows-msvc
            binary: shaperail.exe

    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          targets: ${{ matrix.target }}
      - run: cargo build -p shaperail-cli --release --locked --target ${{ matrix.target }}
      - name: Upload binary
        uses: actions/upload-artifact@v4
        with:
          name: shaperail-${{ matrix.target }}
          path: target/${{ matrix.target }}/release/${{ matrix.binary }}

  publish:
    needs: build
    runs-on: ubuntu-latest
    steps:
      - uses: dtolnay/rust-toolchain@stable
      - run: bash .github/scripts/publish-crates.sh "${GITHUB_REF_NAME#v}"

  release:
    needs: publish
    runs-on: ubuntu-latest
    steps:
      - uses: actions/download-artifact@v4
      - uses: softprops/action-gh-release@v2
        with:
          files: artifacts/**/*
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
- Benchmark results must be committed to `BENCHMARKS.md` before any tagged release
