# Independent protocol confirmation 1

These fixtures were authored from the requirement decisions and scenario contract without
reading the checker or other corpora. Each `files/` directory is a standalone project.
Its command executes Python assertion tests; the report or coverage defect is deliberate.
`case.json` records the expected scenario-adapter exit code. These are tool-error cases,
kept apart from requirement examples because `example.yaml` has no tool-error label.

Run each project's binding command with a fresh `REQFILE_SCENARIO_REPORT` path. The runner
returns 0 in all six cases, while the scenario adapter must return 2. A successful command
alone cannot establish successful execution of all declared scenarios.
