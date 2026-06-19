#!/usr/bin/env bash
# Add a klauthed GitHub-org team as an owner of every published crate, so the
# crates belong to the organization rather than only the publishing account.
#
# Prerequisites:
#   * the crates are already published,
#   * a team exists in the `klauthed` GitHub org (default: `owners`),
#   * you are a member of that team, and
#   * you're authenticated to crates.io (`cargo login`, or CARGO_REGISTRY_TOKEN).
#
# Usage:
#   scripts/add-owners.sh                # adds github:klauthed:owners
#   scripts/add-owners.sh github:klauthed:publishers
set -euo pipefail

TEAM="${1:-github:klauthed:owners}"

CRATES=(
  klauthed
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
  klauthed-cli
)

echo "Adding owner '${TEAM}' to ${#CRATES[@]} crates"
for crate in "${CRATES[@]}"; do
  echo "→ ${crate}"
  cargo owner --add "${TEAM}" "${crate}"
done
echo "✓ done — '${TEAM}' now co-owns every klauthed crate"
