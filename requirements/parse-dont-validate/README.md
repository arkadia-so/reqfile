# PARSE_DONT_VALIDATE

> **Alpha: do not use yet.** Written with an agent and not yet reviewed or
> measured by hand; its checks and examples may be wrong. It is excluded in
> [`../../.reqfile/config.yaml`](../../.reqfile/config.yaml), so nothing
> checks, evaluates or imports it until it is reviewed.

**Must:** External data is parsed once where it enters, with a schema library (Zod, Pydantic, serde), into precise types passed inward; no untyped values (`any`, `Any`) cross that boundary.

**Ref:** King; Minsky, "make illegal states unrepresentable"

Checks: command, in [`Reqfile.yaml`](Reqfile.yaml), with their
assets and 5 labeled examples in [`.reqfile/PARSE_DONT_VALIDATE/`](.reqfile/PARSE_DONT_VALIDATE/).
