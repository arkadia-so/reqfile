#!/usr/bin/env bash
# Tests of FAST_FEEDBACK's measurement and ratchet, which the labeled examples cannot reach:
# measure.sh against a stub `gh` serving a fixed GitHub Actions history, command timing, and
# the baseline file the ratchet proposes. The examples (`reqfile eval`) cover the verdicts.
# Needs bash, jq and awk. Exits 1 listing the failed cases.
set -euo pipefail
assets=$(cd "$(dirname "$0")/.reqfile/FAST_FEEDBACK" && pwd)
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
failures=0
fail() { echo "FAIL  $*"; failures=$((failures + 1)); }
ok() { echo "ok    $*"; }
# expect <case> <jq expression that must be true on the file> <file>
expect() { if jq -e "$2" "$3" >/dev/null; then ok "$1"; else fail "$1: $2 is false on $(jq -c . "$3")"; fi; }

# ---- a repository and a stub gh serving its history ----
repo=$tmp/repo
mkdir -p "$repo/.github/workflows" "$tmp/bin" "$tmp/api"
cd "$repo"
cat >.github/workflows/ci.yml <<'EOF'
on: [pull_request, push]
jobs:
  ci:
    name: CI
    runs-on: ubuntu-latest
    steps:
      - run: make checks
EOF
# The stub answers `gh api <path>` from $tmp/api/<path with / and ? and & as _>, and
# `gh repo view` with a fixed name.
cat >"$tmp/bin/gh" <<EOF
#!/usr/bin/env bash
if [ "\$1" = repo ]; then echo acme/app; exit 0; fi
[ "\$1" = api ] || exit 9
key=\$(printf '%s' "\$2" | sed -E 's/created=[^&]*//; s#[/?&=%]+#_#g')
[ ! -f "$tmp/api/\$key.404" ] || { echo "gh: Not Found (HTTP 404)" >&2; exit 1; }
[ -f "$tmp/api/\$key" ] || { echo "no fixture for \$2 (\$key)" >&2; exit 1; }
cat "$tmp/api/\$key"
EOF
chmod +x "$tmp/bin/gh"
export PATH="$tmp/bin:$PATH"
unset GITHUB_REPOSITORY FAST_FEEDBACK_BASELINE FAST_FEEDBACK_RESULTS FAST_FEEDBACK_MEASURE FAST_FEEDBACK_PREVIOUS FAST_FEEDBACK_SAMPLES FAST_FEEDBACK_PROPOSAL FAST_FEEDBACK_COMMANDS
fixture() { printf '%s' "$1" | sed -E 's/created=[^&]*//; s#[/?&=%]+#_#g'; }

# Runs, newest first: 105 a rerun (attempt 2) of a PR, 104 on push, 103 a PR touching 3 files,
# 102 and 101 PRs touching 1 file, 100 a PR whose job has Checks skipped.
jq -n '{workflow_runs: [
  {id: 105, run_attempt: 2, event: "pull_request", pull_requests: [{number: 5}]},
  {id: 104, run_attempt: 1, event: "push", pull_requests: []},
  {id: 103, run_attempt: 1, event: "pull_request", pull_requests: [{number: 3}]},
  {id: 102, run_attempt: 1, event: "pull_request", head_sha: "c102", pull_requests: []},
  {id: 101, run_attempt: 1, event: "pull_request", pull_requests: [{number: 1}]},
  {id: 100, run_attempt: 1, event: "pull_request", pull_requests: [{number: 9}]}]}' \
  >"$tmp/api/$(fixture 'repos/acme/app/actions/workflows/ci.yml/runs?status=success&per_page=30&created=x&page=1')"
