---
name: Release
about: Track a framework release from prep PR to publish
title: "Release v"
labels: release
assignees: ""
---

## Target version

- [ ] Set the version, for example `0.2.3`

## Changelog summary

- Add user-facing bullets for the changelog and release notes

## Readiness

- [ ] Documentation changes are complete
- [ ] Changelog summary is written
- [ ] CI is green on the release PR
- [ ] Crates.io credentials or trusted publishing are ready

## Release commands

Comment one of these on the issue:

- `/prepare-release 0.2.3`
- `/release-dry-run 0.2.3`
- `/release 0.2.3`

## Release flow

1. Run the `Prepare Release` workflow
2. Review and merge the generated release PR
3. Run the `Release` workflow with the merged version
4. Verify crates.io, GitHub Release, and `shaperail.io`
