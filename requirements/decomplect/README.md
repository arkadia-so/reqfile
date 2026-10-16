# DECOMPLECT

> **Alpha: do not use yet.** Written with an agent and not yet reviewed or
> measured by hand; its checks and examples may be wrong. It is excluded in
> [`../../.reqfile/config.yaml`](../../.reqfile/config.yaml), so nothing
> checks, evaluates or imports it until it is reviewed.

**Must:** A unit (function, module or type) does not braid concerns that change independently, such as I/O with business rules or parsing with computation; a coordinator may connect operations that each live in their own unit.

**Ref:** Hickey, "Simple Made Easy"

Checks: decision, in [`Reqfile.yaml`](Reqfile.yaml), with their
assets and 5 labeled examples in [`.reqfile/DECOMPLECT/`](.reqfile/DECOMPLECT/).

## Check composition

The checks are named exports of [Adapter.yaml](.reqfile/DECOMPLECT/Adapter.yaml).
The requirement, command arguments, modes, assets and labeled examples retain
their existing behavior. Consumers can select an export with an `adapter`
check or compose it with other adapters; an explicit `checks` override still
replaces all inherited checks. This format migration does not change the
checker's evidence or adoption status.
