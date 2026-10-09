#!/usr/bin/env bash
# Every step function under a core/ folder needs a simulation test in its
# package: a test named with a `simulation` prefix that calls a step function
# and checks an invariant in a loop (simulations.yml), written in the step's
# own file or in a file that names the step's module. Exits 1 listing the step
# functions without one, and 2 when a tool is missing.
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
for tool in ast-grep jq git; do
  command -v "$tool" >/dev/null || { echo "$tool is required" >&2; exit 2; }
done

# Existing code files under the given paths, tracked or untracked, outside .reqfile/.
code_files() {
  git ls-files -z --cached --others --exclude-standard -- "$@" \
    | while IFS= read -r -d '' file; do [ -f "$file" ] && printf '%s\n' "$file"; done \
    | grep -E '\.(rs|py|ts|tsx|js|jsx|mjs|cjs)$' \
    | grep -vE '(^|/)\.reqfile/' || true
}

# `file:line` of each match of an ast-grep rule file in the given files.
# Any scanner failure exits 2, a tool error rather than a verdict; callers
# capture the output with $(...) so the failure stops the script.
matches() {
  local rules=$1; shift
  [ $# -gt 0 ] || return 0
  ast-grep scan --rule "$rules" --json=stream "$@" | jq -r '"\(.file):\(.range.start.line + 1)"' \
    || { echo "ast-grep failed on $rules" >&2; exit 2; }
}

# An extended regex matching references to the module of a file: `core/state`,
# `core.state` or `core::state` for core/state.ts; `core` for core/mod.rs.
module_reference() {
  local path=${1%.*} base dir
  base=$(basename "$path")
  dir=$(basename "$(dirname "$path")")
  case $base in
    mod|index|__init__|lib) echo "(^|[^[:alnum:]_])$dir([^[:alnum:]_]|$)" ;;
    # also `from core import state` and `use crate::core::{state, ...}`
    *) echo "(^|[^[:alnum:]_])$dir((/|\.|::)$base|[[:space:]]+import[[:space:]](.*[^[:alnum:]_])?$base|::\{([^}]*[^[:alnum:]_])?$base)([^[:alnum:]_]|$)" ;;
  esac
}

# The nearest folder above a file holding a package manifest, or the root.
package_of() {
  local dir
  dir=$(dirname "$1")
  while [ "$dir" != "." ]; do
    for manifest in Cargo.toml package.json pyproject.toml; do
      if [ -f "$dir/$manifest" ]; then echo "$dir"; return; fi
    done
    dir=$(dirname "$dir")
  done
  echo "."
}

core=()
while IFS= read -r file; do
  [ -n "$file" ] && core+=("$file")
done <<< "$(code_files | grep -E '(^|/)core/' || true)"
[ ${#core[@]} -gt 0 ] || exit 0

steps=$(matches "$here/steps.yml" "${core[@]}")
missing=0
while IFS= read -r step; do
  [ -n "$step" ] || continue
  package=$(package_of "${step%%:*}")
  tests=()
  while IFS= read -r file; do
    [ -n "$file" ] && tests+=("$file")
  done <<< "$(code_files "$package")"
  simulations=$(matches "$here/simulations.yml" "${tests[@]}")
  reference=$(module_reference "${step%%:*}")
  covered=0
  while IFS= read -r file; do
    [ -n "$file" ] || continue
    if [ "$file" = "${step%%:*}" ]; then covered=1; break; fi
    found=0
    grep -qE "$reference" "$file" || found=$?
    case $found in
      0) covered=1; break ;;
      1) ;;
      *) echo "grep failed on $file" >&2; exit 2 ;;
    esac
  done <<< "$(printf '%s\n' "$simulations" | sed 's/:[0-9]*$//' | sort -u)"
  if [ "$covered" = 0 ]; then
    echo "$step: step function without a simulation test that drives it and checks an invariant, in package $package"
    missing=1
  fi
done <<< "$steps"
exit $missing
