# Changelog

Notable changes to reqfile, newest first. Versions follow
[semantic versioning](https://semver.org); before 1.0, a minor version may
change the Reqfile format or the CLI, and says so here.

## 0.5.0 (2026-10-09)

- FAST_FEEDBACK, the first reviewed requirement package shipped in
  [`requirements/fast-feedback/`](requirements/fast-feedback/): the latency
  of each pull-request CI job only ratchets down. Scenarios and baselines
  live in the repository's `.github/fast-feedback.json`; see the package's
  README for the measure workflow and the nightly ratchet. Import it under
  `process:` pinned to the commit of tag `v0.5.0` (`reqfile add
  arkadia-so/reqfile@v0.5.0 FAST_FEEDBACK` writes the line):
  `- { id: FAST_FEEDBACK, use: arkadia-so/reqfile@<40-char commit of v0.5.0> }  # v0.5.0`.
  A scenario may leave out its baseline while there are too few samples to
  set one, with a `todo` saying when: it is measured, never judged.
- `reqfile eval --format json`: each requirement's counts per split and every
  case, with its outcome, the files flagged, its `known` and its error.
- Examples take any split name of lowercase letters, digits and dashes
  (`split: confirmation-1`), each reported apart by name; `holdout` keeps its
  meaning.
- `--command-override 'FROM=>TO'` on `check` and `eval` replaces text in
  command checks' `run` for one run, such as to measure a package's working
  copy; a FROM that matches no command is an error.
- `--changed` works without `origin/HEAD`: it falls back to `origin/main` or
  `origin/master`, and in a repository without a remote to the local `main`
  or `master`, else the last commit; before the first commit it checks
  everything. A `note` line names the base when it is not `origin/HEAD`.
- `reqfile check` and `reqfile eval` fail (exit 3) in a repository without
  any `Reqfile.yaml`, with steps to set reqfile up, instead of a green run
  that checked nothing. Outside a git repository, the error says how to fix
  it too.
- Decision checks can call Jev directly through TypeSafe's API: with only
  `TYPESAFE_API_KEY` set, reqfile uses `https://api.typesafe.ai/v1` and
  `jev-latest`. With `OPENROUTER_API_KEY` set, OpenRouter is used as before.
- Documentation at [reqfile.dev/docs](https://reqfile.dev/docs).
- Tests follow the setup a newcomer does and check that every Reqfile shown
  in the README and the agent skill is accepted as written.

## 0.4.0 (2026-09-29)

- Output for people: in a terminal, `reqfile check` draws one row per
  requirement, grouped by Reqfile folder, then the details of what does not
  pass, each fix hint once. `-v` shows every finding and what each check
  ran, `-q` only what fails. Outside a terminal the plain output is
  unchanged (`--format plain`; `summary` is still accepted).
- `reqfile eval` shows the same table, with rates and their intervals.
- A command check can set `mode: advisory`: its violations are reported
  without failing the run.
- A missing Jev key is announced once, before anything runs.
- Examples can be held out (`split: holdout`), `reqfile eval --repeat N`
  runs each case N times, and rates come with 95% intervals.

## 0.3.0 (2026-09-28)

- **Breaking:** a Reqfile pins a repository to a full commit
  (`use: owner/repo@<commit>  # v1.2.0`); a tag is an error pointing to
  `reqfile add`, which writes the pinned use blocks for you (`--dry-run`
  only shows them). `reqfile update` moves every pin to its repository's
  latest release tag. Sources are fetched once into the user's cache.
- `reqfile check --use <location>` and `reqfile eval --use <location>` try
  a folder's or a repository's requirements for one run, without writing
  any Reqfile.
- **Breaking:** `reqfile test` becomes `reqfile eval` (`test` stays a hidden
  alias). An example is `.reqfile/<ID>/examples/<name>/` with its label in
  `example.yaml` and its case in `files/`, so no check can read its answer.
  `reqfile example add` turns files met in real use into an example.
- Command checks: a SARIF result may carry a probability, judged against the
  check's `thresholds`; `format: junit` reads test runner reports.
- Conflicts: for each finding, reqfile asks Jev whether its fix would break
  another requirement that applies to the same file, lists those under the
  finding (`breaks:`) and groups them as decisions to make.

## 0.2.0 (2026-09-27)

- `use` takes a code or process requirement from a folder of the repository
  or from another repository; the block in the nearest Reqfile applies.
- Commands reach their requirement's files through `$REQFILE_ASSETS`, and
  the running reqfile through `$REQFILE`.
- `--changed` rechecks a requirement on its whole scope when its
  requirement, files, settings or source changed.
- Errors of advisory checks no longer change the exit code; blocking
  decision checks need a pinned model.
- `reqfile list --format json`, and `reqfile add` prints the use blocks and
  the commands they run.

## 0.1.0 (2026-09-26)

First release: `Reqfile.yaml` files cascading from the repository root,
command checks reusing existing tools, decision checks asking Jev a yes/no
question about each code unit, `.reqfile/config.yaml` settings in any
folder, `reqfile check` (with `--changed`, `--fast`, `--only`, `--log`),
`reqfile explain`, `reqfile list`, labeled examples, a Jev answer cache,
prebuilt binaries for macOS and Linux, and an agent skill.
