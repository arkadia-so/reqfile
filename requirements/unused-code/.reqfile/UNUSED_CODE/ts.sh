#!/usr/bin/env bash
# Runs knip in each package containing the given files. Exits 1 when knip
# reports unused code, and 2 when knip is missing or fails.
set -euo pipefail

packages=$(
  for file in "$@"; do
    dir=$(dirname "$file")
    while [ ! -f "$dir/package.json" ] && [ "$dir" != "." ] && [ "$dir" != "/" ]; do
      dir=$(dirname "$dir")
    done
    if [ -f "$dir/package.json" ]; then echo "$dir"; fi
  done | sort -u
)

# knip installed in the package or in a workspace above it.
find_knip() {
  local dir=$1
  while :; do
    if [ -x "$dir/node_modules/.bin/knip" ]; then echo "$dir/node_modules/.bin/knip"; return; fi
    [ "$dir" != "." ] && [ "$dir" != "/" ] || break
    dir=$(dirname "$dir")
  done
  command -v knip || true
}

status=0
while IFS= read -r package; do
  [ -n "$package" ] || continue
  knip=$(find_knip "$package")
  if [ -z "$knip" ]; then
    echo "knip is not installed for $package: run \`bun install --cwd $package\` (or \`mise run setup\`)" >&2
    exit 2
  fi
  knip=$(cd "$(dirname "$knip")" && pwd)/$(basename "$knip")
  echo "== $package"
  code=0
  (cd "$package" && "$knip" --no-progress) || code=$?
  case $code in
    0) ;;
    1) status=1 ;;
    *) exit 2 ;;
  esac
done <<< "$packages"
exit $status
