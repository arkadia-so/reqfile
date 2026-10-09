#!/usr/bin/env bash
# measure.sh <baseline file> <results file>: measures every scenario of FAST_FEEDBACK and writes
# the results (README of FAST_FEEDBACK, "Results"). Run by check.sh when FAST_FEEDBACK_MEASURE
# is set.
#   - A job scenario takes the durations of its job (or of one `step`) in the GitHub Actions
#     history, newest first: successful runs of the last `max_age_days`, at most `max_runs` runs
#     scanned, from the runner picking the job up to its end (queue time excluded), until it has
#     `samples` of them. With fewer, the scenario is reported as not measured.
#   - A command scenario runs its command `warmup` times, then `samples` times timed.
# FAST_FEEDBACK_SAMPLES overrides `samples` (for a noise study); FAST_FEEDBACK_COMMANDS=skip
# measures job scenarios only (a cheap run on pull requests); FAST_FEEDBACK_PREVIOUS, the
# previous results, adds each scenario's previous median for the ratchet. Needs gh (with
# GH_TOKEN in CI) for job scenarios. Exits 2 when a measurement cannot be taken: gh or the API
# failing, or a command failing.
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
. "$here/lib.sh"
baseline=$1 out=$2
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

top() { jq -r "$1" "$baseline"; }
want=${FAST_FEEDBACK_SAMPLES:-$(top .samples)}
max_age_days=$(top '.max_age_days // 14')
max_runs=$(top '.max_runs // 60')
since=$(jq -rn --argjson days "$max_age_days" 'now - $days * 86400 | strftime("%Y-%m-%dT%H:%M:%SZ")')
previous=${FAST_FEEDBACK_PREVIOUS:-}
[ -z "$previous" ] || [ -f "$previous" ] || previous=""
: >"$tmp/results.jsonl"

# Seconds since the epoch, to the millisecond, with what the shell or the system has.
now() {
  if [ -n "${EPOCHREALTIME:-}" ]; then echo "${EPOCHREALTIME/,/.}"
  else perl -MTime::HiRes=time -e 'printf "%.3f\n", time'; fi
}

record() { # record <id> <json object>
  jq -cn --arg id "$1" --argjson value "$2" '{($id): $value}' >>"$tmp/results.jsonl"
}

samples_json() { printf '%s\n' "$@" | jq -s 'map(tonumber)'; }

repo=""
if jq -e 'any(.scenarios[]; has("workflow"))' "$baseline" >/dev/null; then
  command -v gh >/dev/null || { echo "gh is required to measure job scenarios" >&2; exit 2; }
  repo=${GITHUB_REPOSITORY:-$(gh repo view --json nameWithOwner --jq .nameWithOwner)}
fi

# A GitHub API response, cached for the run: several scenarios read the same runs. Returns 4
# when GitHub answers 404; any other failure ends the measurement (exit 2).
api() {
  local key
  key=$tmp/api-$(printf '%s' "$1" | cksum | cut -d' ' -f1)
  if [ ! -f "$key" ] && ! gh api "$1" >"$key" 2>"$tmp/api-error"; then
    rm -f "$key"
    grep -q "HTTP 404" "$tmp/api-error" && return 4
    cat "$tmp/api-error" >&2
    echo "gh api $1 failed" >&2
    exit 2
  fi
  cat "$key"
}

