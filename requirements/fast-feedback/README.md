# FAST_FEEDBACK

> The feedback loop's latency never goes up; it only ratchets down.

A requirement package for [reqfile](https://reqfile.dev): the repository names
the waits of its feedback loop (a pull request's CI job, a rerun, the local
check command), records how long each takes today, and from then on a
scenario that gets slower than its baseline plus a margin fails the check,
while one that stays faster lowers its baseline for good.

```yaml
# Reqfile.yaml
process:
  - { id: FAST_FEEDBACK, use: <this folder or repository> }
```

## What it checks

`.github/fast-feedback.json` (`$FAST_FEEDBACK_BASELINE` to read another file)
declares the scenarios. The check fails when:

- the file is missing or invalid;
- a job of a workflow that runs on pull requests (`pull_request` or
  `pull_request_target`) is measured by no scenario, or a scenario names a
  workflow or job that does not exist;
- a baseline is over the cap (default 180 s) without `over_cap: true` and a
  `todo` naming the work that brings it under;
- with measurements, a scenario's median is over
  `baseline × (1 + margin)`, or over the cap when it is not marked `over_cap`.

Its advisory second check, the ratchet, reports each scenario whose median was
under `baseline × (1 − margin / 2)` on two measurements in a row, and the
baseline to lower it to: the higher of the two medians plus half the margin,
rounded up (to the second, to the tenth under 10 s). The half margin is
headroom: a median of N samples is noisy too, and in simulation a baseline set
at the medians of a lucky pair of nights made ordinary nights fail on a third
to a half of them (below). A baseline brought under the cap loses `over_cap`
and its `todo`.
Nothing raises a baseline: that is a hand edit, with the reason in the pull
request.

Without measurements (every pull request, `reqfile check`), only the file and
the coverage are checked: it costs nothing and needs no network.

## The baseline file

```json
{
  "margin": 0.25,
  "samples": 7,
  "cap_seconds": 180,
  "scenarios": [
    {
      "id": "pr-docs-only",
      "describe": "A pull request with nothing to check",
      "workflow": "ci.yml",
      "job": "ci",
      "event": "pull_request",
      "skipped_steps": ["Checks"],
      "baseline_seconds": 13
    },
    {
      "id": "full-run",
      "workflow": "ci.yml",
      "job": "ci",
      "event": "schedule",
      "baseline_seconds": 422,
      "over_cap": true,
      "todo": "ACME-1: shard the full run"
    },
    {
      "id": "check-warm",
      "command": "make check",
      "warmup": 1,
      "baseline_seconds": 2
    }
  ]
}
```

| Field | Meaning |
|---|---|
| `margin` | Required. The noise allowed over a baseline, as a fraction; a scenario may set its own. Derive it from data (below). |
| `samples` | Required. N: a scenario's measurement is the median of N samples. |
| `cap_seconds` | The most any loop may take (default 180); a scenario may set its own. |
| `max_age_days`, `max_runs` | How far back a job scenario reads the history: successful runs of the last 14 days, at most 60 runs scanned. |
| `$comment` | Free text. |

A scenario has an `id` (lowercase words and dashes), a `baseline_seconds`, an
optional `describe`, `margin`, `cap_seconds`, `over_cap` and `todo`, and either:

- a **command**, run `warmup` times (default 1) untimed, then N times timed, on
  the machine measuring (the nightly runner); or
- a **job**: `workflow` (file name in `.github/workflows/`) and `job` (its id),
  measured in the GitHub Actions history from the runner picking the job up to
  its end, so queue time never counts. Filters choose which runs are samples:
  `event` (one or a list), `branch`, `attempt` (`first`, the default, `rerun`
  or `any`), `skipped_steps` and `ran_steps` (steps that must have been skipped
  or have run), `max_changed_files` (pull requests touching at most that many
  files), and `step` to time one step instead of the job. `job_name` overrides
  the display name used to find the job in a run.

A scenario may leave out `baseline_seconds` while there are too few samples to
set one (fewer than 10 since the job last changed, say), with a `todo` saying
when it gets one. It covers its job, it is measured and its median reported
(`no baseline yet:`), but it never fails the check and the ratchet never sets
its baseline: that is a hand edit once the samples exist. `over_cap` needs a
baseline.

## Measuring

The check measures when `FAST_FEEDBACK_MEASURE=<results file>` is set: job
scenarios through `gh` (with `GH_TOKEN`, `actions: read` and, for
`max_changed_files`, `pull-requests: read`), commands where it runs. A scenario
with fewer than N samples in the history is reported as not measured, never
as a failure; so is one whose workflow GitHub has no runs of yet, such as the
measuring workflow in the pull request that adds it. It writes the results, then checks them:

```json
{ "measured_at": "…", "samples": 7,
  "scenarios": { "pr-docs-only": { "samples": [12, 11, 15, 13, 14, 12, 16], "median": 13, "previous_median": 14 } } }
```

| Variable | Effect |
|---|---|
| `FAST_FEEDBACK_MEASURE` | Measure every scenario, write the results there, check them |
| `FAST_FEEDBACK_RESULTS` | Check these results (else `.github/fast-feedback-results.json` if present) |
| `FAST_FEEDBACK_PREVIOUS` | The previous measurement: its medians become `previous_median`, for the ratchet |
| `FAST_FEEDBACK_SAMPLES` | N for this run only (a noise study) |
| `FAST_FEEDBACK_COMMANDS=skip` | Measure job scenarios only, reporting commands as not measured: a cheap run on pull requests |
| `FAST_FEEDBACK_PROPOSAL` | The ratchet writes the baseline file with lowered baselines there |

Exit codes: 0 holds, 1 violations, 2 cannot measure (gh failing, a command
failing).

A nightly workflow, after the runs it measures:

```yaml
on:
  schedule: [{ cron: "30 5 * * *" }]
  pull_request: { paths: [".github/**"] }
permissions: { contents: read, actions: read, pull-requests: read }
jobs:
  measure:
    runs-on: ubuntu-latest
    env: { GH_TOKEN: "${{ github.token }}" }
    steps:
      - uses: actions/checkout@v4
      # (install what the command scenarios need)
      - if: github.event_name == 'schedule'
        run: |   # the previous night's results, for the ratchet
          id=$(gh run list -w measure.yml -e schedule -s success -L 1 --json databaseId --jq '.[0].databaseId')
          [ -z "$id" ] || gh run download "$id" -n fast-feedback-results -D previous
      - run: reqfile check --only FAST_FEEDBACK
        env:
          FAST_FEEDBACK_MEASURE: fast-feedback-results.json
          FAST_FEEDBACK_PREVIOUS: previous/fast-feedback-results.json
      - uses: actions/upload-artifact@v4
        if: always()
        with: { name: fast-feedback-results, path: fast-feedback-results.json }
```

The measuring job runs on pull requests, so it needs a scenario of its own;
`FAST_FEEDBACK_COMMANDS=skip` keeps it cheap there. On a pull request the job
history is main's, so that run checks the scenarios and baselines against
reality rather than the change itself.
The ratchet then runs the check again with `FAST_FEEDBACK_RESULTS` and
`FAST_FEEDBACK_PROPOSAL`, and lands the proposal when there is one. A pull
request is the wrong vehicle with `GITHUB_TOKEN`: one it creates triggers no
workflow, so it never gets its checks, and creating it needs "Allow GitHub
Actions to create and approve pull requests". Commit the proposal to the
default branch instead (`contents: write`), after checking that it only lowers
baselines: every other field and scenario unchanged, `over_cap` and `todo`
dropped only under the cap. Name each scenario, old → new, and the run ids in
the commit message, and when the branch moved meanwhile, redo the lowering on
it and push once more. The next full CI run checks the commit. Arkadia's
`.github/workflows/fast-feedback-ratchet.yml` and
`scripts/fast-feedback-ratchet.sh` do this (an app token, which does trigger
workflows, would allow a pull request instead).

## Choosing the margin and N

The margin must exceed the noise of an unchanged repository, or the check
cries wolf; it must stay small, or slowdowns pass. Derive it from data:

1. Take at least 10 samples of each scenario on an unchanged repository
   (`FAST_FEEDBACK_SAMPLES=12` on a dispatched run, probe pull requests,
   reruns).
2. For each, compute the CV (standard deviation over mean) and the 95th
   percentile of `sample / median`.
3. `margin = max(20%, the highest p95 excess, rounded up to 5%)`.
4. Choose N so that, with nothing changed, fewer than 5% of nights fail, all
   scenarios together. Simulate the nights with the ratchet: a year of
   nights, each the median of N samples drawn from a scenario's own samples,
   failing over `baseline × (1 + margin)`, lowering the baseline as above.
   Give a scenario its own margin when the simulation needs it.
5. A scenario needing more than 50% is not measuring the loop but its noise:
   measure a stable step (`step`) or a narrower set of runs (`ran_steps`,
   `skipped_steps`, `attempt`) instead.

### Arkadia, 2026-10-09

Job durations from `started_at` to `completed_at` (queue excluded), on
`ubuntu-latest`: 11 pushes to a probe pull request changing Markdown (each
rerun once for the cached rerun), 10 reruns of a push to main, dispatched full
runs on main, 9 reruns of tween's job and of this measurement on a pull
request, the history of the private evaluations; the command scenarios 12
times on a runner of fast-feedback.yml (`FAST_FEEDBACK_SAMPLES=12`), and
`check:code` warm on three runners and on a Mac. "Needs" is
`max(20%, p95 excess rounded up to 5%)`; "Margin" is what
`.github/fast-feedback.json` uses.

| Scenario | n | Median | CV | p95 of sample/median | Needs | Margin |
|---|---|---|---|---|---|---|
| pr-docs-only | 12 | 12.5 s | 20.2% | +23.6% | 25% | 25% |
| pr-one-package-file (command) | 12 | 12.2 s | 2.6% | +4.0% | 20% | 25% |
| rerun-all-cached | 11 | 13 s | 17.2% | +23.1% | 25% | 25% |
| push-main | 13 | 15 s | 21.2% | +16.0% | 20% | 35% (simulation) |
| full-cold, main (over the cap) | 6 | 376.5 s | 21.1% | +29.8% | 30% | 40% (simulation) |
| reqfile-evals-verify (over the cap) | 20 | 252.5 s | 11.4% | +7.0% | 20% | 25% |
| tween-standalone, job | 14 | 76.5 s | 24.3% | +61.9% | 65% | not measured: over 50% |
| tween-standalone, check step | 14 | 31.5 s | 13.3% | +14.4% | 20% | 25% |
| check-code-warm, 3 runners | 36 | 0.39 s | 28.8% | +41.3% | 45% | 50% (simulation) |
| fast-feedback-measure | 10 | 68.5 s | 27.0% | +46.6% | 50% | 50% |
| check-code-warm, Mac | 12 | 0.84 s | 3.5% | +3.7% | 20% | (not measured nightly) |
| one-file pull request, ci job | 10 | 57 s | 11.4% | +17.1% | 20% | (not measured: below) |

**Default margin: 25%**, the highest need of the scenarios that share it
(pr-docs-only, +23.6%: whole-second rounding and runner start-up on 8 to 16 s).
A 20% default for the scenarios needing 20% would raise the false alarms below
from 2.5% to 4.6% of nights, too close to 5%.

**N = 7.** Share of nights with a false alarm, all scenarios together, in a
simulation of the ratchet: 40 years of nights, each the median of N samples
drawn from a scenario's own samples (from its runner's samples, for
check-code-warm: one runner per night), failing over `baseline × (1 + margin)`,
lowered as above.

