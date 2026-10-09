# FUNCTIONAL_CORE

> **Alpha: do not use yet.** Written with an agent and not yet reviewed or
> measured by hand; its checks and examples may be wrong. It is excluded in
> [`../../.reqfile/config.yaml`](../../.reqfile/config.yaml), so nothing
> checks, evaluates or imports it until it is reviewed.

**Must:** State machines where the order of events matters (timeouts, retries, sync, multi-step workflows) are pure step functions named `step`, taking (state, event) and returning (state, effects), under a core/ folder; code under core/ imports no I/O module, and only the shell performs I/O and executes effects. Other code uses ordinary pure functions where it can and focused tests.

**Ref:** Bernhardt, "Boundaries"; Elm Architecture

Checks: command, in [`Reqfile.yaml`](Reqfile.yaml), with their
assets and 7 labeled examples in [`.reqfile/FUNCTIONAL_CORE/`](.reqfile/FUNCTIONAL_CORE/).
