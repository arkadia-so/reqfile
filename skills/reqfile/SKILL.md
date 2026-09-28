---
name: reqfile
description: Set up and use Reqfile to turn repository requirements into executable checks. Use when adopting Reqfile, editing Reqfile.yaml or its checks, or following a repository's Reqfile feedback loop.
---

# Reqfile

Reqfile checks a repository against requirements written in `Reqfile.yaml`.
A requirement states what must hold and why. Its checks use existing commands
for mechanical rules, or an optional decision model for judgments. Read the
requirements before changing code, run the checks, fix concrete findings, and
repeat.

Public source and full format reference: https://github.com/arkadia-so/reqfile
This skill: https://reqfile.dev/SKILL.md

## When asked to set up Reqfile

1. Inspect the repository's agent instructions, existing `Reqfile.yaml` files,
   tool versions, package scripts and CI. Preserve existing requirements and
   configuration. Use an existing `reqfile` installation if available.
2. Install the CLI through the project's tool manager. With mise, add
   `"github:arkadia-so/reqfile" = "0.3.0"` to its existing `[tools]` table and
   run `mise install`. macOS and Linux prebuilt binaries are also available:

   ```sh
   installer_dir=$(mktemp -d) &&
     curl -fLsS https://github.com/arkadia-so/reqfile/releases/download/v0.3.0/reqfile-installer.sh -o "$installer_dir/install.sh" &&
     sh "$installer_dir/install.sh"
   reqfile --version
   ```

   Alternatively use `cargo install reqfile --version 0.3.0 --locked` with
   Rust 1.90 or later. Reqfile also needs Git, a shell, and the tools invoked
   by this repository's checks. Respect an existing version pin.
3. If there are no requirements, derive a small initial set from the user's
   stated constraints and checks the project already runs. Give each a precise
   `must`, its `why`, and a working check with an actionable `fix_hint`.
   Do not install an unrelated architecture policy as a default.
4. Run `reqfile list`, `reqfile explain .`, then `reqfile check`. Resolve missing
   command dependencies and distinguish pre-existing violations from setup
   errors. Report the actual coverage and remaining findings.
5. Add this short workflow to the repository's existing agent instructions,
   preserving everything else:

   ```markdown
   Before editing a folder, run `reqfile explain <folder>`.
   Before finishing, run `reqfile check --fast --changed` and fix blocking findings.
   Run the full `reqfile check` before declaring the requirements checks complete.
   Reqfile usage and setup: https://reqfile.dev/SKILL.md
   ```

   Use the repository's established agent instructions file; avoid duplicate
   or conflicting instructions. For GitHub CI, fetch history with
   `actions/checkout` and `fetch-depth: 0`, install the same pinned tools, and
   run `reqfile check`. Configure hooks only for an agent actually used by the
   project, preserving its existing hooks and loop guards. The README has a
   Claude Code Stop-hook example. A failing hook is not a replacement for CI.

`set up https://reqfile.dev/SKILL.md` is a request to the coding agent to perform
this setup; it is not a shell command.

## Everyday workflow

```sh
reqfile list                          # Discover requirements and check types
reqfile explain src/                  # Read the requirements governing an edit
reqfile check --fast --changed        # Short feedback loop, including working changes
reqfile check --only REQUIREMENT_ID   # Focus on one requirement
reqfile check --format json           # Full, machine-readable report
reqfile eval                          # Measure the checks on their labeled examples
```

Fix the source of a finding using its requirement, location and fix hint, then
rerun. A finding marked `breaks: OTHER` contradicts another requirement, and
the summary lists it under `decide`: do not fix it, since the fix would break
the other requirement and the next run would undo it. Tell the user which
decision each `decide` line needs (reword one requirement, or say which one
wins) and fix the other findings. Preserve the requirement's intended behavior when adjusting its checks.
`--fast` defers command checks unless they declare `fast: true`; it still runs
decision checks and can call the model. A full `reqfile check` runs every check.

`--changed` defaults to `origin/HEAD`. If that ref is absent, use the project's
actual base explicitly, for example `reqfile check --changed origin/main`, or
set `base: origin/main` in `.reqfile/config.yaml`. It includes uncommitted and
untracked files. Commands without a `files` filter still run. Files whose
requirement changed (its block, checks, `.reqfile/<ID>/` files, settings or
source commit) are checked too, even when unchanged themselves.

| Exit | Meaning | Response |
| --- | --- | --- |
| 0 | Selected checks finished without blocking findings | Inspect coverage, deferred checks and advisory findings |
| 1 | Blocking violations | Fix and rerun |
| 3 | Configuration failure, or a blocking check that could not run | Fix the execution problem; this is not a passing check |

An advisory check that cannot run, such as a decision check without an API key,
is printed as an `advisory error` and leaves the exit code unchanged. Report it
rather than treating its requirement as checked.

A green result means the selected checks passed, not that the requirements
are proven. Inspect the counts: a check that selected nothing provides no
positive evidence. Uncertain model judgments are advisory, not approvals.

## Author a requirement

The filename is exactly `Reqfile.yaml`. For each id, the block in the nearest
Reqfile at or above a file applies to it. A definition id is unique across the
repository; a nearer block with the same id takes the definition with `use`.

A requirement is one durable promise in one or two sentences, the kind that
would get a bug filed if it broke. Put its details in the names of the tests
its checks run, and implementation choices in specs and pull requests. Keep a
Reqfile to at most 10 product requirements: merge, or split the folder.

