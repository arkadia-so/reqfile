# Contributing

Thanks for helping. Issues and pull requests are welcome at
[arkadia-so/reqfile](https://github.com/arkadia-so/reqfile).

## Issues

Open an issue for a bug, a check that judges wrongly, a confusing message or
a missing feature. Include:

- `reqfile --version` and your OS;
- the command you ran and its full output (`--format plain` is easiest to
  paste);
- for a wrong judgment, the smallest code that shows it and what you
  expected. A wrong judgment is the most useful report: it becomes a labeled
  example that every later change is measured against.

## Pull requests

This repository is exported from a private monorepo, one commit per
release, so a pull request is never merged here directly. When we accept
one, we apply it to the monorepo with you as co-author, it ships in the next
release, and we close the pull request with a link to that release.

Before opening one:

- Open an issue first for anything larger than a fix, so we agree on the
  behavior before you write it.
- Run `cargo fmt`, `cargo clippy --all-targets -- -D warnings` and
  `cargo test`. The tests never call the network: they replace Jev with a
  fake server, so they need no key. `reqfile check` and `reqfile eval`
  always call the real Jev.
- Run `reqfile check` in this repository: its own requirements are in
  `Reqfile.yaml`. A change of behavior changes the requirement it belongs
  to, and the test named after the sentence it proves.
- Say what changes for users; it goes in [CHANGELOG.md](CHANGELOG.md).

## Platforms

reqfile runs on macOS and Linux. Windows is not supported.
