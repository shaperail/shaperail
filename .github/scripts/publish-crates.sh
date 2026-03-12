#!/usr/bin/env bash

set -euo pipefail

if [[ $# -ne 1 ]]; then
  echo "usage: $0 <version>" >&2
  exit 1
fi

version="$1"
publish_attempts="${PUBLISH_ATTEMPTS:-20}"
publish_retry_delay="${PUBLISH_RETRY_DELAY:-15}"
availability_attempts="${AVAILABILITY_ATTEMPTS:-30}"
availability_retry_delay="${AVAILABILITY_RETRY_DELAY:-10}"

crates=(
  "shaperail-core"
  "shaperail-codegen"
  "shaperail-runtime"
  "shaperail-cli"
)

wait_for_crate_version() {
  local crate="$1"
  local attempt http_code

  echo "Waiting for ${crate} ${version} to appear on crates.io..."
  for attempt in $(seq 1 "${availability_attempts}"); do
    http_code="$(curl -sS -o /dev/null -w '%{http_code}' \
      "https://crates.io/api/v1/crates/${crate}/${version}" || true)"

    if [[ "${http_code}" == "200" ]]; then
      echo "${crate} ${version} is visible on crates.io."
      return 0
    fi

    echo "Attempt ${attempt}/${availability_attempts}: crates.io returned ${http_code:-curl-error}."
    sleep "${availability_retry_delay}"
  done

  echo "Timed out waiting for ${crate} ${version} to propagate on crates.io." >&2
  exit 1
}

publish_crate() {
  local crate="$1"
  local attempt output status

  for attempt in $(seq 1 "${publish_attempts}"); do
    echo "Publishing ${crate} ${version} (attempt ${attempt}/${publish_attempts})..."

    set +e
    output="$(cargo publish -p "${crate}" --locked 2>&1)"
    status=$?
    set -e

    echo "${output}"

    if [[ ${status} -eq 0 ]]; then
      echo "${crate} ${version} published."
      return 0
    fi

    if grep -qi "already uploaded" <<<"${output}"; then
      echo "${crate} ${version} is already published. Continuing."
      return 0
    fi

    if grep -qi "no matching package named" <<<"${output}"; then
      echo "Dependency propagation lag detected for ${crate}. Retrying after ${publish_retry_delay}s..."
      sleep "${publish_retry_delay}"
      continue
    fi

    echo "Publishing ${crate} failed with a non-retryable error." >&2
    exit "${status}"
  done

  echo "Publishing ${crate} exhausted all retries." >&2
  exit 1
}

for crate in "${crates[@]}"; do
  publish_crate "${crate}"
  wait_for_crate_version "${crate}"
done
