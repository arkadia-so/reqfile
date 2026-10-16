# REPLAY_SAFE label review

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
| case-022 | [c1-receipt-boundary-a](.reqfile/REPLAY_SAFE/examples/c1-receipt-boundary-a/example.yaml) | confirmation-1 | ok | Deduplication is recorded before acknowledgement failure and distinct operations remain separate. |
| case-017 | [c1-receipt-boundary-b](.reqfile/REPLAY_SAFE/examples/c1-receipt-boundary-b/example.yaml) | confirmation-1 | violation | Acknowledgement loss occurs before deduplication records the already-applied effect. |
| case-032 | [c2-receipt-retention-ok](.reqfile/REPLAY_SAFE/examples/c2-receipt-retention-ok/example.yaml) | confirmation-2 | ok | Receipts remain across archival for the declared replay lifetime and acknowledgement-loss retry. |
| case-013 | [c2-receipt-retention-violation](.reqfile/REPLAY_SAFE/examples/c2-receipt-retention-violation/example.yaml) | confirmation-2 | violation | Archiving deletes deduplication receipts before the declared replay lifetime ends. |
| case-034 | [development-ok](.reqfile/REPLAY_SAFE/examples/development-ok/example.yaml) | development | ok | One business effect is recorded per identity before an acknowledgement can fail. |
| case-015 | [development-violation](.reqfile/REPLAY_SAFE/examples/development-violation/example.yaml) | development | violation | Repeated delivery increments the business balance each time despite a stable operation identifier. |

Reviewed Python program hashes:

- `case-022`: `d3541fb74fe03cf24746cefcfcf2d99cffaf4a064544a1981eba89eec66746a8`.
- `case-017`: `7f4effaff42c88d65464dd1aa8820ad5e2bb1d4a8d66e56ec9689126c29668f7`.
- `case-032`: `484fec37d7ff8195d5fe64da619a7bf1165f70fbc275743fe9a864556b656eb7`.
- `case-013`: `18b83f82b2024c27bce1be9d41acac5c1b39c69b9108122da67ec99b82c4068a`.
- `case-034`: `07d5cc76bb45e18ee94845a430fca9048e7c2929a72ca68c0c87c8eb2081151d`.
- `case-015`: `552b671b7a3297e44e41e0b56d27c5085818f11cf172d929b11f6418afb2f8c9`.

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
