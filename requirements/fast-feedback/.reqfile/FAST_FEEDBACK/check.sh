#!/usr/bin/env bash
# FAST_FEEDBACK: the feedback loop's latency never goes up; it only ratchets down.
#
# The repository declares its scenarios and their baselines in .github/fast-feedback.json
# ($FAST_FEEDBACK_BASELINE to read another file). This check fails when:
#   - that file is missing or invalid, or a baseline is over the cap without `over_cap` and a `todo`;
#   - a job of a workflow that runs on pull requests is measured by no scenario;
#   - with measurements, a scenario's median is over baseline × (1 + margin), or over the cap
#     (unless it is marked `over_cap`, whose baseline can then only go down). A scenario without
#     a baseline yet (left out, with a `todo`) is measured and its median reported, never judged.
# Measurements are read from $FAST_FEEDBACK_RESULTS, else .github/fast-feedback-results.json
# when it exists. With FAST_FEEDBACK_MEASURE=<file>, the check first measures every scenario
# (measure.sh: GitHub Actions job history through `gh`, commands run here), writes the results to
# <file> and checks them; FAST_FEEDBACK_PREVIOUS=<file>, the previous night's results, is
# copied into them for the ratchet (ratchet.sh).
#
# Exits 0 when everything holds, 1 listing the violations, 2 when it cannot check.
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
command -v jq >/dev/null || { echo "jq is required" >&2; exit 2; }
. "$here/lib.sh"

baseline=${FAST_FEEDBACK_BASELINE:-.github/fast-feedback.json}
violations=()

if [ ! -f "$baseline" ]; then
  echo "$baseline: missing; declare the scenarios of the feedback loop and their baselines (README of FAST_FEEDBACK)."
  exit 1
fi
if ! jq -e . "$baseline" >/dev/null 2>&1; then
  echo "$baseline: not valid JSON."
  exit 1
fi

# The shape of the file, one violation per problem.
while IFS= read -r problem; do
  [ -n "$problem" ] && violations+=("$baseline: $problem")
done < <(jq -r -f "$here/validate.jq" "$baseline")

# Every job of a workflow that runs on pull requests is measured by a scenario.
covered=$(jq -r '.scenarios[]? | select(.workflow and .job) | "\(.workflow)\t\(.job)"' "$baseline")
for workflow in $(workflow_files); do
  name=${workflow#.github/workflows/}
  while IFS=$'\t' read -r on_pr job _; do
    [ "$on_pr" = 1 ] || continue
    if ! grep -qxF "$name"$'\t'"$job" <<<"$covered"; then
      violations+=("$workflow: job \`$job\` runs on pull requests but no scenario of $baseline measures it")
    fi
  done < <(workflow_jobs "$workflow")
done

# A scenario naming a workflow or job that does not exist measures nothing.
while IFS=$'\t' read -r id workflow job; do
  file=.github/workflows/$workflow
  if [ ! -f "$file" ]; then
    violations+=("$baseline: scenario \`$id\` measures $file, which does not exist")
  elif ! workflow_jobs "$file" | cut -f2 | grep -qxF "$job"; then
    violations+=("$baseline: scenario \`$id\` measures job \`$job\`, which $file does not define")
  fi
done < <(jq -r '.scenarios[]? | select(.workflow and .job) | "\(.id)\t\(.workflow)\t\(.job)"' "$baseline")

if [ ${#violations[@]} -gt 0 ]; then
  printf '%s\n' "${violations[@]}"
  exit 1
fi

# Measurements: taken now, or read from a previous measurement.
results=${FAST_FEEDBACK_RESULTS:-}
if [ -n "${FAST_FEEDBACK_MEASURE:-}" ]; then
  "$here/measure.sh" "$baseline" "$FAST_FEEDBACK_MEASURE" || exit 2
  results=$FAST_FEEDBACK_MEASURE
elif [ -z "$results" ] && [ -f .github/fast-feedback-results.json ]; then
  results=.github/fast-feedback-results.json
fi
[ -n "$results" ] || exit 0
[ -f "$results" ] && jq -e '.scenarios | type == "object"' "$results" >/dev/null 2>&1 \
  || { echo "$results: not a results file of FAST_FEEDBACK" >&2; exit 2; }

# Unmeasured scenarios (too few samples in the history) are reported, never a failure.
jq -r '.scenarios | to_entries[] | select(.value.error) | "not measured: \(.key): \(.value.error)"' "$results" >&2

# Scenarios awaiting a baseline: their medians, to set one from.
jq -r --slurpfile results "$results" '.scenarios[] | select(has("baseline_seconds") | not) | . as $s
  | ($results[0].scenarios[$s.id].median // null) as $m | select($m != null)
  | "no baseline yet: \($s.id): median \($m) s (\($s.todo))"' "$baseline" >&2

report=$(jq -r --slurpfile results "$results" -f "$here/compare.jq" "$baseline")
if [ -n "$report" ]; then
  echo "$report"
  exit 1
fi
