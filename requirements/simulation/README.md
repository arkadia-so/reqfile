# SIMULATION

> **Alpha: do not use yet.** Written with an agent and not yet reviewed or
> measured by hand; its checks and examples may be wrong. It is excluded in
> [`../../.reqfile/config.yaml`](../../.reqfile/config.yaml), so nothing
> checks, evaluates or imports it until it is reviewed.

**Must:** Every step function has a deterministic simulation test, named with a `simulation` prefix, that runs seeded random event sequences and checks invariants after every step.

**Ref:** FoundationDB; TigerBeetle

Checks: command, in [`Reqfile.yaml`](Reqfile.yaml), with their
assets and 6 labeled examples in [`.reqfile/SIMULATION/`](.reqfile/SIMULATION/).
