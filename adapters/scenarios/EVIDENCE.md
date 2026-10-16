# Scenario checker evidence

Measured on 2026-10-15 against the original public corpus frozen as
`reqfile-reliability-corpus-v1`, before checker implementation. The two confirmation
authors did not read the checker or each other's cases. An independent reviewer
labeled all 36 programs without their example labels, then compared the frozen
verdicts; see each requirement's `LABELS.md` for the method and its limits.

The first confirmation measurement ran each case three times. No confirmation result
was used to tune the checker. [Raw results](evidence/corpus-v1.json) include every case,
split, outcome and the checker source hash.

| Split | Cases | Conforming passed | Violations detected | Errors | Unstable cases |
|---|---:|---:|---:|---:|---:|
| Development | 12 | 6/6 | 6/6 | 0 | 0 |
| Confirmation 1 | 12 | 6/6 | 6/6 | 0 | 0 |
| Confirmation 2 | 12 | 6/6 | 6/6 | 0 | 0 |

Each of the six requirements contributes one conforming/violating pair per split.
These are 36 small, original, runnable models and 108 checker executions. They test
the scenario contract and the detection of failures asserted by those programs;
they are not a sample of production repositories and do not establish production
precision, recall, assertion quality or operation coverage.

Six additional independent protocol cases cover skipped, missing, duplicate, empty
and truncated evidence. All six produced tool errors in all three executions
([results](evidence/protocol-v1.json)). Their independent source review was explicitly
nonblind and is separate from the behavioral label agreement.

Independent adversarial review also reproduced a suite-level JUnit error that the
first parser ignored. A development regression and valid twin now cover it. This
fix preceded the confirmation measurement. The protocol unit suite has 14 tests.

## Actual framework execution

The committed scoped-work examples exercised real dependencies, without replacing
their reporters or lifecycle APIs:

- Effect 4.0.1 with Vitest 5.0.3 on Node 24.12: four tests and the installed-version
  guard passed. An independent reviewer also observed an actual failing assertion
  produce a violation with the test location and adapter provenance.
- Riverpod 3.0.3 with Dart 3.9 and test 1.26.3: four provider lifecycle tests and the
  resolved-version guard passed. This adapter supports the Dart test runner.

The examples include lockfiles. A compatible installed major is an explicit guard;
it does not establish compatibility with every minor release or a project's complete
dependency graph. Version guards and scenario checks are separate composed leaves,
so they may run concurrently; every blocking leaf must pass.
