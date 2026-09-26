#!/usr/bin/env bash
# Every pull request and release runs the tests, `reqfile check` and
# `reqfile test` on the exact revision: .github/check.sh runs them in order,
# after building the tests so a cold build does not count against the
# checks' timeouts, and both workflows run it. Exits 1 listing what is
# missing. Whether the branch rule requires the workflow is a repository
# setting, outside the files checked here.
set -euo pipefail

missing=()

# The line number of the first line of $2 matching the extended regex $1, or nothing.
line_of() { grep -nE "$1" "$2" | head -n 1 | cut -d: -f1 || true; }

script=.github/check.sh
if [ -f "$script" ]; then
  previous=0
  for step in \
    'cargo test --no-run --locked' \
    'cargo test --locked$' \
    'reqfile check$' \
    'reqfile test$'; do
    line=$(line_of "^[^#]*${step}" "$script")
    if [ -z "$line" ] || [ "$line" -le "$previous" ]; then
      missing+=("$script: \`${step%\$}\`, after the steps before it")
    else
      previous=$line
    fi
  done
else
  missing+=("$script")
fi

ci=.github/workflows/ci.yml
if [ -z "$(line_of '^[[:space:]]*pull_request:' "$ci" 2>/dev/null)" ]; then
  missing+=("$ci: a pull_request trigger")
fi
if [ -z "$(line_of 'bash \.github/check\.sh' "$ci" 2>/dev/null)" ]; then
  missing+=("$ci: a step running bash .github/check.sh")
fi

release=.github/workflows/release.yml
checked=$(line_of 'bash \.github/check\.sh' "$release" 2>/dev/null)
built=$(line_of 'dist build' "$release" 2>/dev/null)
if [ -z "$checked" ] || [ -z "$built" ] || [ "$checked" -ge "$built" ]; then
  missing+=("$release: bash .github/check.sh before dist build")
fi

if [ ${#missing[@]} -gt 0 ]; then
  echo "Missing from the acceptance gate:"
  printf '  %s\n' "${missing[@]}"
  exit 1
fi
