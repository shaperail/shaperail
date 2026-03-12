#!/usr/bin/env bash

set -euo pipefail

if [[ $# -ne 1 ]]; then
  echo "usage: $0 <version>" >&2
  exit 1
fi

version="$1"
release_date="${RELEASE_DATE:-$(date -u +%F)}"
changelog_summary="${CHANGELOG_SUMMARY:-}"

if [[ ! "${version}" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "version must be semver, for example 0.2.3" >&2
  exit 1
fi

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${repo_root}"

perl -0pi -e 's/(\[workspace\.package\][^\[]*?version = ")([^"]+)(")/${1}'"${version}"'${3}/s' Cargo.toml
perl -0pi -e 's/(^release_version:\s+).*$/${1}'"${version}"'/m' docs/_config.yml

perl -0pi -e 's/(shaperail-core = \{ version = ")([^"]+)(")/${1}'"${version}"'${3}/' shaperail-codegen/Cargo.toml
perl -0pi -e 's/(shaperail-core = \{ version = ")([^"]+)(")/${1}'"${version}"'${3}/' shaperail-runtime/Cargo.toml
perl -0pi -e 's/(shaperail-codegen = \{ version = ")([^"]+)(")/${1}'"${version}"'${3}/' shaperail-cli/Cargo.toml
perl -0pi -e 's/(shaperail-core = \{ version = ")([^"]+)(")/${1}'"${version}"'${3}/' shaperail-cli/Cargo.toml
perl -0pi -e 's/(shaperail-runtime = \{ version = ")([^"]+)(")/${1}'"${version}"'${3}/' shaperail-cli/Cargo.toml

if ! grep -Fq "## [${version}]" CHANGELOG.md; then
  tmp_file="$(mktemp)"
  {
    sed -n '1,6p' CHANGELOG.md
    echo
    printf '## [%s] - %s\n\n' "${version}" "${release_date}"
    echo '### Changed'
    echo

    if [[ -n "${changelog_summary}" ]]; then
      while IFS= read -r line; do
        [[ -n "${line}" ]] || continue
        printf -- '- %s\n' "${line}"
      done <<< "${changelog_summary}"
    else
      echo '- TBD'
    fi

    echo
    sed -n '7,$p' CHANGELOG.md
  } > "${tmp_file}"
  mv "${tmp_file}" CHANGELOG.md
fi

release_link="[${version}]: https://github.com/shaperail/shaperail/releases/tag/v${version}"
if ! grep -Fq "${release_link}" CHANGELOG.md; then
  printf '%s\n' "${release_link}" >> CHANGELOG.md
fi
