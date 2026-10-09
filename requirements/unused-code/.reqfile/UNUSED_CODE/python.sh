#!/usr/bin/env bash
# Reports top-level Python functions and classes that nothing uses, as SARIF on
# stdout: their name appears nowhere else in their file nor in any other file of
# the repository (tracked or untracked, outside .reqfile/). Decorated
# definitions are registered by a framework and tests are run by their runner,
# so both are skipped. Exits 1 when there is some unused code, 0 when there is
# none, and 2 when a tool is missing.
set -euo pipefail

for tool in ast-grep jq git; do
  command -v "$tool" >/dev/null || { echo "$tool is required" >&2; exit 2; }
done

files=()
for file in "$@"; do
  case /$file in
    */tests/*|*/test_*.py|*_test.py|*/conftest.py) ;;
    *) files+=("$file") ;;
  esac
done

results=$(mktemp)
trap 'rm -f "$results"' EXIT
if [ ${#files[@]} -gt 0 ]; then
  definitions=$(ast-grep scan --inline-rules '
id: top-level-definition
language: python
rule:
  any: [{ kind: function_definition }, { kind: class_definition }]
  inside: { kind: module }
  has: { field: name, pattern: $NAME }
' --json=stream "${files[@]}" | jq -r '"\(.file)\t\(.range.start.line + 1)\t\(.metaVariables.single.NAME.text)"') \
    || { echo "ast-grep failed" >&2; exit 2; }
  while IFS=$'\t' read -r file line name; do
    [ -n "$name" ] || continue
    case $name in __*__|test_*) continue ;; esac
    # Uses elsewhere in its own file, then in any other file.
    if grep -Fwn -- "$name" "$file" | grep -qv "^$line:"; then continue; fi
    found=0
    git grep -qFw --untracked -e "$name" -- ':(top)' ":(exclude)$file" ':(top,exclude,glob)**/.reqfile/**' || found=$?
    case $found in
      0) continue ;;
      1) ;;
      *) echo "git grep failed" >&2; exit 2 ;;
    esac
    jq -cn --arg file "$file" --argjson line "$line" --arg name "$name" '{
      ruleId: "unused-definition",
      message: { text: "\($name) is defined but never used" },
      locations: [{ physicalLocation: { artifactLocation: { uri: $file }, region: { startLine: $line } } }]
    }' >> "$results"
  done <<< "$definitions"
fi

jq -s '{ version: "2.1.0", runs: [{ tool: { driver: { name: "unused-python-definitions" } }, results: . }] }' "$results"
if [ -s "$results" ]; then exit 1; fi
