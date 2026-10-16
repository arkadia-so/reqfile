# Scenario protocol evidence review

Reviewed on 2026-10-15 by Codex subagent `/root/evidence_review`, separately from the
36 behavioral examples. The reviewer did not inspect or run the checker.

## Disclosure

The helper anonymized directory names and omitted `example.yaml`, but these supplemental
cases store expected outcomes in `case.json`. That file was inadvertently displayed
before independent protocol verdicts were frozen. This is an independent source and
execution review, **not a blind label review**. These six cases are excluded from the
behavioral label-agreement counts and cannot be claimed as six blind confirmations.

## Direct execution

All six commands exited zero. The reviewer inspected the emitted reports and confirmed
that each is unusable evidence under DECISIONS.md; the required adapter outcome is a
tool error (exit 2), never a pass. No adapter implementation was used for these results.

| Anonymous case | Source case | Requirement | Observed contract problem |
| --- | --- | --- | --- |
| case-037 | [c1-skipped-test](examples/confirmation-1-errors/c1-skipped-test/case.json) | DURABLE_WORK | A required restart scenario is skipped, so it provides no behavioral evidence. |
| case-038 | [c1-missing-report](examples/confirmation-1-errors/c1-missing-report/case.json) | DATA_ISOLATION | The runner writes a previous-report artifact but does not produce the fresh report requested by the adapter. |
| case-039 | [c1-duplicate-test](examples/confirmation-1-errors/c1-duplicate-test/case.json) | CONCURRENT_TRANSITIONS | The runner report repeats a required test name, so evidence is ambiguous. |
| case-040 | [c1-missing-test](examples/confirmation-1-errors/c1-missing-test/case.json) | REPLAY_SAFE | The runner report omits one required test after executing the behavior suite. |
| case-041 | [c1-empty-report](examples/confirmation-1-errors/c1-empty-report/case.json) | RECOVERABLE_EFFECTS | The runner reports zero executed tests despite the command returning successfully. |
| case-042 | [c1-truncated-report](examples/confirmation-1-errors/c1-truncated-report/case.json) | SCOPED_WORK | The runner leaves an incomplete XML report after running the behavior suite. |

## Claims and boundaries

The decisions correctly limit a pass to successfully executed named scenarios, and
distinguish violations from unavailable, missing, or malformed evidence. Language
runner selection and installed-library checks establish contract compatibility; they
cannot establish that a project test uses the actual library boundary or meaningful
assertions. Keep those limits visible in adoption documentation.

The 36 behavioral labels were frozen before their expected labels were revealed; their
record SHA-256 is
`be09c2c09b9bddd052e8e5969485b247a3b4715f9655fa18932ef76b9168bb82`.
Per-requirement LABELS.md files record every independent verdict and program hash.
Neither this review nor that label agreement measures production precision or recall.