echo '{"workflow_runs": []}' >"$tmp/api/$(fixture 'repos/acme/app/actions/workflows/ci.yml/runs?status=success&per_page=30&created=x&page=2')"
for args in "pull_request 1" "push 1"; do
  set -- $args
  cp "$tmp/api/$(fixture 'repos/acme/app/actions/workflows/ci.yml/runs?status=success&per_page=30&created=x&page=1')" \
    "$tmp/api/$(fixture "repos/acme/app/actions/workflows/ci.yml/runs?status=success&per_page=30&created=x&event=$1&page=$2")"
  echo '{"workflow_runs": []}' >"$tmp/api/$(fixture "repos/acme/app/actions/workflows/ci.yml/runs?status=success&per_page=30&created=x&event=$1&page=2")"
done
# jobs <run> <attempt> <job seconds> <checks step: success|skipped> [conclusion]: the job was
# created 100 s before it started (queue time, never counted) and its Checks step took 7 s.
jobs() {
  jq -n --argjson secs "$3" --arg checks "$4" --arg conclusion "${5:-success}" '
    def at($s): 1760000000 + $s | todate;
    {jobs: [{name: "CI", conclusion: $conclusion, created_at: at(-100), started_at: at(0), completed_at: at($secs),
      steps: [{name: "Set up job", conclusion: "success", started_at: at(0), completed_at: at(2)},
              {name: "Checks", conclusion: $checks, started_at: at(2), completed_at: (if $checks == "skipped" then at(2) else at(9) end)}]},
     {name: "other", conclusion: "success", started_at: at(0), completed_at: at(999), steps: []}]}' \
    >"$tmp/api/$(fixture "repos/acme/app/actions/runs/$1/attempts/$2/jobs?per_page=100")"
}
jobs 105 1 50 success
jobs 105 2 12 skipped
jobs 104 1 15 skipped
jobs 103 1 80 success
jobs 102 1 41 success
jobs 101 1 45 success
jobs 100 1 11 skipped
echo '[{"number": 2}]' >"$tmp/api/$(fixture repos/acme/app/commits/c102/pulls)"  # merged: the run lists no pull request
for pr in 1:1 2:1 3:3 5:1 9:2; do
  echo "{\"changed_files\": ${pr#*:}}" >"$tmp/api/$(fixture "repos/acme/app/pulls/${pr%:*}")"
done

measure() { # measure <baseline json> → results in $tmp/results.json, exit status of measure.sh
  echo "$1" >.github/fast-feedback.json
  rm -f "$tmp/results.json"
  "$assets/measure.sh" .github/fast-feedback.json "$tmp/results.json"
}

measure '{"margin": 0.25, "samples": 2, "scenarios": [
  {"id": "pr", "workflow": "ci.yml", "job": "ci", "event": "pull_request", "baseline_seconds": 40},
  {"id": "pr-one-file", "workflow": "ci.yml", "job": "ci", "event": "pull_request", "ran_steps": ["Checks"], "max_changed_files": 1, "baseline_seconds": 40},
  {"id": "nothing-to-check", "workflow": "ci.yml", "job": "ci", "event": "pull_request", "attempt": "any", "skipped_steps": ["Checks"], "baseline_seconds": 10},
  {"id": "push", "workflow": "ci.yml", "job": "ci", "event": "push", "baseline_seconds": 10},
  {"id": "checks-step", "workflow": "ci.yml", "job": "ci", "step": "Checks", "event": "pull_request", "baseline_seconds": 10}]}' 2>"$tmp/err" \
  || fail "measure.sh failed: $(cat "$tmp/err")"
