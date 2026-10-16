# Scenario evidence adapter

This adapter checks that an explicit set of scenario tests executes successfully.
Project tests own the assertions; the adapter owns invocation, coverage and report
validation. It uses Python 3.10+ and the standard library only.

The [contract](DECISIONS.md) defines binding fields and verdicts. Six exports match
the six requirement package names: `replay-safe`, `concurrent-transitions`,
`durable-work`, `recoverable-effects`, `scoped-work`, and `data-isolation`.
Their required obligations are documented with each requirement.

## Bind a project

Create `.reqfile/<ID>/binding.json` in the adopting Reqfile's directory. For example:

```json
{
  "command": ["python3", "invoices/scenarios.py"],
  "scenarios": {
    "same-operation": "repeated invoice delivery is applied once",
    "distinct-operations": "distinct invoices both apply",
    "retry-after-failure": "lost acknowledgement does not duplicate the invoice"
  },
  "timeout_seconds": 60
}
```

The generic command writes JUnit XML to the fresh path in `REQFILE_SCENARIO_REPORT`.
`command` is an argv array, with no shell expansion. Optional `cwd` stays inside the
adopting directory, including after resolving symlinks. An explicit adapter `with`
can supply these fields directly, or select another relative file with `binding`.
Library adapters add obligations through `extra_scenarios`; that field belongs to
adapter composition and cannot be smuggled into a project binding file.

Exit 0 means every named test ran once and passed. A reported failed test exits 1,
with a SARIF location in the test file when available. A missing binding, missing
runner, timeout, skipped test, malformed report or runner crash exits 2. Nonzero
runner exits without a reported failed test are tool errors. Suite-level failures
and inconsistent summary counts cannot be converted into a passing verdict.

Reports and captured output are limited to 8 MiB, commands to 300 seconds. A new
temporary report path prevents accidental reuse of a prior run's evidence. A
project command is trusted code with the same access as its test runner; this is
not a sandbox and cannot stop a dishonest program from fabricating test evidence.

## Coverage limits

Passing tests cannot establish their own completeness or assertion quality. Review
the chosen operations, failure boundaries, interleavings, storage fidelity and
authorization expectations. The checker does not discover these from API names.
No automatic claim of coverage across arbitrary repositories is made.

Run the protocol regression suite with:

```sh
python3 -m unittest discover -s adapters/scenarios -p test_check.py
```
