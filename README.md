# reqfile

Check that a codebase meets its requirements, so agents and humans get
concrete, fixable violations instead of vague advice.

Requirements live in `Reqfile.yaml` files next to the code. Each one says what
must hold, why, for whom, and how it is checked: with an existing tool when the
rule is mechanical (`command`), or with [Jev](https://docs.typesafe.ai), a
decision model returning calibrated probabilities, when it needs judgment
(`decision`).

```
$ reqfile check
FAIL_FAST  services/api/handler.py:42  Do not use bare `except` (E722)
  fix: Catch the specific expected error and handle it visibly, or let it propagate.
advisory  DECOMPLECT  services/sync/worker.ts:88  p=0.91  It mixes several jobs.
  fix: Split the I/O from the logic into two functions.

14 checks run, 2 with nothing to check, 212 code units judged. 1 violation, 1 advisory finding, 0 errors.
```

## Install

With [mise](https://mise.jdx.dev), from the prebuilt binaries (macOS and Linux):

```toml
# mise.toml
[tools]
"github:arkadia-so/reqfile" = "0.1"
```

Or with the install script, `cargo binstall reqfile`, or from source with
`cargo install reqfile --locked` (Rust 1.90 or later):

```sh
curl -LsSf https://github.com/arkadia-so/reqfile/releases/latest/download/reqfile-installer.sh | sh
```

reqfile needs `git`, `sh`, and the tools your Reqfiles' commands call.

## Reqfile.yaml

```yaml
# Requirements for this folder, checked by reqfile (https://reqfile.dev)
reqfile: 1

code:
  - id: FAIL_FAST
    ref: Shore, "Fail Fast"
    must: No silent fallbacks, no swallowed errors, no default values that hide a failure.
    why: A swallowed error is an invariant violation nobody sees.
    who: Anyone debugging production.
    checks:
      - command:
          run: ruff check --isolated --select E722,BLE001,S110 --output-format sarif
          files: "**/*.py"
          pass_files: true
          format: sarif
          fix_hint: Catch the specific expected error and handle it visibly, or let it propagate.
      - decision
```

- The file is named exactly `Reqfile.yaml` and has a `reqfile: 1` version key.
- Requirements are listed under `product` (what users get) or `code` (how the
  code is built). Each has an `id` in SCREAMING_SNAKE_CASE, `must`, `why` and
  at least one check, and optionally `who` and `ref`.
- Unknown keys, missing fields and near-miss file names (`reqfile.yaml`,
  `Reqfile.yml`) are errors naming the file and line. A near-miss name is only
  an error when the file has a top-level `reqfile`, `product` or `code` key, so
  a CI workflow named `reqfile.yml` is fine.

### Cascade

Any folder can hold a `Reqfile.yaml`. The requirements that apply to a file are
those of every Reqfile from its folder up to the repository root, and a
requirement only checks files under its own folder. Ids are unique across the
repository.

Target files come from git (tracked and untracked, never ignored), minus
`.reqfile/` folders and the `exclude` globs of the configs above them.

### Command checks

| Field | Required | Meaning |
|---|---|---|
| `run` | yes | Shell command, run from the folder of its Reqfile |
| `fix_hint` | yes | One line telling how to fix a violation |
| `files` | no | Glob relative to that folder; the command runs only if a target file matches |
| `pass_files` | no | Append the matching files as arguments (default false) |
| `format` | no | `exit` (default) or `sarif` |
| `violation_codes` | no | Exit codes meaning "violations found" (default `[1]`; `[101]` for `cargo test`) |
| `timeout` | no | Seconds, default 60 |
| `fast` | no | Quick enough for `reqfile check --fast`, such as in an edit hook (default false) |

Exit 0 passes. An exit code listed in `violation_codes` means violations: one
per SARIF result with `format: sarif`, otherwise one violation carrying the last
50 lines of output. Any other exit code, a signal or a timeout is a tool error.

### Decision checks

A `decision` check (or `decision: { mode: blocking }`) reads
`.reqfile/<ID>/decision.yaml` next to its Reqfile:

```yaml
units:                      # ast-grep rules selecting the code to ask about
  - { language: typescript, rule: { kind: catch_clause } }
  - { language: python, rule: { kind: except_clause } }
  - { language: rust, rule: { pattern: $X.unwrap_or_default() } }
context: enclosing          # optional: also send the enclosing function
question: Does this error handler hide the failure?
violation_when: It discards the error, only logs it, or replaces it with a default value, and execution continues as if nothing failed.
ok_when: It rethrows, returns an explicit error, or handles a specific expected error in a way the caller can observe.
fix_hint: Rethrow, return an explicit error, or narrow the handler to the expected error and handle it visibly.
thresholds: { violation_above: 0.8, pass_below: 0.2 }
```

Units accept ast-grep's `rule`, `constraints` and `utils`, plus `files` and
`ignores` globs (relative to the Reqfile's folder). For each unit, reqfile asks
Jev the question with the language, path, unit source and, with
`context: enclosing`, the enclosing function. Each unit is sent once, with the
questions of every decision check that selects it: Jev charges input tokens
and reads the state once per request, so several checks on the same code cost
little more than one. Answers are cached in a local file in the git folder (`.git/reqfile/jev-cache`),
so checking never modifies tracked files. An answer is reused while the unit,
the question and the exact model behind `latest` are unchanged, and a full run
drops answers no unit uses any more. In CI, keep that file between runs (for
example with `actions/cache`). A probability above
`violation_above` is a violation, below `pass_below` a pass, and anything in
between an uncertain finding.

Decision checks are advisory unless `mode: blocking`; uncertain findings are
always advisory. Label a few dozen units of your codebase and adjust the
thresholds before making a check blocking.

### Examples: `reqfile test`

Each requirement can keep labeled examples in `.reqfile/<ID>/examples/`: one
folder per case, named `violation-…` or `ok-…`, holding a small file tree.
`reqfile test` copies each case into a fresh repository with the Reqfile, the
requirement's `.reqfile/<ID>/` and its Jev settings, and runs the requirement's
real checks on it.

```
UNUSED_CODE  2 examples, 2 as labeled
FAIL_FAST  6 examples, measured: 3 of 4 violations caught (1 missed, 0 not selected, 0 uncertain); 0 of 2 correct examples flagged (0 uncertain)
  violation-rust-ok-discard: missed
```

Requirements with only command checks must match every label, or `reqfile
test` exits 1. With a decision check, results are measured instead: model
judgments vary, and a detector earns its place, or `mode: blocking`, by the
rates it reaches on examples it was not tuned on. Violations no check looked at
are reported as "not selected", apart from those it looked at and missed.

## Settings: `.reqfile/config.yaml`

Any folder can hold a `.reqfile/config.yaml`, with or without a `Reqfile.yaml`
next to it. Nothing is specific to the repository root: the settings of a
folder come, key by key, from the nearest config in that folder or its
ancestors, and each check uses the settings of its Reqfile's folder. Every key
is optional, and most repositories need no config at all.

```yaml
base: origin/main                      # default base of --changed (default: origin/HEAD)
exclude: ["vendor/**", "tests/fixtures/**"]   # relative to this folder

decision:                              # defaults shown
  model: "~typesafe/jev-latest"        # the latest Jev on OpenRouter
  api_key_env: OPENROUTER_API_KEY
  endpoint: https://openrouter.ai/api/v1
  concurrency: 16
```

`exclude` globs add up down the tree, so a nested config cannot re-include
what a parent excluded; a config inside an excluded folder is ignored.

Decision checks follow the latest Jev by default. Set `model` to an exact
version (such as `typesafe/jev-1.13-20260917`) to keep thresholds tied to the
version they were chosen with. The endpoint also accepts TypeSafe's own API
(`https://api.typesafe.ai/v1`, model `jev-latest`).

## CLI

```
reqfile check [--fast] [--changed [BASE]] [--only ID,...] [--format summary|json]
              [--log FILE [--log-tag KEY=VALUE]...]
reqfile explain <PATH>
reqfile test [--only ID,...]
```

- `--changed` restricts file-based checks (commands with `files`, decision
  units) to files changed since the merge-base of BASE and HEAD, including
  uncommitted, untracked and deleted files. Commands without `files` still run.
  A requirement whose definition changed (its Reqfile, its `.reqfile/<ID>/`
  files, or a config above it) is checked on its whole scope.
- `--fast` runs only decision checks and the commands declared `fast: true`,
  for edit hooks; the summary counts the checks left for a full run. Without
  it, every check runs, so a CI running `reqfile check` never skips checks.
- `--only` runs only the listed requirements.
- `--log FILE` appends one JSON line per judged code unit (clean ones
  included), command check and command finding: run id, git HEAD, requirement,
  location, the unit's source and fingerprint, the question's fingerprint,
  probability, verdict, exact model and whether the answer was cached, plus
  any `--log-tag KEY=VALUE`. It feeds evaluation and replay without changing
  results.
- `explain` lists the requirements that apply to a path, each with its Reqfile.
- `list` lists every requirement of the repository with its type (`product`
  or `code`) and the types of its checks; `--kind product` or `--kind code`
  keeps one type.
- `test` runs each requirement's checks on its labeled examples (below).

Exit codes: `0` when every selected check ran and found no blocking violation,
`1` on blocking violations, `3` on any config or tool error (which takes
precedence). Exit 0 means the checks passed, not that the requirements hold:
checks approximate requirements. The summary counts checks run, checks with
nothing to check, and code units judged, so a run that looked at nothing shows.

## Using it

**In CI**, as a required check on pull requests:

```yaml
# .github/workflows/reqfile.yml
name: reqfile
on: pull_request
jobs:
  check:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0
      - run: cargo install reqfile --locked
      # Also install the tools the Reqfiles' commands use (ruff, ast-grep, knip, ...).
      - run: reqfile check
        env:
          OPENROUTER_API_KEY: ${{ secrets.OPENROUTER_API_KEY }}
```

**With Claude Code**, as a Stop hook that sends violations back to the agent
before it finishes:

```json
{
  "hooks": {
    "Stop": [
      { "hooks": [{ "type": "command",
        "command": "jq -e '.stop_hook_active' >/dev/null && exit 0; reqfile check --fast --changed >&2 || exit 2" }] }
    ]
  }
}
```

The `stop_hook_active` guard lets the agent stop after one forced retry, which
avoids infinite loops but allows finishing with violations left. CI remains the
gate.

**In AGENTS.md**:

```markdown
Before editing a folder, run `reqfile explain <folder>` to see the requirements that apply.
Before finishing, run `reqfile check --fast --changed` and fix every violation.
```

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.

## Development

reqfile checks itself: `Reqfile.yaml` lists its product requirements, each
verified by the integration tests in `tests/<id in lowercase>.rs`. Decision
tests call a local fake of Jev, so the suite runs offline.

```sh
cargo test
reqfile check
```
