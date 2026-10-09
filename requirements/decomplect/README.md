# DECOMPLECT

> **Alpha: do not use yet.** Written with an agent and not yet reviewed or
> measured by hand; its checks and examples may be wrong. It is excluded in
> [`../../.reqfile/config.yaml`](../../.reqfile/config.yaml), so nothing
> checks, evaluates or imports it until it is reviewed.

**Must:** A unit (function, module or type) does not braid concerns that change independently, such as I/O with business rules or parsing with computation; a coordinator may connect operations that each live in their own unit.

**Ref:** Hickey, "Simple Made Easy"

Checks: decision, in [`Reqfile.yaml`](Reqfile.yaml), with their
assets and 5 labeled examples in [`.reqfile/DECOMPLECT/`](.reqfile/DECOMPLECT/).
