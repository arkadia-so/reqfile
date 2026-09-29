# reqfile

Check that a codebase meets its requirements, so agents and humans get
concrete, fixable violations instead of vague advice.

Requirements live in `Reqfile.yaml` files next to the code. Each one says what
must hold, why, for whom, and how it is checked: with an existing tool when the
rule is mechanical (`command`), or with [Jev](https://docs.typesafe.ai), a
decision model returning calibrated probabilities, when it needs judgment
(`decision`). A requirement written once can be taken by reference in other
folders and repositories (`use`).

```
$ reqfile check
FAIL_FAST  services/api/handler.py:42  Do not use bare `except` (E722)
  fix: Catch the specific expected error and handle it visibly, or let it propagate.
advisory  DECOMPLECT  services/sync/worker.ts:88  p=0.91  It mixes several jobs.
  fix: Split the I/O from the logic into two functions.

14 checks run, 2 with nothing to check, 212 code units judged. 1 violation, 1 advisory finding, 0 errors.
```

## Install

Give your coding agent this prompt to install Reqfile and wire it into your
project's workflow:

```text
set up https://reqfile.dev/SKILL.md
```

Read the [agent skill](skills/reqfile/SKILL.md) or visit [reqfile.dev](https://reqfile.dev).

With [mise](https://mise.jdx.dev), from the prebuilt binaries (macOS and Linux):

```toml
# mise.toml
[tools]
"github:arkadia-so/reqfile" = "0.4"
```

Or with the install script, `cargo binstall reqfile`, or from source with
`cargo install reqfile --locked` (Rust 1.90 or later):

```sh
curl -LsSf https://github.com/arkadia-so/reqfile/releases/latest/download/reqfile-installer.sh | sh
```

reqfile needs `git`, `sh`, `tar` (for requirements taken from other
repositories), and the tools your Reqfiles' commands call.

## Security

Reqfiles, configs and requirements taken with `use` are executable code:

- A command check runs its shell command on your machine or CI runner, with
  your environment.
- A config chooses the Jev endpoint and the environment variable whose value is
  sent to it as the API key.
- Decision checks send the code they select to the configured provider.
- Taking a requirement from another repository (`use: owner/repo@ref`) means
  running that repository's commands in your CI. Pin it to a commit or a tag
  you trust, and read what `reqfile add` prints before adding it.

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

  - { id: DECOMPLECT, use: acme/reqs@v1.2.0 }
```

- The file is named exactly `Reqfile.yaml` and has a `reqfile: 1` version key.
- Requirements are listed under `product` (what users get), `code` (how the
  code is built) or `process` (how the project is run, such as its CI).
- A block with `use` is a use block, taking its requirement by reference
  (below); any other block is a definition. A definition has an `id` in
  SCREAMING_SNAKE_CASE, `must`, `why` and at least one check, and optionally
  `who` and `ref`.
- Unknown keys, missing fields and near-miss file names (`reqfile.yaml`,
  `Reqfile.yml`) are errors naming the file and line. A near-miss name is only
  an error when the file has a top-level `reqfile`, `product`, `code` or
  `process` key, so a CI workflow named `reqfile.yml` is fine.

### Macro and micro

A requirement is a macro: one durable promise, in one or two sentences, that
would get a bug filed if it broke six months from now. Its details are micro:
tests named after the sentence they prove, in the files its checks run.
Implementation decisions, such as a chunk size or a library choice, stay in
specs and pull requests. Keep each Reqfile to at most 10 product requirements;
past that, merge requirements into broader promises, or split the folder into
subfolders with their own Reqfiles.

### Scope

Any folder can hold a `Reqfile.yaml`. For a file and an id, the block that
applies is the one in the nearest Reqfile at or above the file's folder; blocks
with that id in farther Reqfiles do not apply to it. A definition id appears
once in a repository, and an id at most once in a Reqfile, so a nearer block
with the same id is always a use block. Each code unit is judged at most once
per id in a run.

Target files come from git (tracked and untracked, never ignored), minus
`.reqfile/` folders and the `exclude` globs of the configs above them.

### Command checks

| Field | Required | Meaning |
|---|---|---|
| `run` | yes | Shell command, run from the folder of its Reqfile |
| `fix_hint` | yes | One line telling how to fix a violation |
| `files` | no | Glob relative to that folder; the command runs only if a target file matches |
| `pass_files` | no | Append the matching files as arguments (default false) |
| `format` | no | `exit` (default), `sarif` or `junit` |
| `violation_codes` | no | Exit codes meaning "violations found" (default `[1]`; `[101]` for `cargo test`) |
| `timeout` | no | Seconds, default 60 |
| `fast` | no | Quick enough for `reqfile check --fast`, such as in an edit hook (default false) |
| `thresholds` | no | `{ violation_above, pass_below }` for SARIF results carrying a probability (default 0.8 and 0.2) |
| `mode` | no | `blocking` (default), or `advisory`: violations are reported without failing the run, and a failure to run is an advisory error |

A command without `files` always runs. With `pass_files`, the files are
appended at the end of the whole command, so a pipeline such as `tool "$@" |
filter` receives nothing; put pipelines in a script.

Commands reach the files of their requirement through `$REQFILE_ASSETS`, the
absolute path of its `.reqfile/<ID>/` folder, as in `run:
$REQFILE_ASSETS/check.sh`. `$REQFILE` is the reqfile executable running the
check, for commands that inspect the requirements themselves, such as
`$REQFILE list --format json`.

With `format: exit`, exit 0 passes and a code listed in `violation_codes`
reports one violation carrying the last 50 lines of output.

With `format: sarif`, the report is parsed on both exit 0 and a listed violation
code. Each `kind: fail` result is a violation; an absent `kind` defaults to
`fail`. Other kinds and suppressed results are ignored. A suppression with
no status or `status: accepted` suppresses the result; `rejected` and
`underReview` do not. An empty `suppressions` array does not suppress anything.
This catches findings from pipelines such as `clippy | clippy-sarif` even when
the final command exits 0.

A SARIF result may carry `properties.probability`, the probability that it
is a violation, so heuristics and models speak the same protocol as linters.
Such a result is judged by it, whatever its kind: above `violation_above` a
violation, below `pass_below` nothing, and between an uncertain finding,
always advisory. A probability outside 0 to 1 is a tool error. A checker can
report every file it judged, with low probabilities for the fine ones.

With `format: junit`, the command prints a JUnit XML report, as cargo-nextest,
pytest, vitest, bun and gotestsum write: each failed or errored test case is a
violation named after the test (`test output::names_conflicts failed: …`),
located by its `file` and `line` attributes when the runner gives them. A
report with no test case, or a violation code with no failed test, is a tool
error.

A checker is any program. When a requirement is shared, its command fetches
the checker with its ecosystem's runner at an exact version, such as `bunx
@scope/reqfile-colocation@0.1.1`, `uvx tool==1.2.0` or `go run
example.com/tool@v1.2.0`, so nothing needs installing and reqfile manages no
packages.

Invalid SARIF (including on exit 0), a violation code with no SARIF results,
any unexpected exit code, a signal or a timeout is a tool error. A report
containing only ignored results passes on exit 0 or a listed violation code.

### Decision checks

A `decision` check (or `decision: { mode: blocking }`) reads
`$REQFILE_ASSETS/decision.yaml`, the `.reqfile/<ID>/decision.yaml` of its
requirement:

```yaml
units:                      # ast-grep rules selecting the code to ask about
  - { language: typescript, rule: { kind: catch_clause } }
  - { language: tsx, rule: { kind: catch_clause } }
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
`ignores` globs (relative to the Reqfile's folder). A unit rule selects files
of exactly its language: a `typescript` rule never selects `.tsx` files, so
TSX needs its own `tsx` rules. For each unit, reqfile asks Jev the question
with the language, path, unit source and, with `context: enclosing`, the
enclosing function. Each unit is sent once, with the questions of every
decision check that selects it: Jev charges input tokens and reads the state
once per request, so several checks on the same code cost little more than
one. Answers are cached in a local file in the git folder
(`.git/reqfile/jev-cache`), shared by every worktree, so checking never
modifies tracked files. An answer is reused while the unit, the requirement's
question and the exact model behind `latest` are unchanged, and a full run
drops answers no unit uses any more; pruning changes cost, never results. In
CI, keep that file between runs (for example with `actions/cache`). A
probability above `violation_above` is a violation, below `pass_below` a pass,
and anything in between an uncertain finding.

Decision checks are advisory unless `mode: blocking`; uncertain findings are
always advisory. A blocking decision check needs a pinned model (an exact
version, such as `typesafe/jev-1.13-20260917`, set in `.reqfile/config.yaml`),
so its thresholds stay tied to the version they were measured with. Label a few
dozen units of your codebase and adjust the thresholds before making a check
blocking.

## Imports: `use`

A use block takes a requirement defined elsewhere:

```yaml
code:
  - { id: FAIL_FAST, use: ./std }                                                    # a folder of this repository
  - { id: COLOCATION, use: gabsn/reqfile-colocation@d0ba113c4ca2882d8c86329049359e02ae9625b6 }  # v0.1.1
  - id: CONTRACTS
    use: acme/reqs@3f2a9c1e0b7d4c6a8e5f1b2d3c4a5e6f7a8b9c0d   # v1.2.0
    why: The billing core must never reach an invalid state.
```

Any repository or folder whose Reqfile.yaml defines a requirement, with its
checks and labeled examples under `.reqfile/<ID>/`, is a package: the same
format as a local requirement, nothing more.

- **Locations.** A folder, relative to the folder of the use block's Reqfile
  (`./std`, `../shared`), or `owner/repo@<commit>`, fetched from
  `https://github.com/owner/repo` (`$REQFILE_GIT_BASE/owner/repo` when that
  variable is set, for a mirror). The use block resolves to the unique
  definition with its id among the Reqfiles at or under that location,
  discovered like a repository of its own, with its exclusions. Use blocks are
  never resolution targets, so chains cannot form.
- **Pins.** A Reqfile pins a repository to a full 40-character commit id; the
  tag it came from goes in a comment. A tag, a branch, a short id or `HEAD` is
  an error: the rules of a repository must not change under it. Nobody types
  the commit: `reqfile add` writes the line and `reqfile update` moves it.
- **Cache.** A commit never changes, so it is fetched once into the user's
  cache (`$REQFILE_CACHE`, else `$XDG_CACHE_HOME/reqfile`, else
  `~/.cache/reqfile`), shared by every repository and worktree, and checked
  offline from then on. CI can keep that folder between runs.
- **Types.** Only `code` and `process` requirements can be taken: a `product`
  requirement describes its own repository's product. The section of the use
  block must match its definition's.
- **What can be set locally.** A use block takes `must`, `ref` and its type
  from its definition. It may set `why` and `who`, and `checks`, which then
  replace the definition's checks entirely. Setting `must` is an error.
- **Where checks run.** The checks of a use block run in the folder of its
  Reqfile, with its globs relative to that folder, and the settings of that
  folder in the repository being checked. A source's configs are only used to
  discover its definitions.
- **`$REQFILE_ASSETS`** is one `.reqfile/<ID>/` folder, never a merge: the
  definition's for inherited checks, the use block's own for checks it sets.
  With inherited checks, the use block's own `.reqfile/<ID>/` may only hold
  `examples/`, where you add your own examples of a shared requirement.

`reqfile add <location> [IDS]` writes the use blocks into the Reqfile of the
current folder, under their sections, a repository pinned to the commit its
tag points to (resolved through `refs/tags/<name>`, never a branch), and says
what their checks run; `--dry-run` only shows them. Without ids, it takes
every code and process requirement there. `reqfile update` moves every pin to
the commit of its repository's latest release tag (`vX.Y.Z`); the diff is the
review.

To try requirements before adopting them, `reqfile check --use <location>`
and `reqfile eval --use <location>` add them for one run, without writing any
Reqfile; `--only` narrows them. A tag is welcome there: it is resolved on
each run and the summary names the commit it resolved to, or the last one
cached when the remote cannot be reached.

Use blocks need a reqfile of version 0.2 or later, and commit pins 0.3: 0.1
reports `use` as an unknown key, naming its line, so it never skips one
silently.

## Evals: `reqfile eval`

Checks approximate requirements, so each one is measured on labeled examples,
the eval of the requirement. An example is a folder under
`.reqfile/<ID>/examples/`:

```
.reqfile/COLOCATION/examples/distant-test/
  example.yaml   # what the checks should say
  files/         # the case, the only part the checks see
    src/checkout/discount.ts
    test/unit/checkout/discount.test.ts
```

```yaml
expected: violation                          # violation | ok
findings: [test/unit/checkout/discount.test.ts]   # optional: where the checks must flag
rationale: The test sits in a mirror tree while its sibling tests are colocated.
origin: authored                             # or real:<commit>, for a case met in real use
known: miss                                  # optional: a failure tracked without failing
split: holdout                               # optional: kept out of tuning, reported apart
```

`reqfile eval` puts each case's `files/` alone in a fresh repository and runs
the requirement's effective checks on it with `$REQFILE_ASSETS` and the Jev
settings of its folder. The label stays outside, so no check can read its
answer. A violation flagged elsewhere than its `findings` counts as missed.
`known: miss` (a violation) or `known: false_alarm` (an ok example) records a
failure the checks are known to have, reported without failing, until the
checks improve and the eval says to remove it. A requirement taken with `use`
is measured on its definition's examples and its own. Examples double as the
requirement's documentation: a violation and an ok case side by side say what
the `must` means.

Checks become precise over time when their failures become evidence:
`reqfile example add <ID> <name> --expected violation|ok [--finding FILE]...
[--rationale TEXT] FILES...` copies files of the repository into a new
example, with the commit it came from, and `reqfile eval` reports it as
missed or a false alarm until the check is improved.

```
UNUSED_CODE  asserted: 2 examples, 2 as labeled
FAIL_FAST  measured, no assertion: 3 of 4 violations caught (95% interval 30 to 95%; 1 missed, 0 not selected, 0 uncertain); 0 of 2 correct examples flagged (95% interval 0 to 66%; 0 uncertain)
  rust-ok-discard: missed
SIMULATION  no evidence: no labeled examples

3 requirements: 1 asserted, 1 measured, 1 without evidence; 6 examples. 0 failing their labels.
```

Each requirement falls in one of three classes:

- **asserted**: deterministic checks only, which must match every label, or
  `reqfile eval` exits 1;
- **measured**: with a decision check, or a command reporting probabilities,
  results are counted, never asserted: judgments vary, so exit 0 does not
  mean the check works. A detector
  earns its place, or `mode: blocking`, by the rates it reaches on examples it
  was not tuned on;
- **no evidence**: no labeled examples, so nothing is known about its checks.

Violations no check looked at are reported as "not selected", apart from those
it looked at and missed. Each rate of a measured requirement comes with its 95%
Wilson interval: on a few examples it is wide, and two rates whose intervals
overlap are not evidence of a difference. A requirement whose examples hold one
class only is reported as measuring half of its checks. A case that cannot run
exits 3.

### The learning loop

A requirement's `must` is a person's promise; its checks are the machine's
reading of it; its examples are where the two meet. Checks converge on what
people mean because every disagreement between them ends as an example:

1. **One precise question per check.** The `must` may be broad; each check
   answers one narrow question about it: a lint rule, a script's criterion,
   a decision's `question` with its `violation_when` and `ok_when`. A broad
   principle gets several checks, each with its own examples, rather than
   one vague question that no example can pin down.
2. **A mistake becomes an example.** A false alarm someone disputes, or a
   violation found by sampling what passed, goes into
   `.reqfile/<ID>/examples/` with `reqfile example add`. Misses are silent:
   `reqfile check --log` records what passed too, so sample it, and review
   the uncertain findings, where a decision check doubts.
3. **Someone else labels it.** Whoever changes the check does not choose the
   label. When two labelers disagree, the `must` is ambiguous: a person
   rewords it, rather than the check being tuned to one reading.
4. **One change, measured.** `reqfile eval` shows the failure; one change to
   the check (its question, selector, thresholds, or a new check for a new
   question) is kept only if the held-out examples improve beyond the noise
   and no other case regresses, as below.
5. **Rates are the contract.** Asserted checks match every label; measured
   ones report rates with intervals. A check becomes blocking only once its
   measured rates meet a target chosen beforehand.

People own the `must`, the labels of disputed cases, and the choice between
requirements that contradict each other. Agents fix code, capture mistakes as
examples, and iterate on checks within the evidence. The output says which is
which: a violation is work for the agent; a `decide` line, a contested label
or a rate below its target is a question for a person.

### Improving a check without fitting its examples

A measured check is improved like any model: on evidence it was not tuned on,
and by more than its noise.

- **Held-out examples.** `split: holdout` (or `reqfile example add
  --holdout`) keeps an example out of tuning. With any, `reqfile eval`
  reports the tuning examples and the held-out ones on separate lines. Choose
  about a third of each class before looking at results, and change a check
  looking only at tuning failures: a change that raises the tuning rates and
  not the held-out ones fits its examples, not the requirement, and is
  reverted.
- **Noise.** `reqfile eval --repeat N` runs each case N times, lists the
  cases whose outcome changes, and counts each by its most frequent outcome,
  a tie against its label; 3 to 5 runs are enough to see it. A change
  improves a check only when it moves more cases than are unstable. An
  asserted requirement whose outcome changes between runs fails: its checks
  are not deterministic. Repetition does not reveal a mistake a model makes
  every time; its probability does.
- **Uncertainty is the review queue.** A decision check's probability says
  where it is unsure, in one call: findings between `pass_below` and
  `violation_above` are reported as uncertain. Those, in `reqfile check`
  output or its `--log`, are the cases worth a person's label; `reqfile
  example add` turns them into examples.
- **Thresholds.** Choose `thresholds` on the tuning examples and confirm them
  on the held-out ones, even when both classes are balanced.
- **Headroom.** When every example is classified as labeled, the benchmark
  cannot show that a change helps. Add cases a person judges hard, and say
  why in `rationale`: near misses on both sides of the `must`, legitimate
  exceptions, violations spread over several files. Not only those today's
  check fails: a benchmark of its failures measures its blind spots, not the
  requirement.
- **No leaks.** No example's text, names or identifiers go into a command, a
  decision question or a threshold. Fix the cause a failure reveals, one
  change at a time, so each change of the rates has one explanation.
- **Labels first.** Before changing a check for a failing case, reread its
  label against the `must`, above all when the check was confident: a wrong
  label is fixed in `example.yaml`, with the reason in `rationale`.

What each step establishes:

| Step | Establishes | Does not establish |
|---|---|---|
| `reqfile eval` | How the effective checks classify the labeled examples, in isolation | Anything about your code, the tools in CI, or thresholds on your code |
| `reqfile check` | What each check selected (files, units) and found, with your settings and this machine's tools | That the selection covers all relevant code, that no finding means the requirement holds, or that decision thresholds suit your code |
| `reqfile add` | Which definitions and commit the blocks resolve to, and what they will run | Anything else: it runs none of them |

What stays unverified in every case: code no check looks at, decision
precision on your code, the tools installed in CI, and the requirements
themselves. Exit 0 means the checks passed, not that the requirements hold.

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

Advisory decision checks follow the latest Jev by default. Set `model` to an
exact version (such as `typesafe/jev-1.13-20260917`) to keep thresholds tied to
the version they were chosen with; blocking decision checks require it. The
endpoint also accepts TypeSafe's own API (`https://api.typesafe.ai/v1`, model
`jev-latest`).

## CLI

```
reqfile check [--fast] [--changed [BASE]] [--only ID,...] [--use LOCATION]... [-v|-q]
              [--format auto|pretty|plain|json] [--log FILE [--log-tag KEY=VALUE]...]
reqfile eval [--only ID,...] [--use LOCATION]... [--repeat N] [-v] [--format auto|pretty|plain] [--repeat N]
reqfile example add <ID> <NAME> --expected violation|ok [--finding FILE]... [--rationale TEXT] [--holdout] <FILE>...
reqfile explain <PATH>
reqfile list [--kind product|code|process] [--format summary|json]
reqfile add <LOCATION> [ID]... [--dry-run]
reqfile update [--dry-run]
```

- `--changed` checks everything a change since the merge-base of BASE and HEAD
  can affect: the changed files (committed, uncommitted, untracked and
  deleted), and every file whose applicable block is not the same as at the
  merge-base, because another block now applies to it or because its
  requirement, its `$REQFILE_ASSETS` files, its settings or the commit of its
  source changed. Unchanged files are checked against a redefined requirement,
  and only files in its scope. Commands without `files` still run.
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
- `explain` lists the requirements that apply to a path: for each, the block
  that applies, and for a use block its resolved definition and whether its
  checks are inherited or set in the block.
- `list` lists every requirement of the repository with its type, the types of
  its checks and, for a use block, its source; `--format json` gives the same
  as data.
- `--format`: in a terminal, `auto` (the default) draws a table of
  requirements, then the details of what does not pass, grouped by
  requirement with each fix hint once, and a summary line, in color unless
  `NO_COLOR` is set; `-v` shows every finding and what each check ran, `-q`
  only what fails the run. Anywhere else, such as in a hook, a pipe or CI, it
  is `plain`: one finding per line, stable for agents and scripts (formerly
  `summary`, still accepted). `json` is the same content as data.
- Before anything runs, a missing Jev key is said once, first (`setup` in the
  plain output, a banner in the pretty one, `setup` in JSON), naming the
  variable and the requirements whose decision checks will not judge code.
  reqfile reads keys from the environment only: fill it with the secret
  manager you use.
- `--use` adds the requirements of a location for this run (above).
- `eval` measures each requirement's checks on its labeled examples (above),
  each case `--repeat N` times; `test` is its former name, kept as an alias
  for one version.
- `example add` turns files of the repository into a labeled example (above).
- `add` writes use blocks pinned to a commit, `update` moves the pins (above).

Every finding of a requirement taken with `use` names its use block and where
its definition was resolved; the summary lists the commit every tag given to
`--use` resolved to.

Requirements will contradict each other. For each violation and advisory
finding, reqfile asks Jev whether making the fix it asks for would break
another requirement that applies to the same file; each one it would break
above 0.7 is listed under the finding (`breaks: CORE (p=0.90)`), and the
summary groups them by pair of requirements as decisions to make:

```
decide  COLOCATION vs FUNCTIONAL_CORE  p=0.85  fixing COLOCATION in 4 files would break FUNCTIONAL_CORE
    src/core/cache.rs, …
    Reword one of the two requirements, or say which one wins where they meet.
```

A contradiction is a product decision, not a fix: an agent would undo one fix
with the next. JSON marks each finding's `conflicts` so hooks can send them to
a human. Answers share the decision cache and go to the run log; without a Jev
key the summary says conflicts were not checked, and failing to ask never
changes the exit code.

Exit codes: `0` when every blocking check ran and found no blocking violation,
`1` on blocking violations, `3` on any config error or any error in a blocking
check (which takes precedence). An advisory check that cannot run, such as a
decision check without an API key in a pull request from a fork, is reported
as an advisory error without changing the exit code: it could never have
blocked. Command checks are always blocking. Exit 0 means the checks passed,
not that the requirements hold: checks approximate requirements. The summary
counts checks run, checks with nothing to check, and code units judged, so a
run that looked at nothing shows.

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
      - run: reqfile eval
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

With `--fast`, only decision checks (advisory unless blocking) and the command
checks declared `fast: true` run, so the hook blocks only on those; declare
quick commands `fast`. The `stop_hook_active` guard lets the agent stop after
one forced retry, which avoids infinite loops but allows finishing with
violations left. CI remains the gate. When the output holds `decide` lines,
tell the agent not to fix the findings marked `breaks:` but to ask you: they
contradict another requirement.

**In AGENTS.md**:

```markdown
Before editing a folder, run `reqfile explain <folder>` to see the requirements that apply.
Before finishing, run `reqfile check --fast --changed` and fix every violation.
```

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.

## Development

reqfile checks itself: `Reqfile.yaml` lists its product requirements, each a
macro promise checked by the integration tests in `tests/`, whose names are
its micro requirements. Decision tests call a local fake of Jev, and import
tests local git repositories, so the suite runs offline.

```sh
cargo test
reqfile check
reqfile eval
```

CI (`.github/check.sh`, run for every pull request and before every release)
compiles the tests with `cargo test --no-run --locked` before running `cargo
test --locked`, `reqfile check` and `reqfile eval`, so cold builds and Cargo's
build lock cannot exhaust a check's timeout.
