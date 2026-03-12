# Release Process

## What Gets Released
1. **crates.io** — `shaperail-core`, `shaperail-codegen`, `shaperail-runtime`, `shaperail-cli`
2. **GitHub Releases** — pre-built binaries for macOS, Linux, Windows
3. **Install script** — `curl -fsSL https://shaperail.io/install.sh | sh`

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

## Step 2 — Prepare The Release PR
Use either the release issue commands or the GitHub Actions **Prepare Release**
workflow instead of editing versions and tags by hand.

Inputs:

- `version` — semver without a leading `v`, for example `0.2.3`
- `base_ref` — normally `main`
- `changelog_summary` — optional bullet points, one per line

What it does:

- updates `workspace.package.version`
- updates internal `shaperail-*` dependency versions across crate manifests
- updates `docs/_config.yml` release metadata
- adds a changelog section and release link if missing
- refreshes `Cargo.lock`
- opens a release PR from `codex/release-v<version>`

Merge that PR only after CI is green.

Issue-driven option:

- open a release issue from `.github/ISSUE_TEMPLATE/release.md`
- comment `/prepare-release 0.2.3`
- GitHub will queue the same workflow for you

---

## Step 3 — Run The Release Workflow
After the release PR is merged, use the release issue commands or the GitHub
Actions **Release** workflow.

Inputs:

- `version` — the merged semver, for example `0.2.3`
- `ref` — normally `main`
- `dry_run` — set to `true` to validate without publishing

What it does:

- validates release metadata with `.github/scripts/assert-release-version.sh`
- runs formatting, clippy, tests, audit, install-path validation, and publish dry-run checks
- builds release binaries for Linux, macOS, and Windows
- publishes crates to crates.io in dependency order with `.github/scripts/publish-crates.sh`
- creates and pushes the git tag
- creates or updates the GitHub Release and uploads binaries

The release workflow is self-contained on purpose. It does not depend on a
second tag-triggered workflow.

Issue-driven commands:

- `/release-dry-run 0.2.3` — run the full release validation without publishing
- `/release 0.2.3` — publish crates, create the tag, and create/update the GitHub Release

The comment router lives in `.github/workflows/release-command.yml` and only
accepts commands from repository owners, members, or collaborators on issues
marked as release issues.

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
- The GitHub Actions release workflows live in `.github/workflows/prepare-release.yml`, `.github/workflows/release.yml`, and `.github/workflows/release-command.yml`
- The install script lives in `install.sh` at the repo root
- Benchmark results must be committed to `BENCHMARKS.md` before any tagged release
