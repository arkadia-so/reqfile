# SCOPED_WORK label review

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
| case-011 | [c1-late-callback-a](.reqfile/SCOPED_WORK/examples/c1-late-callback-a/example.yaml) | confirmation-1 | ok | Closing cancels the child task and prevents a pending dependency from publishing to the closed owner. |
| case-002 | [c1-late-callback-b](.reqfile/SCOPED_WORK/examples/c1-late-callback-b/example.yaml) | confirmation-1 | violation | An uncancellable dependency callback publishes after its owner closes. |
| case-007 | [c2-owner-name-reuse-ok](.reqfile/SCOPED_WORK/examples/c2-owner-name-reuse-ok/example.yaml) | confirmation-2 | ok | Late callbacks require the same live owner incarnation rather than merely a reused owner name. |
| case-018 | [c2-owner-name-reuse-violation](.reqfile/SCOPED_WORK/examples/c2-owner-name-reuse-violation/example.yaml) | confirmation-2 | violation | A stale callback resolves an owner by name and mutates a replacement owner incarnation. |
| case-019 | [development-ok](.reqfile/SCOPED_WORK/examples/development-ok/example.yaml) | development | ok | The commit callback checks owner liveness and resource release is guarded against duplication. |
| case-036 | [development-violation](.reqfile/SCOPED_WORK/examples/development-violation/example.yaml) | development | violation | The callback writes an obsolete value even when its owner is no longer live. |

Reviewed Python program hashes:

- `case-011`: `92de4262e537f23950cfe6111d78f7b9b5bfb27006b845df9123834fe36046eb`.
- `case-002`: `84dcc8bb01ba06d5cc88005b4edbdca809191cc020338340bbb5b0db5dd83712`.
- `case-007`: `024019c9fe5a0c17d4d426a626eb72ee8b39088de8cbb3f01db2b23b5a2463c7`.
- `case-018`: `c9203bb2dd10d5783731117aa9ce93174829835b66ea870cdb8c0914a484bdd3`.
- `case-019`: `3cd39a44f14ab6f298821ca3a4d2b96769e19ff2d233aee9ef2e2af4da0cfc99`.
- `case-036`: `553b263e98346ad62716b7bd7c3e224ff796cd2f2dc14d91a74df881df7138ee`.

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
