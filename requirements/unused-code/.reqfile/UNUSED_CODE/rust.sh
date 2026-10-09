#!/usr/bin/env bash
# Reports unused Rust code (dead code, unused imports and variables) in each
# crate containing the given files, as SARIF on stdout. Exits 1 when there is
# some, 0 when there is none, and 2 when a tool is missing or clippy fails.
set -euo pipefail

for tool in cargo jq; do
  command -v "$tool" >/dev/null || { echo "$tool is required" >&2; exit 2; }
done

# The nearest Cargo.toml above each file.
manifests=$(
  for file in "$@"; do
    dir=$(dirname "$file")
    while [ ! -f "$dir/Cargo.toml" ] && [ "$dir" != "." ] && [ "$dir" != "/" ]; do
      dir=$(dirname "$dir")
    done
    if [ -f "$dir/Cargo.toml" ]; then echo "$dir/Cargo.toml"; fi
  done | sort -u
)

results=$(mktemp)
trap 'rm -f "$results"' EXIT
while IFS= read -r manifest; do
  [ -n "$manifest" ] || continue
  root=$(cargo metadata --no-deps --format-version 1 --manifest-path "$manifest" | jq -r .workspace_root)
  cargo clippy --quiet --all-targets --message-format=json --manifest-path "$manifest" \
    | jq -c --arg root "$root" '
        select(.reason == "compiler-message")
        | .message
        | select(.code.code == "dead_code" or .code.code == "unused_imports" or .code.code == "unused_variables")
        | (.spans[] | select(.is_primary)) as $span
        | {
            ruleId: .code.code,
            message: { text: .message },
            locations: [{ physicalLocation: {
              artifactLocation: { uri: ($root + "/" + $span.file_name) },
              region: { startLine: $span.line_start }
            } }]
          }' >> "$results"
done <<< "$manifests"

jq -s '{ version: "2.1.0", runs: [{ tool: { driver: { name: "clippy" } }, results: unique }] }' "$results"
if [ -s "$results" ]; then exit 1; fi