r=$tmp/results.json
expect "a job's duration runs from its start to its end, never counting the queue (first attempts, newest first)" '.scenarios.pr.samples == [50, 80]' "$r"
expect "the median of a scenario's samples is recorded" '.scenarios.pr.median == 65' "$r"
expect "ran_steps and max_changed_files keep pull requests that ran the checks and touched few files, merged ones too" '.scenarios["pr-one-file"].samples == [50, 41]' "$r"
expect "skipped_steps keeps the runs that had nothing to check; attempt any reads reruns too" '.scenarios["nothing-to-check"].samples == [12, 11]' "$r"
expect "an event filter keeps only the runs of that event; too few samples is reported as not measured" '.scenarios.push.error | test("^1 of 2 samples")' "$r"
expect "a step scenario measures that step only" '.scenarios["checks-step"].samples == [7, 7]' "$r"
measure '{"margin": 0.25, "samples": 1, "scenarios": [
  {"id": "rerun", "workflow": "ci.yml", "job": "ci", "attempt": "rerun", "baseline_seconds": 10}]}' 2>"$tmp/err" \
  || fail "measure.sh failed: $(cat "$tmp/err")"
expect "attempt rerun measures only the attempts after the first" '.scenarios.rerun.samples == [12]' "$r"
# A workflow the pull request adds: GitHub knows it only once it is on the default branch.
printf 'on: [pull_request]\njobs:\n  measure:\n    runs-on: ubuntu-latest\n    steps:\n      - run: true\n' >.github/workflows/new.yml
touch "$tmp/api/$(fixture 'repos/acme/app/actions/workflows/new.yml/runs?status=success&per_page=30&created=x&page=1').404"
measure '{"margin": 0.25, "samples": 1, "scenarios": [
  {"id": "new", "workflow": "new.yml", "job": "measure", "todo": "ACME-4"},
  {"id": "rerun", "workflow": "ci.yml", "job": "ci", "attempt": "rerun", "baseline_seconds": 10}]}' 2>"$tmp/err" \
  || fail "measure.sh failed on a workflow GitHub does not know yet: $(cat "$tmp/err")"
expect "a workflow not yet on the default branch (404) is not measured, and the others still are" '(.scenarios.new.error | test("no runs of new.yml yet")) and .scenarios.rerun.samples == [12]' "$r"
rm .github/workflows/new.yml

# ---- commands ----
cat >"$tmp/slow.sh" <<'EOF'
#!/usr/bin/env bash
# Records each call, sleeps 0.2 s, and fails when told to.
echo x >>"$1"
[ -n "${FAST_FEEDBACK_MEASURE:-}" ] && { echo "measurement variables leaked into the command" >&2; exit 3; }
sleep 0.2
[ -z "${FAIL:-}" ]
EOF
chmod +x "$tmp/slow.sh"
echo '{"scenarios": {"warm": {"median": 0.5, "samples": [0.5]}}}' >"$tmp/previous.json"
FAST_FEEDBACK_MEASURE=$tmp/ignored FAST_FEEDBACK_PREVIOUS=$tmp/previous.json measure "{\"margin\": 0.25, \"samples\": 3, \"scenarios\": [
  {\"id\": \"warm\", \"command\": \"$tmp/slow.sh $tmp/calls\", \"warmup\": 2, \"baseline_seconds\": 1}]}" 2>"$tmp/err" \
  || fail "measure.sh failed on a command: $(cat "$tmp/err")"
[ "$(wc -l <"$tmp/calls" | tr -d ' ')" = 5 ] && ok "a command runs warmup + samples times" || fail "a command ran $(wc -l <"$tmp/calls") times, not 5"
expect "only the runs after the warmup are timed" '.scenarios.warm.samples | length == 3 and all(.[]; . >= 0.15 and . < 2)' "$r"
expect "the previous median is kept for the ratchet" '.scenarios.warm.previous_median == 0.5' "$r"
calls=$(wc -l <"$tmp/calls")
FAST_FEEDBACK_COMMANDS=skip measure "{\"margin\": 0.25, \"samples\": 1, \"scenarios\": [{\"id\": \"warm\", \"command\": \"$tmp/slow.sh $tmp/calls\", \"baseline_seconds\": 1}]}" 2>/dev/null \
  || fail "measure.sh failed with FAST_FEEDBACK_COMMANDS=skip"
[ "$(wc -l <"$tmp/calls")" = "$calls" ] && ok "FAST_FEEDBACK_COMMANDS=skip runs no command" || fail "FAST_FEEDBACK_COMMANDS=skip ran a command"
expect "a skipped command is reported as not measured" '.scenarios.warm.error | test("skipped")' "$r"
if FAIL=1 measure "{\"margin\": 0.25, \"samples\": 1, \"scenarios\": [{\"id\": \"broken\", \"command\": \"$tmp/slow.sh $tmp/calls\", \"warmup\": 0, \"baseline_seconds\": 1}]}" 2>/dev/null; then
  fail "a failing command is a measurement that cannot be taken (exit 2)"
else
  status=$?
  [ $status = 2 ] && ok "a failing command is a measurement that cannot be taken (exit 2)" || fail "a failing command exits $status, not 2"
fi

# ---- the ratchet's proposal ----
cat >.github/fast-feedback.json <<'EOF'
{"margin": 0.2, "samples": 5, "cap_seconds": 180, "scenarios": [
  {"id": "fast", "workflow": "ci.yml", "job": "ci", "baseline_seconds": 40},
  {"id": "once", "command": "true", "baseline_seconds": 40},
  {"id": "sharded", "command": "true", "baseline_seconds": 400, "over_cap": true, "todo": "ACME-1"},
  {"id": "still-over", "command": "true", "baseline_seconds": 400, "over_cap": true, "todo": "ACME-2"},
  {"id": "sub-second", "command": "true", "baseline_seconds": 0.9},
  {"id": "pending", "command": "true", "todo": "ACME-3: baseline once 10 samples exist"}]}
EOF
echo '{"scenarios": {
  "fast": {"median": 30.2, "previous_median": 33},
  "once": {"median": 30, "previous_median": 39},
  "sharded": {"median": 150, "previous_median": 160.2},
  "still-over": {"median": 300, "previous_median": 310},
  "sub-second": {"median": 0.52, "previous_median": 0.61},
  "pending": {"median": 20, "previous_median": 21}}}' >"$tmp/results.json"
