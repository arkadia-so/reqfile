# UNUSED_CODE

> **Alpha: do not use yet.** Written with an agent and not yet reviewed or
> measured by hand; its checks and examples may be wrong. It is excluded in
> [`../../.reqfile/config.yaml`](../../.reqfile/config.yaml), so nothing
> checks, evaluates or imports it until it is reviewed.

**Must:** No unused code; unused functions, types, imports, variables, exports and dependencies are deleted.

**Ref:** Beck, Extreme Programming (YAGNI)

Checks: command, in [`Reqfile.yaml`](Reqfile.yaml), with their
assets and 4 labeled examples in [`.reqfile/UNUSED_CODE/`](.reqfile/UNUSED_CODE/).

## Check composition

The checks are named exports of [Adapter.yaml](.reqfile/UNUSED_CODE/Adapter.yaml).
The requirement, command arguments, modes, assets and labeled examples retain
their existing behavior. Consumers can select an export with an `adapter`
check or compose it with other adapters; an explicit `checks` override still
replaces all inherited checks. This format migration does not change the
checker's evidence or adoption status.
