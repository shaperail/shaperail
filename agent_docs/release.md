# Release Process

Shaperail releases are driven by [release-plz](https://release-plz.dev). The
release pipeline is fully automatic except for one human step: reviewing and
merging the generated release PR.

## What Gets Released
1. **crates.io** — `shaperail-core`, `shaperail-codegen`, `shaperail-runtime`, `shaperail-cli` (in dependency order)
2. **GitHub Releases** — pre-built binaries for macOS, Linux, Windows
3. **Install script** — `curl -fsSL https://shaperail.io/install.sh | sh`

---

## How a Release Happens

Every push to `main` triggers `.github/workflows/release-plz.yml`, which runs two jobs:

1. **`release-plz-release`** — checks whether the workspace version in `Cargo.toml` is ahead of the latest tag. If yes (i.e. the previous push merged a release PR), it publishes all crates to crates.io in dependency order, tags the commit, and creates a GitHub Release with notes generated from the conventional-commit history.
2. **`release-plz-pr`** — inspects commits since the last release. If there are any release-worthy changes (`feat:`, `fix:`, `feat!:`, etc.), it opens or updates a single release PR titled `chore(release): <new version>`. The PR contains:
   - The bumped workspace version in `Cargo.toml`
   - Internal `shaperail-*` dependency versions kept in lockstep
   - A new `CHANGELOG.md` section grouped into Breaking / Added / Changed / Fixed

When a GitHub Release is published, `.github/workflows/release-binaries.yml` fires and uploads release archives for the five supported targets.

The whole flow:

```
feature PR merged ──► release-plz-pr opens/updates release PR
                              │
                              ▼
                  human merges release PR
                              │
                              ▼
release-plz-release ──► crates.io publish
                   ──► git tag vX.Y.Z
                   ──► GitHub Release created
                              │
                              ▼
release-binaries.yml ──► 5 platform archives uploaded to the release
```

---

## What Authors Need To Do

**Use conventional-commit titles on every PR.** That is the only ongoing requirement. Examples:

| Prefix | Bumps | Example |
|---|---|---|
| `feat: ...` | minor | `feat: add SSE streaming for /events` |
| `fix: ...` | patch | `fix: prevent panic on empty cache key` |
| `feat!: ...` or `BREAKING CHANGE` in body | major (or pre-1.0 minor) | `feat!: drop legacy hooks API` |
| `perf:` `refactor:` | patch | `perf: avoid clone in deserializer` |
| `chore:` `docs:` `style:` `test:` `ci:` `build:` | no release | bookkeeping commits |

If a merged PR contains nothing but `chore:`/`docs:`/etc. commits, no release PR is opened. That's the intended behavior.

If a release PR is already open and you merge another `feat:` PR, release-plz updates the open release PR in place — it does not stack PRs.

---

## Cutting a Release

1. Open the auto-generated release PR (titled `chore(release): X.Y.Z`).
2. Review the version bump and the generated CHANGELOG section.
3. Edit the CHANGELOG inline if a generated bullet needs more context (release-plz does not regenerate the CHANGELOG once you commit to the PR branch).
4. Confirm CI is green.
5. Merge.

After merge, do nothing. crates.io publish, tag, GitHub Release, and binary attachment all run from `main`.

---

## Verification

```bash
gh run list --workflow release-plz.yml --limit 1   # release publish run
gh run list --workflow release-binaries.yml --limit 1   # binary upload run
gh release view vX.Y.Z                                  # tag + 5 binaries
cargo install shaperail-cli@X.Y.Z                       # crates.io has it
```

Cross-platform binaries take ~15 minutes (Windows MSVC is the long pole). crates.io publish completes earlier, so `cargo install` works before the GitHub Release page shows binaries.

---

## Configuration

- `release-plz.toml` (repo root) — workspace settings, changelog template, conventional-commit grouping rules.
- `.github/workflows/release-plz.yml` — runs both release-plz commands on push to `main`.
- `.github/workflows/release-binaries.yml` — fires on `release: published` and uploads archives.

Required GitHub secrets:

- `CARGO_REGISTRY_TOKEN` — crates.io API token with publish scope for all four crates.
- `GITHUB_TOKEN` — provided automatically by Actions.

---

## Version Policy
- `0.x.0` — minor releases: new features (new milestones) and breaking changes (pre-1.0)
- `0.x.y` — patch releases: bug fixes and additive non-breaking features
- `1.0.0` — when all v2 milestones complete + performance targets validated
- `2.0.0` — when all v3 milestones complete
- `3.0.0` — when all v4 milestones complete

A `feat!:` commit pre-1.0 currently bumps the minor version, matching the policy above.

---

## Rules for Claude
- The release pipeline is `release-plz`. Do not reintroduce a manual `workflow_dispatch` release path or a 7-place version-bump checklist.
- Never edit `workspace.package.version` or internal `shaperail-*` dependency versions by hand. Let release-plz do it.
- Never edit CHANGELOG.md sections for already-published versions — they are frozen historical records.
- The published CHANGELOG section for the in-flight release PR can be edited if a generated bullet needs more context. release-plz won't overwrite manual edits to `CHANGELOG.md` once they are committed to the PR branch.
- Use conventional-commit titles on every PR. That is the only ongoing release requirement.
- The install script lives in `install.sh` at the repo root.
- Benchmark results must be committed to `BENCHMARKS.md` before any tagged release.
