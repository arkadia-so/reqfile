# CONCURRENT_TRANSITIONS

Competing transitions on shared state preserve its declared invariants.

The [decisions](DECISIONS.md) define each required scenario and the boundary of the
promise. This package uses the [scenario adapter](../../adapters/scenarios/README.md):
your project supplies executable tests and exact names in `.reqfile/CONCURRENT_TRANSITIONS/binding.json`.
Python 3.10 or later is required to run the adapter. Missing bindings fail as tool
errors. Adoption never uses this package's development fixture as your evidence.

## Adoption

Import this requirement through a commit-pinned package or a local `use`, then add a
project binding. Its `command` is an argv array. For generic JUnit, the command writes
a fresh report to `REQFILE_SCENARIO_REPORT`. Use the TypeScript or Dart adapter to run
Vitest or Dart test; use Effect or Riverpod to add their library-specific obligations.

```json
{
  "command": [
    "python3",
    "suite.py"
  ],
  "scenarios": {
    "competing-transitions": "test_competing_transitions",
    "invariant-after-contention": "test_invariant_after_contention"
  }
}
```

The names above belong to the minimal demonstration. Replace them with tests of your
real operations, durable storage, lifetimes and authorization boundary as applicable.
Keep fixtures and assertions with the feature they exercise. A project adapter can
reuse the shared export and bind these inputs once.

## Evidence and limits

The package's own binding runs its conforming executable development example.
`reqfile eval` runs each labeled case in isolation, using that case's binding.
The frozen corpus contains development and independently authored confirmation sets;
measurements and reviewer decisions live in [LABELS.md](LABELS.md).

A pass establishes that all declared scenario tests ran and passed. It does not prove
that their assertions are sufficient, that doubles represent production, or that all
relevant operations were inventoried. No keyword, class name or framework import is
accepted as proof of the business guarantee. Review the scenario quality at adoption.
