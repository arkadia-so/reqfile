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

**Every package here but `fast-feedback/` is alpha: do not use them yet.** They were written with
an agent and have not been reviewed or measured one by one by hand.
[`../.reqfile/config.yaml`](../.reqfile/config.yaml) excludes each of them, so
reqfile's own runs neither check nor evaluate them, and neither `--use` nor a
use block can import them.

To review one, delete its line from that config, then measure it with
`reqfile eval --only <ID>` and fix its checks and examples until it holds.
From then on CI evaluates it with reqfile's own requirements, and it can be
imported with a use block.