| N | Ratchet to the higher median | Ratchet to it + half the margin |
|---|---|---|
| 5 | 84% | 13.6% |
| 7 | 72% | 2.5% |

Without the ratchet, a fixed baseline at 25% fails 0.1% (resampling) to 5%
(log-normal fit) of nights at N = 7. Scenarios read overlapping windows of the
history from one night to the next, so a ratchet's two nights share samples;
the simulation draws them independently.

Three scenarios needed more than their p95 in that simulation, all for the
runner drawn: `ubuntu-latest` is not one machine.

- **push-main**: 10 to 13 s or 15 to 18 s. 11% of nights at 25% or 30%, 0.7%
  at 35%.
- **full-cold**: on the same commit, minutes apart, its Checks step took 205 to
  224 s or 311 to 338 s. Once the ratchet lowers the baseline to the fast mode,
  a slow night is +35%; 40% keeps it at 0%. Its scenario reads main only: a
  pull request reshaping ci.yml (shards) would otherwise feed it 2 s samples.
  The fix is ARK-1's (sharding, the remote cache), or a runner of a fixed size.
- **check-code-warm**: 0.26, 0.39 or 0.54 s by runner, each within 3%. 35%
  fails 15% of nights, 50% none. Three runners are few: watch its first nights.

**Over 50%:** the tween standalone job (+62%) is bimodal, 66 to 82 s or 120 to
131 s, depending on whether the Flutter SDK comes from its cache (24 s against
60 s). Its scenario measures the `check-standalone` step instead (+14%).
fast-feedback-measure sits at 50%: one run in 10, the first of a new head,
installed the Rust toolchain (122 s against 56 to 74 s). The fix, if it fires:
skip the toolchain on pull requests, whose measurement runs no command.

**Not measurable from the history:** a pull request changing one file of a
package is rare here (none in 14 days but the probe's), so that scenario runs
its checks as a command (`scripts/fast-feedback-one-file.sh`, 12.2 s on a
runner); the job around them (setup, about 40 s) is pr-docs-only's.