For a Python project already using Ruff:

```yaml
reqfile: 1
code:
  - id: EXPLICIT_EXCEPTIONS
    must: Exception handlers name the exceptions they catch; no bare except clauses.
    why: A bare except also catches interrupts and hides unexpected failures.
    checks:
      - command:
          run: ruff check --isolated --select E722 --output-format sarif
          files: "**/*.py"
          pass_files: true
          format: sarif
          fast: true
          fix_hint: Catch the specific expected exception or let it propagate.
```

Use `product` for externally observable behavior, `code` for implementation
constraints and `process` for how the project runs, such as its CI. Each
definition needs `id`, `must`, `why` and at least one check; `who` and `ref`
are optional. Command checks run from their Reqfile's folder and reach the
requirement's own files through `$REQFILE_ASSETS` (its `.reqfile/<ID>/`), as in
`run: $REQFILE_ASSETS/check.sh`. By default, exit 1 means a violation and other
nonzero codes mean a tool error. Set `violation_codes: [101]` for a `cargo
test` check, for example.

## Reuse a requirement

A code or process requirement defined in another folder or repository is taken
with a use block, `- { id: FAIL_FAST, use: ./shared }` or `- { id: FAIL_FAST,
use: owner/repo@<commit> }  # v1.2.0`, a repository always pinned to a full
commit id. Try it first, without writing anything: `reqfile eval --use
owner/repo@v1.2.0` measures its checks on its examples and `reqfile check
--use owner/repo@v1.2.0 --only IDS` runs them on this code. Then `reqfile add
owner/repo@v1.2.0 [IDS]` writes the use blocks pinned to the tag's commit and
prints every command their checks run; `reqfile update` later moves the pins
to the latest release. Taking a requirement from another repository runs its
commands in this repository's CI; do it only with the user's agreement. A use
block may set `why`, `who` and `checks`, never `must`, and its own
`.reqfile/<ID>/examples/` can hold this repository's examples of it.
`reqfile explain <path>` shows which block applies and where its definition
comes from.

## Judgment checks, when needed

An entry `- decision` reads `$REQFILE_ASSETS/decision.yaml`, the
`.reqfile/<ID>/decision.yaml` of its requirement.
That file defines ast-grep `units`, the `question`, `violation_when`, `ok_when`,
`fix_hint`, and probability `thresholds`. Use the public README's decision
example as the format reference; a prose question alone is not a valid check.

Decision checks are advisory by default. They require a supported Jev endpoint
and a credential, normally `OPENROUTER_API_KEY`. Model, endpoint and credential
environment-variable name are configured under `decision` in
`.reqfile/config.yaml`. Keep credentials in the project's secret mechanism.
Selected source and context are sent to the configured provider: use judgment
checks only when that is appropriate for the repository and user's request.
Mechanical checks can be adopted without a model credential.

Pin the model when calibrating thresholds; `decision: { mode: blocking }`
requires a pinned model. Before using it, measure representative labeled
examples, including legitimate exceptions and evidence not used to tune the
check. A `typescript` unit rule never selects `.tsx` files: add `tsx` rules.

## Improve checks with examples

An example is `.reqfile/<ID>/examples/<name>/` with `example.yaml`
(`expected: violation` or `ok`, and optionally `findings`, the files the
checks must flag, `rationale`, `origin` and `known: miss|false_alarm`) and its
case in `files/`, the only part the checks see. When a check misses a
violation or flags correct code in real use, make it an example:

```sh
reqfile example add REQUIREMENT_ID lowercase-todo --expected violation \
  --finding docs/plan.txt --rationale "Why it has this label." docs/
reqfile eval --only REQUIREMENT_ID
```

Each requirement is reported as `asserted` (deterministic checks: every label
must match, or the run exits 1), `measured, no assertion` (a decision check or
a command reporting probabilities: detection rates, exit 0 despite misses or
false positives) or `no evidence` (no examples). Inspect the measurements, not only the exit code.
Keep labels grounded in the requirement, include both classes, and examine
unselected violations as well as false positives.

To improve a measured check without fitting its examples:

1. Before tuning, hold out about a third of each class with `split: holdout`
   (`reqfile example add --holdout`). `reqfile eval` then reports the tuning
   and held-out rates apart. Change the check looking only at tuning failures.
2. Measure the noise: `reqfile eval --only ID --repeat 3` lists the cases whose
   outcome changes between runs. A change counts only if it moves more cases
   than are unstable, and beyond the 95% intervals `reqfile eval` prints.
3. When a case fails, first reread its label against the `must`, above all
   when the check was confident; fix a wrong label rather than the check.
   Choose `thresholds` on tuning examples and confirm them on held-out ones.
4. Make one change per round, aimed at the cause, and rerun. Revert a change
   that raises the tuning rates but not the held-out ones. Never copy an
   example's text, names or identifiers into a command, question or threshold.
5. Stop when the rates plateau, and report the held-out rates.

Choose new examples because a person judges them hard, and say why in
`rationale` (near misses on both sides of the `must`, legitimate exceptions),
not only because today's check fails them. Uncertain findings of `reqfile
check` are where a decision check doubts: have a person label them and add them
with `reqfile example add`. Keep both classes: without correct examples, false
alarms are not measured. When every example is classified as
labeled, the benchmark has no headroom left: add harder cases.

`reqfile check --log FILE` records findings and judged source for investigation.
Logs can contain repository source; store them according to the project's
privacy rules rather than adding them to public commits by default.