measure_job() { # measure_job <scenario json>
  local s=$1 id workflow job name step events branch attempt max_changed query page=1 scanned=0
  local -a samples=()
  id=$(jq -r .id <<<"$s")
  workflow=$(jq -r .workflow <<<"$s")
  job=$(jq -r .job <<<"$s")
  name=$(jq -r '.job_name // empty' <<<"$s")
  [ -n "$name" ] || name=$(workflow_jobs ".github/workflows/$workflow" | awk -F'\t' -v j="$job" '$2 == j { print $3 }')
  step=$(jq -r '.step // empty' <<<"$s")
  events=$(jq -c '[.event // empty] | flatten' <<<"$s")
  branch=$(jq -r '.branch // empty' <<<"$s")
  attempt=$(jq -r '.attempt // "first"' <<<"$s")
  max_changed=$(jq -r '.max_changed_files // empty' <<<"$s")
  query="repos/$repo/actions/workflows/$workflow/runs?status=success&per_page=30&created=%3E%3D$since"
  [ -z "$branch" ] || query+="&branch=$branch"
  [ "$(jq length <<<"$events")" != 1 ] || query+="&event=$(jq -r '.[0]' <<<"$events")"

  # A workflow GitHub does not know yet (added by the pull request being checked, not on the
  # default branch) has no history: not measured, like any scenario with too few samples.
  local known=0
  api "$query&page=1" >/dev/null || known=$?
  if [ "$known" = 4 ]; then
    record "$id" "$(jq -cn --arg e "GitHub has no runs of $workflow yet (not on the default branch)" '{error: $e}')"
    return 0
  fi

  while [ ${#samples[@]} -lt "$want" ] && [ "$scanned" -lt "$max_runs" ]; do
    local runs
    runs=$(api "$query&page=$page" | jq -c --argjson events "$events" \
      '.workflow_runs[] | select(($events | length) == 0 or (.event as $e | $events | index($e))) | {id, run_attempt, head_sha, prs: [.pull_requests[]?.number]}')
    [ -n "$runs" ] || break
    local run
    while IFS= read -r run; do
      [ ${#samples[@]} -lt "$want" ] && [ "$scanned" -lt "$max_runs" ] || break
      scanned=$((scanned + 1))
      local run_id last first a pr
      run_id=$(jq -r .id <<<"$run")
      last=$(jq -r .run_attempt <<<"$run")
      case $attempt in first) first=1 last=1 ;; rerun) first=2 ;; any) first=1 ;; esac
      if [ -n "$max_changed" ]; then
        local changed
        pr=$(jq -r '.prs[0] // empty' <<<"$run")
        # A run lists its pull request only while it is open: a merged one is found by its head.
        [ -n "$pr" ] || pr=$(api "repos/$repo/commits/$(jq -r .head_sha <<<"$run")/pulls" | jq -r '.[0].number // empty')
        [ -n "$pr" ] || continue
        changed=$(api "repos/$repo/pulls/$pr" | jq .changed_files)
        [ "$changed" -le "$max_changed" ] || continue
      fi
      for ((a = last; a >= first; a--)); do
        local secs
        secs=$(api "repos/$repo/actions/runs/$run_id/attempts/$a/jobs?per_page=100" | jq -r \
          --arg name "$name" --arg step "$step" --argjson s "$s" '
          def secs: if .started_at and .completed_at then (.completed_at | fromdate) - (.started_at | fromdate) else null end;
          def conclusion($n): first(.steps[]? | select(.name == $n) | .conclusion) // "absent";
          [ .jobs[]
            | select(.name == $name or (.name | startswith($name + " (")))
            | select(.conclusion == "success")
            | select(. as $j | all(($s.skipped_steps // [])[]; . as $n | $j | conclusion($n) == "skipped"))
            | select(. as $j | all(($s.ran_steps // [])[]; . as $n | $j | conclusion($n) == "success"))
            | if $step == "" then secs else first(.steps[]? | select(.name == $step and .conclusion == "success") | secs) // null end
            | select(. != null) ] | first // empty')
        [ -z "$secs" ] || samples+=("$secs")
        [ ${#samples[@]} -lt "$want" ] || break
      done
    done <<<"$runs"
    page=$((page + 1))
  done

  if [ ${#samples[@]} -lt "$want" ]; then
    record "$id" "$(jq -cn --arg e "${#samples[@]} of $want samples in the runs of the last $max_age_days days ($scanned scanned)" '{error: $e}')"
  else
    record "$id" "$(jq -cn --argjson samples "$(samples_json "${samples[@]}")" '{samples: $samples}')"
  fi
}

measure_command() { # measure_command <scenario json>
  local s=$1 id command warmup i start end log=$tmp/command.log
  local -a samples=()
  id=$(jq -r .id <<<"$s")
  command=$(jq -r .command <<<"$s")
  warmup=$(jq -r '.warmup // 1' <<<"$s")
  for ((i = 0; i < warmup + want; i++)); do
    start=$(now)
    # Without this check's variables: a command running `reqfile check` must not measure again.
    if ! env -u FAST_FEEDBACK_MEASURE -u FAST_FEEDBACK_RESULTS -u FAST_FEEDBACK_PREVIOUS \
      -u FAST_FEEDBACK_PROPOSAL -u FAST_FEEDBACK_SAMPLES -u FAST_FEEDBACK_COMMANDS bash -c "$command" >"$log" 2>&1; then
      echo "scenario $id: \`$command\` failed; its last lines:" >&2
      tail -n 30 "$log" >&2
      exit 2
    fi
    end=$(now)
    [ "$i" -lt "$warmup" ] || samples+=("$(jq -n --argjson a "$start" --argjson b "$end" '($b - $a) * 1000 | round / 1000')")
  done
  record "$id" "$(jq -cn --argjson samples "$(samples_json "${samples[@]}")" '{samples: $samples}')"
}

while IFS= read -r scenario; do
  if ! jq -e 'has("command")' <<<"$scenario" >/dev/null; then
    measure_job "$scenario"
  elif [ "${FAST_FEEDBACK_COMMANDS:-run}" = skip ]; then
    record "$(jq -r .id <<<"$scenario")" '{"error": "commands skipped in this run (FAST_FEEDBACK_COMMANDS=skip)"}'
  else
    measure_command "$scenario"
  fi
done < <(jq -c '.scenarios[]' "$baseline")

jq -s --argjson want "$want" --slurpfile previous "${previous:-/dev/null}" '
  def median: sort | if length % 2 == 1 then .[length / 2 | floor] else (.[length / 2 - 1] + .[length / 2]) / 2 end;
  ($previous[0].scenarios // {}) as $before
  | { measured_at: (now | strftime("%Y-%m-%dT%H:%M:%SZ")), samples: $want,
      scenarios: (add // {} | with_entries(
        .key as $id
        | .value |= (if .samples then . + {median: (.samples | median)} else . end
                     | if $before[$id].median then . + {previous_median: $before[$id].median} else . end))) }
' "$tmp/results.jsonl" >"$out"
