#!/bin/sh
# The integration test files given (a Rust crate's tests/*.rs, which cargo
# requires there) that no requirement's command check runs, as SARIF. A
# test file is run by a check whose command names it with `--test <name>`,
# or runs a bare `cargo test`, which runs them all.
set -eu
# Reqfiles inside .reqfile/ folders are examples, not requirements.
runs=$(git ls-files -z --cached --others --exclude-standard -- '*Reqfile.yaml' 'Reqfile.yaml' ':!:*.reqfile/*' | xargs -0 grep -h 'run:' 2>/dev/null || true)
printf '{"version":"2.1.0","runs":[{"tool":{"driver":{"name":"tests-prove-requirements"}},"results":['
sep=''
for file in "$@"; do
  tests=$(dirname "$file")
  crate=$(dirname "$tests")
  # Only files directly in a crate's tests/ folder are test targets.
  [ "$(basename "$tests")" = tests ] && [ -f "$crate/Cargo.toml" ] || continue
  name=$(basename "$file" .rs)
  if printf '%s\n' "$runs" | grep -Eq -- "--test[ =]$name([ \"']|$)"; then continue; fi
  if printf '%s\n' "$runs" | grep -Eq 'cargo test' && ! printf '%s\n' "$runs" | grep 'cargo test' | grep -q -- '--test'; then continue; fi
  printf '%s{"ruleId":"TESTS_PROVE_REQUIREMENTS","message":{"text":"No requirement runs %s: add `--test %s` to the check of the requirement it proves"},"locations":[{"physicalLocation":{"artifactLocation":{"uri":"%s"},"region":{"startLine":1}}}]}' "$sep" "$file" "$name" "$file"
  sep=','
done
printf ']}]}\n'
