#!/usr/bin/env bash

set -euo pipefail

if [[ $# -ne 1 ]]; then
  echo "usage: $0 <version>" >&2
  exit 1
fi

version="$1"

awk -v version="${version}" '
  $0 ~ "^## \\[" version "\\]" { printing=1; next }
  printing && $0 ~ "^## \\[" { exit }
  printing { print }
' CHANGELOG.md | sed '/^[[:space:]]*$/N;/^\n$/D'
