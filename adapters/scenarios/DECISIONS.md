# Scenario evidence contract

The adapter runs a project's tests and checks explicit coverage. It cannot establish
whether assertions are adequate, whether every operation was inventoried, or whether a
test double faithfully represents production. A pass means the declared scenarios ran
successfully; it is evidence for the requirement, not a proof of it.

## Project binding

By default, a requirement reads `.reqfile/<REQUIREMENT>/binding.json` beneath the checked
project's current directory. `REQFILE_BINDING` may supply `binding` (a different relative
path), or the binding fields directly. Binding files and `cwd` cannot escape that project,
including through symlinks. Unknown fields are errors.

The binding is an object with:

- `command`: a nonempty array of nonempty strings, executed as argv, never a shell string.
- `scenarios`: an object mapping each required obligation to one exact, nonempty test name.
  Every obligation maps to a distinct test. Extra obligations are rejected unless the
  selected library adapter requires them.
- `runner`: `junit` (default), `vitest`, or `dart`. A language adapter fixes this field.
- `cwd`: an optional relative working directory, default `.`.
- `timeout_seconds`: an optional integer from 1 through 300, default 60.

The adapter creates a new temporary report path for each invocation. For JUnit, it passes
this path in `REQFILE_SCENARIO_REPORT`; the project command must write JUnit XML there.
For Vitest, it appends `run --reporter=json --outputFile=<path>` to the command prefix.
For Dart, it appends `test --reporter=json` and reads the resulting JSON event stream.
A command prefix must therefore name the runner, for example `["npx", "--no-install",
"vitest"]` or `["dart"]`, with optional runner-independent arguments before the suffix.

No report is read from an earlier run. Captured output and report size are bounded.
Process trees are terminated on timeout. The test command runs only once per check.

## Verdicts

- Every required test must occur exactly once and pass. A reported failed test is a
  violation (exit 1). A nonzero command exit without a reported failed test is a tool
  error, since configuration failures and runner crashes are not business violations.
- Missing, empty, skipped, pending, duplicate, malformed, truncated or incomplete
  reports are tool errors (exit 2), including a missing binding or missing runner.
- Failures in any test, even outside the required names, prevent a passing verdict.
- A suite with no executed tests never passes. A named successful test is the unit of
  evidence; adapters do not infer assertion counts from framework output.
- Vitest names are `fullName`; JUnit names are `testcase/@name`; Dart names are the
  `testStart.test.name` values for actual test cases, excluding framework load suites.

The six requirements compose and remain separate. For example, retrying a scoped task
may need REPLAY_SAFE and SCOPED_WORK; passing either does not imply the other.

## Library adaptations

Language adapters select the real test runner. Library adapters add explicit scenarios
to the same requirement and a separate blocking check of the installed library version.
Reqfile may execute these independent leaves concurrently; an unsupported version makes
the aggregate result a tool error even when the scenario command also ran.
Effect supports installed major 4; Riverpod supports installed major 3. Unsupported or
unresolvable versions are tool errors, not silent fallback. These checks establish
compatibility of the scenario contract only, not a static audit of every library call.
Project-specific fixtures, assertions and dependency construction remain in the project.

Corpus authors should exercise this contract independently, using neutral runnable test
programs. Protocol test programs may emit reports intentionally: their role is to test
the adapter's treatment of runner evidence, not demonstrate an application's correctness.