status=0
FAST_FEEDBACK_RESULTS=$tmp/results.json FAST_FEEDBACK_PROPOSAL=$tmp/proposal.json "$assets/ratchet.sh" >"$tmp/out" || status=$?
[ "$status" = 1 ] && ok "the ratchet reports baselines that can be lowered (exit 1)" || fail "the ratchet exited $status, not 1"
p=$tmp/proposal.json
expect "a baseline under baseline × (1 − margin / 2) twice in a row is lowered to the higher median plus half the margin, rounded up" '.scenarios[0].baseline_seconds == 37' "$p"
expect "one fast night does not lower a baseline" '.scenarios[1].baseline_seconds == 40' "$p"
expect "a baseline brought under the cap loses over_cap and its todo" '.scenarios[2] == {"id": "sharded", "command": "true", "baseline_seconds": 177}' "$p"
expect "a baseline still over the cap keeps over_cap and its todo" '.scenarios[3].baseline_seconds == 341 and .scenarios[3].over_cap == true and .scenarios[3].todo == "ACME-2"' "$p"
expect "under 10 s a baseline is rounded up to the tenth of a second" '.scenarios[4].baseline_seconds == 0.7' "$p"
expect "a scenario without a baseline yet is never given one" '.scenarios[5] == {"id": "pending", "command": "true", "todo": "ACME-3: baseline once 10 samples exist"}' "$p"
expect "the proposal keeps every other field of the file" '.margin == 0.2 and .samples == 5 and .cap_seconds == 180' "$p"
FAST_FEEDBACK_BASELINE=$p "$assets/check.sh" >"$tmp/out" 2>&1 && ok "the proposal is a valid baseline file" || fail "the proposal is invalid: $(cat "$tmp/out")"

[ "$failures" -eq 0 ] || { echo "$failures case(s) failed"; exit 1; }
