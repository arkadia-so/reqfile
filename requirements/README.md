# Requirement packages

Shareable requirements maintained next to reqfile, each a folder with the
layout of any package: a `Reqfile.yaml` defining it, its check assets and
labeled examples under `.reqfile/<ID>/`, and a README.

| Package | Requirement | Status |
|---|---|---|
| [`composition/`](composition/) | COMPOSITION | alpha |
| [`contracts/`](contracts/) | CONTRACTS | alpha |
| [`decomplect/`](decomplect/) | DECOMPLECT | alpha |
| [`fail-fast/`](fail-fast/) | FAIL_FAST | alpha |
| [`fast-feedback/`](fast-feedback/) | FAST_FEEDBACK | reviewed, measured on Arkadia |
| [`functional-core/`](functional-core/) | FUNCTIONAL_CORE | alpha |
| [`parse-dont-validate/`](parse-dont-validate/) | PARSE_DONT_VALIDATE | alpha |
| [`simulation/`](simulation/) | SIMULATION | alpha |
| [`unused-code/`](unused-code/) | UNUSED_CODE | alpha |
| [`replay-safe/`](replay-safe/) | REPLAY_SAFE | executable scenario contract |
| [`concurrent-transitions/`](concurrent-transitions/) | CONCURRENT_TRANSITIONS | executable scenario contract |
| [`durable-work/`](durable-work/) | DURABLE_WORK | executable scenario contract |
| [`recoverable-effects/`](recoverable-effects/) | RECOVERABLE_EFFECTS | executable scenario contract |
| [`scoped-work/`](scoped-work/) | SCOPED_WORK | executable scenario contract |
| [`data-isolation/`](data-isolation/) | DATA_ISOLATION | executable scenario contract |

**The eight packages marked alpha are not ready for adoption.** They were written with
an agent and have not been reviewed or measured one by one by hand.
[`../.reqfile/config.yaml`](../.reqfile/config.yaml) excludes each of them, so
reqfile's own runs neither check nor evaluate them, and neither `--use` nor a
use block can import them.

To review one, delete its line from that config, then measure it with
`reqfile eval --only <ID>` and fix its checks and examples until it holds.
From then on CI evaluates it with reqfile's own requirements, and it can be
imported with a use block.

The six reliability requirements are independent promises. Their shared checker runs
project scenarios and verifies complete, successful coverage of the declared obligations.
It does not infer behavioral correctness from source patterns or certify the assertions
in a project's tests. Each package documents its boundary decisions, evidence and limits.
Select a language or library adapter without inventing another requirement ID; bind
actual operations and tests in the adopting repository. See
[`../adapters/`](../adapters/) for the four roles and the strict execution contract.

All nine original packages now expose named adapter exports while preserving their
previous checks and evidence status. Migration alone is not a validation of an alpha rule.
