#!/usr/bin/env bash
# Publish the klauthed workspace to crates.io, idempotently and resilient to the
# new-crate rate limit.
#
# crates.io throttles brand-new crates (a small burst, then ~1 per 10 minutes),
# so a 13-crate first release can't be pushed in one shot. This script:
#   * publishes in dependency order,
#   * skips any crate+version already on crates.io (safe to re-run), and
#   * on a 429 (rate limit), waits and retries rather than failing the release.
#
# Reads the token from CARGO_REGISTRY_TOKEN (set by CI). `klauthed-examples` is
# `publish = false`, so it's intentionally absent from the list.
set -uo pipefail

# Dependency-ordered: a crate appears after every klauthed-* crate it depends on.
CRATES=(
  klauthed-error
  klauthed-macros
  klauthed-core
  klauthed-i18n
  klauthed-discovery
  klauthed-protocol
  klauthed-observability
  klauthed-security
  klauthed-testing
  klauthed-data
  klauthed-platform
  klauthed-web
  klauthed
  klauthed-cli
)

VERSION="$(grep -m1 '^version' klauthed-core/Cargo.toml | sed 's/.*"\(.*\)".*/\1/')"
UA="klauthed-release (https://github.com/klauthed/rust-libraries)"
RETRY_SLEEP=660 # 11 minutes — one rate-limit refill window, with margin.
MAX_ATTEMPTS=12 # ~2h of patience per crate, well within the CI job limit.

echo "Publishing klauthed workspace v${VERSION}"

is_published() {
  curl -fsS -H "User-Agent: ${UA}" "https://crates.io/api/v1/crates/$1/${VERSION}" 2>/dev/null \
    | grep -q "\"num\":\"${VERSION}\""
}

for crate in "${CRATES[@]}"; do
  if is_published "${crate}"; then
    echo "✓ ${crate} ${VERSION} already on crates.io — skipping"
    continue
  fi

  published=false
  for attempt in $(seq 1 "${MAX_ATTEMPTS}"); do
    echo "→ publishing ${crate} (attempt ${attempt}/${MAX_ATTEMPTS})"
    if output="$(cargo publish -p "${crate}" 2>&1)"; then
      echo "${output}"
      echo "✓ published ${crate} ${VERSION}"
      published=true
      break
    fi

    if grep -qiE "already (uploaded|exists)" <<<"${output}"; then
      echo "✓ ${crate} ${VERSION} already uploaded — continuing"
      published=true
      break
    fi

    if grep -qiE "429|too many|rate.?limit" <<<"${output}"; then
      echo "${output}" | tail -3
      echo "… rate-limited; sleeping ${RETRY_SLEEP}s before retrying ${crate}"
      sleep "${RETRY_SLEEP}"
      continue
    fi

    echo "✗ failed to publish ${crate}:" >&2
    echo "${output}" >&2
    exit 1
  done

  if [[ "${published}" != true ]]; then
    echo "✗ gave up on ${crate} after ${MAX_ATTEMPTS} attempts" >&2
    exit 1
  fi
done

echo "✓ all crates published"
