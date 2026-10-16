# CONCURRENT_TRANSITIONS label review

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
| case-005 | [c1-inventory-contention-a](.reqfile/CONCURRENT_TRANSITIONS/examples/c1-inventory-contention-a/example.yaml) | confirmation-1 | ok | Both competing calls overlap at a barrier and recheck current capacity inside the lock. |
| case-026 | [c1-inventory-contention-b](.reqfile/CONCURRENT_TRANSITIONS/examples/c1-inventory-contention-b/example.yaml) | confirmation-1 | violation | The protected write still trusts stale pre-barrier capacity, allowing over-allocation. |
| case-029 | [c2-reservation-snapshot-ok](.reqfile/CONCURRENT_TRANSITIONS/examples/c2-reservation-snapshot-ok/example.yaml) | confirmation-2 | ok | The transaction conditionally reduces current capacity, preserving the declared conservation invariant. |
| case-023 | [c2-reservation-snapshot-violation](.reqfile/CONCURRENT_TRANSITIONS/examples/c2-reservation-snapshot-violation/example.yaml) | confirmation-2 | violation | Each transaction trusts stale pre-barrier capacity and both reservations are accepted. |
| case-014 | [development-ok](.reqfile/CONCURRENT_TRANSITIONS/examples/development-ok/example.yaml) | development | ok | The lock protects both the current-capacity check and the state transition after forced overlap. |
| case-035 | [development-violation](.reqfile/CONCURRENT_TRANSITIONS/examples/development-violation/example.yaml) | development | violation | Both writers use stale capacity despite serializing writes, so the booking invariant fails. |

Reviewed Python program hashes:

- `case-005`: `dbcc6842047ab18398f6941f3df93c16a6970f0c2d460c5b9f7639500ddb301d`.
- `case-026`: `f486c67875d837028554239a3967a459a421ce11a198fbc61fb30936267fe7e3`.
- `case-029`: `e46254901fab7604be76f8529149a7baf8dfbfd85aa00d88f8ef0bf196b24633`.
- `case-023`: `fa4115bb808902333d35e3886fbd07f777f31571d552ddc4cbf45fcea734f260`.
- `case-014`: `b6643d8f67b434592d4b09fa07529de1f6de2e005ea9317aab86500c60ca4e94`.
- `case-035`: `45d5b97813cc7c8d452ab13ca018cc0d0e77472719a6dd19f5b36b980f53763d`.

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
