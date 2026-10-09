#!/usr/bin/env bash
# The ratchet of FAST_FEEDBACK, an advisory check: a scenario whose median was under
# baseline × (1 − margin / 2) on two measurements in a row (the results' `median` and
# `previous_median`) should have its baseline lowered to the higher of the two plus half the
# margin, rounded up (to the second, to the tenth under 10 s). The half margin is headroom: the
# median of N samples is noisy too, and a baseline set at a lucky pair of nights would make the
# next ordinary night fail. Lists those scenarios and exits 1; with FAST_FEEDBACK_PROPOSAL=<file>, also
# writes there the baseline file with the lowered baselines, to commit (a baseline
# brought under the cap loses its `over_cap` and `todo`). It never raises a baseline: that is
# done by hand, with the reason in the pull request.
# Reads the results like check.sh; does nothing while FAST_FEEDBACK_MEASURE is measuring.
# Exits 0 when no baseline can be lowered, 1 when some can, 2 when it cannot check.
set -euo pipefail

command -v jq >/dev/null || { echo "jq is required" >&2; exit 2; }
baseline=${FAST_FEEDBACK_BASELINE:-.github/fast-feedback.json}
results=${FAST_FEEDBACK_RESULTS:-}
if [ -n "${FAST_FEEDBACK_MEASURE:-}" ]; then
  exit 0  # check.sh is taking the measurements; the ratchet reads them in a second run
fi
if [ -z "$results" ] && [ -f .github/fast-feedback-results.json ]; then
  results=.github/fast-feedback-results.json
fi
[ -n "$results" ] && [ -f "$baseline" ] || exit 0
jq -e . "$baseline" >/dev/null 2>&1 || exit 0  # check.sh reports an invalid file
jq -e '.scenarios | type == "object"' "$results" >/dev/null 2>&1 \
  || { echo "$results: not a results file of FAST_FEEDBACK" >&2; exit 2; }

lowered=$(jq -c --slurpfile results "$results" '
  def up: if . >= 10 then ceil else (. * 10 | ceil) / 10 end;
  . as $top
  | ($results[0].scenarios) as $measured
  | [ .scenarios[]
      | . as $s
      | $measured[$s.id] as $m
      | select($m.median != null and $m.previous_median != null and $s.baseline_seconds != null)
      | ($s.margin // $top.margin) as $margin
      | ($s.baseline_seconds * (1 - $margin / 2)) as $floor
      | select($m.median < $floor and $m.previous_median < $floor)
      | ([$m.median, $m.previous_median] | max * (1 + $margin / 2) | up) as $new
      | select($new < $s.baseline_seconds)
      | {id: $s.id, from: $s.baseline_seconds, to: $new, median: $m.median, previous: $m.previous_median, margin: $margin} ]
' "$baseline")

[ "$(jq length <<<"$lowered")" -gt 0 ] || exit 0

jq -r '.[] | "\(.id): median \(.previous) s then \(.median) s, under its baseline \(.from) s − \(.margin * 50)% twice in a row; lower the baseline to \(.to) s (the higher median + \(.margin * 50)%)"' <<<"$lowered"

if [ -n "${FAST_FEEDBACK_PROPOSAL:-}" ]; then
  jq --argjson lowered "$lowered" '
    . as $top
    | ($lowered | map({(.id): .to}) | add) as $to
    | .scenarios |= map(
        if $to[.id] then
          .baseline_seconds = $to[.id]
          | if .over_cap == true and .baseline_seconds <= (.cap_seconds // $top.cap_seconds // 180) then del(.over_cap, .todo) else . end
        else . end)
  ' "$baseline" >"$FAST_FEEDBACK_PROPOSAL"
fi
exit 1
