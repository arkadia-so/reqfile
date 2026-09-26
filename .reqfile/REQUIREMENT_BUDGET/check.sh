#!/usr/bin/env bash
# Each Reqfile at or under this folder holds at most 10 product
# requirements. Exits 1 listing the Reqfiles over budget, and 2 when a tool
# is missing or reqfile cannot list the requirements.
set -euo pipefail

budget=10
command -v jq >/dev/null || { echo "jq is required" >&2; exit 2; }
# The reqfile running this check, so the listing matches its version.
reqfile=${REQFILE:-reqfile}
prefix=$(git rev-parse --show-prefix)

over=$("$reqfile" list --format json --kind product | jq -r --arg prefix "$prefix" --argjson budget "$budget" '
  .requirements
  | map(select(.reqfile | startswith($prefix)))
  | group_by(.reqfile)[]
  | select(length > $budget)
  | "\(.[0].reqfile): \(length) product requirements, at most \($budget)"
') || exit 2

if [ -n "$over" ]; then
  echo "$over"
  exit 1
fi
