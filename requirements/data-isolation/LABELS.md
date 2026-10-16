# DATA_ISOLATION label review

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
| case-020 | [c1-identity-response-a](.reqfile/DATA_ISOLATION/examples/c1-identity-response-a/example.yaml) | confirmation-1 | ok | Identity and generation checks reject old responses after cached state is cleared. |
| case-010 | [c1-identity-response-b](.reqfile/DATA_ISOLATION/examples/c1-identity-response-b/example.yaml) | confirmation-1 | violation | A response from the previous identity overwrites the new session cache. |
| case-012 | [c2-organization-switch-ok](.reqfile/DATA_ISOLATION/examples/c2-organization-switch-ok/example.yaml) | confirmation-2 | ok | A changed authorization generation prevents old-organization data from entering the new cache. |
| case-003 | [c2-organization-switch-violation](.reqfile/DATA_ISOLATION/examples/c2-organization-switch-violation/example.yaml) | confirmation-2 | violation | Authorization context changes organization while the late response only compares the stable user identifier. |
| case-008 | [development-ok](.reqfile/DATA_ISOLATION/examples/development-ok/example.yaml) | development | ok | Unauthorized reads and writes are rejected, and session generation rejects obsolete responses. |
| case-033 | [development-violation](.reqfile/DATA_ISOLATION/examples/development-violation/example.yaml) | development | violation | An obsolete pending response bypasses the session-generation check after identity switch. |

Reviewed Python program hashes:

- `case-020`: `d02b4ab7fc0ebb5ff1cc8b908837f0b8b26d1fe64eb92a0d7d8d1334220caecf`.
- `case-010`: `3dded3b1f3f62a5a10597fb58963537bef9a87c29ed4a1f72cabdebea431b3bc`.
- `case-012`: `f56f0ce47011e24f6b20bea570e834a79571975b2fed501ce46cf9d0b0ed82dc`.
- `case-003`: `29791502f2228628b1ad7ef22efaee02d4bc288c073463a8ab67ba900d5c041d`.
- `case-008`: `7e4544fb843ad17d5e23b2a3d774874ba03c8a2e790e9380ed0cc2886b090c3c`.
- `case-033`: `54e30684117548af63b48916e67d68e585a9016320018fd01485e036ff9f1f45`.

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
