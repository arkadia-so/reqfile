# FAIL_FAST

> **Alpha: do not use yet.** Written with an agent and not yet reviewed or
> measured by hand; its checks and examples may be wrong. It is excluded in
> [`../../.reqfile/config.yaml`](../../.reqfile/config.yaml), so nothing
> checks, evaluates or imports it until it is reviewed.

**Must:** No silent fallbacks, no swallowed errors, no default values that hide a failure.

**Ref:** Shore, "Fail Fast"; Erlang, "let it crash"

Checks: command, decision, in [`Reqfile.yaml`](Reqfile.yaml), with their
assets and 6 labeled examples in [`.reqfile/FAIL_FAST/`](.reqfile/FAIL_FAST/).
