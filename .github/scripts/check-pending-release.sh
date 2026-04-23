#!/usr/bin/env bash
# Outputs the workspace version if it is unreleased (greater than the latest tag),
# or "NO_RELEASE" if the current version is already tagged.

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${repo_root}"

workspace_version="$(sed -n '/^\[workspace\.package\]/,/^\[/s/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"

latest_tag="$(git tag --list 'v*' --sort=-version:refname | head -1 | sed 's/^v//')"

if [[ -z "${latest_tag}" ]]; then
  echo "${workspace_version}"
  exit 0
fi

if [[ "${workspace_version}" == "${latest_tag}" ]]; then
  echo "NO_RELEASE"
  exit 0
fi

# version > tag → unreleased; version < tag → already superseded (shouldn't happen)
higher="$(printf '%s\n%s\n' "${workspace_version}" "${latest_tag}" | sort -V | tail -1)"
if [[ "${higher}" == "${workspace_version}" ]]; then
  echo "${workspace_version}"
else
  echo "NO_RELEASE"
fi
