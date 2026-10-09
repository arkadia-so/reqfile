# COMPOSITION

> **Alpha: do not use yet.** Written with an agent and not yet reviewed or
> measured by hand; its checks and examples may be wrong. It is excluded in
> [`../../.reqfile/config.yaml`](../../.reqfile/config.yaml), so nothing
> checks, evaluates or imports it until it is reviewed.

**Must:** No implementation inheritance in application code; combine pieces through shared interfaces and data formats. Inheriting from a base a framework requires (Pydantic BaseModel, Exception or Error, ORM declarative bases) is allowed.

**Ref:** Gang of Four; McIlroy, Unix philosophy

Checks: command, in [`Reqfile.yaml`](Reqfile.yaml), with their
assets and 5 labeled examples in [`.reqfile/COMPOSITION/`](.reqfile/COMPOSITION/).
