# RECOVERABLE_EFFECTS label review

Reviewed on 2026-10-15 by the independent Codex subagent `/root/evidence_review`.
The reviewer authored neither these examples nor the checker and did not inspect the checker.
The source corpus was present at commit `31775d644f98afef61e126c6075c6aeed4dc62ff`;
the corpus tag created with this review freezes its contents before checker evaluation.

## Method

A helper shuffled all 36 reliability cases with seed `184620`, copied their program and
binding files to numbered directories, and withheld original directory names and
`example.yaml`. The reviewer read the requirement decisions and actual programs, then
froze independent verdicts and rationales before comparing expected labels. The frozen
36-case label record has SHA-256
`be09c2c09b9bddd052e8e5969485b247a3b4715f9655fa18932ef76b9168bb82`.

Each copied program was then executed directly with a fresh JUnit report path, without
the proposed checker. All six program exits agreed with the independent labels, and
every binding named exactly the tests present in its report.

## Results

| Split | Cases | Independent ok | Independent violation | Disagreements |
| --- | ---: | ---: | ---: | ---: |
| development | 2 | 1 | 1 | 0 |
| confirmation-1 | 2 | 1 | 1 | 0 |
| confirmation-2 | 2 | 1 | 1 | 0 |

These are label-agreement and direct-execution results, not measured checker precision
or recall. No case required an ambiguous label.

| Anonymous case | Source case | Split | Independent label | Reason |
| --- | --- | --- | --- | --- |
| case-025 | [c1-partial-signup-a](.reqfile/RECOVERABLE_EFFECTS/examples/c1-partial-signup-a/example.yaml) | confirmation-1 | ok | Each applied effect is recorded before the injected interruption and recovery skips completed effects. |
| case-016 | [c1-partial-signup-b](.reqfile/RECOVERABLE_EFFECTS/examples/c1-partial-signup-b/example.yaml) | confirmation-1 | violation | Partial failure happens before progress records the committed membership, so recovery repeats it. |
| case-021 | [c2-lost-step-acknowledgement-ok](.reqfile/RECOVERABLE_EFFECTS/examples/c2-lost-step-acknowledgement-ok/example.yaml) | confirmation-2 | ok | Recovery consults the independently committed effect receipt before finishing forward. |
| case-031 | [c2-lost-step-acknowledgement-violation](.reqfile/RECOVERABLE_EFFECTS/examples/c2-lost-step-acknowledgement-violation/example.yaml) | confirmation-2 | violation | Recovery ignores an identifiable receipt and reserves the same stock again. |
| case-028 | [development-ok](.reqfile/RECOVERABLE_EFFECTS/examples/development-ok/example.yaml) | development | ok | Partial progress identifies the charge and repeated forward recovery skips completed effects. |
| case-009 | [development-violation](.reqfile/RECOVERABLE_EFFECTS/examples/development-violation/example.yaml) | development | violation | The applied charge is absent from progress after partial failure, so recovery charges again. |

Reviewed Python program hashes:

- `case-025`: `3a8dfdae0be8d3dc6699620360d34edf823174364cc5b951f218b44be44d4726`.
- `case-016`: `9084351ad911acbc2a4158f1b8a9072907b5a065de7dc333857f1f658e08e86e`.
- `case-021`: `187a1bb21cd62780c0001718d666c794351140d642e76c89f04f83e1007cb423`.
- `case-031`: `1c4b9fc89ffbb71d0b2ddb93611d8e2bea407cd32735b566dad23375ac82243d`.
- `case-028`: `cf71bfac761d3395c287396ba7a7c6d61309ea089ebf8f6a1a48e8640dbf4930`.
- `case-009`: `5371b51931356e2b5863297fd9b14135c423300f63980f36291af6eb7a2e60f5`.

## Limits

Blinding hid metadata and provenance; it did not remove source-level clues. Some
programs contain mutation constants such as `BUG`, and the programs are small near
pairs. The reviewer is another Codex agent sharing the requirement design context,
not a human reviewer or a different model family. Label agreement on these fixtures
does not establish generalization to real repositories.

All reviewed behavior fixtures use Python and JUnit reporting. They do not exercise
the real TypeScript/Vitest, Dart, Effect, or Riverpod execution boundaries. Passing
their declared scenarios is evidence for those scenarios only; it does not establish
complete operation inventory, production-faithful test doubles, or assertion adequacy.

Six additional report-protocol cases are reviewed separately in
[the scenario adapter label record](../../adapters/scenarios/LABELS.md). Their expected
outcomes were accidentally exposed by the copy helper, so that review is explicitly
non-blind and is excluded from the counts above.
