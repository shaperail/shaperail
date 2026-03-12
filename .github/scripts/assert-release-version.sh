#!/usr/bin/env bash

set -euo pipefail

if [[ $# -ne 1 ]]; then
  echo "usage: $0 <version>" >&2
  exit 1
fi

version="$1"

if [[ ! "${version}" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "version must be semver, for example 0.2.3" >&2
  exit 1
fi

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${repo_root}"

workspace_version="$(sed -n '/^\[workspace\.package\]/,/^\[/s/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)"
docs_version="$(sed -n 's/^release_version: //p' docs/_config.yml | head -n 1)"
codegen_core_version="$(sed -n 's/^shaperail-core = { version = "\(.*\)", path = "..\/shaperail-core" }/\1/p' shaperail-codegen/Cargo.toml)"
runtime_core_version="$(sed -n 's/^shaperail-core = { version = "\(.*\)", path = "..\/shaperail-core" }/\1/p' shaperail-runtime/Cargo.toml)"
cli_codegen_version="$(sed -n 's/^shaperail-codegen = { version = "\(.*\)", path = "..\/shaperail-codegen" }/\1/p' shaperail-cli/Cargo.toml)"
cli_core_version="$(sed -n 's/^shaperail-core = { version = "\(.*\)", path = "..\/shaperail-core" }/\1/p' shaperail-cli/Cargo.toml)"
cli_runtime_version="$(sed -n 's/^shaperail-runtime = { version = "\(.*\)", path = "..\/shaperail-runtime" }/\1/p' shaperail-cli/Cargo.toml)"

for value in \
  "${workspace_version}" \
  "${docs_version}" \
  "${codegen_core_version}" \
  "${runtime_core_version}" \
  "${cli_codegen_version}" \
  "${cli_core_version}" \
  "${cli_runtime_version}"; do
  if [[ "${value}" != "${version}" ]]; then
    echo "release metadata mismatch: expected ${version}, found ${value}" >&2
    exit 1
  fi
done

grep -Fq "## [${version}]" CHANGELOG.md
grep -Fq "[${version}]: https://github.com/shaperail/shaperail/releases/tag/v${version}" CHANGELOG.md
