# DURABLE_WORK label review

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
| case-006 | [c1-pending-retry-a](.reqfile/DURABLE_WORK/examples/c1-pending-retry-a/example.yaml) | confirmation-1 | ok | Accepted and retryable work remains persisted until an explicit success or refusal. |
| case-030 | [c1-pending-retry-b](.reqfile/DURABLE_WORK/examples/c1-pending-retry-b/example.yaml) | confirmation-1 | violation | A transient failure deletes accepted work before recording any terminal outcome. |
| case-024 | [c2-abandoned-claim-ok](.reqfile/DURABLE_WORK/examples/c2-abandoned-claim-ok/example.yaml) | confirmation-2 | ok | A replacement worker recovers the claimed job after an abrupt process exit. |
| case-004 | [c2-abandoned-claim-violation](.reqfile/DURABLE_WORK/examples/c2-abandoned-claim-violation/example.yaml) | confirmation-2 | violation | A claimed job remains permanently undiscoverable after its worker process exits. |
| case-001 | [development-ok](.reqfile/DURABLE_WORK/examples/development-ok/example.yaml) | development | ok | Accepted pending state is committed to a file database and reconstructed by a new worker. |
| case-027 | [development-violation](.reqfile/DURABLE_WORK/examples/development-violation/example.yaml) | development | violation | Accepted work lives only in the original in-memory database and disappears on replacement. |

Reviewed Python program hashes:

- `case-006`: `5a11e8c8812ef64122c07b6c0cdc96739f2843461b91dab6d776efa4f4044f8f`.
- `case-030`: `aac75be077c7c1028c7f301a36aaf1695ab8710704f53eb4999572eab5494295`.
- `case-024`: `5b3f09cf6fd15c937596c7e99c48530c5001b909ec28c66122bcfa5acb8d793e`.
- `case-004`: `fee43e4014f3cf1d9bbb1da4828d16175bcae804b4af04f28d833bcf098625d5`.
- `case-001`: `cf08395da5eac47b6a686762ae7c370c4d04cfebf554231ae7100b606d942e27`.
- `case-027`: `8b625636b5b976768faeb533a62122965e0fdd5f0dacf27a33aa59863614f2b4`.

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
