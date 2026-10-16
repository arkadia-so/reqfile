# Effect scenario adapter

Composes the TypeScript adapter, checks the project's resolved Effect package and
adds one Effect-specific obligation to each reliability guarantee. The installed
version must be stable major 4; package resolution follows Node from the checked
project, including workspace and package-manager links. An unresolved or unsupported
version is a tool error. Python 3.10+ and Node are required.

| Export | Additional scenario |
|---|---|
| `replay-safe` | `effect-retry` |
| `concurrent-transitions` | `fiber-contention` |
| `durable-work` | `runtime-restart` |
| `recoverable-effects` | `interrupted-sibling` |
| `scoped-work` | `scope-interruption` |
| `data-isolation` | `request-context` |

Each requirement's DECISIONS.md describes the assertion the scenario should make.
The project supplies real Effect services, scoped fibers, fault injection and tests.
Using `Effect.retry`, `forkScoped` or a service context is not itself evidence that
the corresponding business guarantee holds.

The [scoped-work example](examples/scoped-work/) pins Effect 4.0.1 and Vitest 5.0.3
with an npm lockfile; its validation uses Node 24.12.0.
It exercises actual Scope closure, child interruption, AbortSignal propagation,
late Promise completion and resource finalization. Install its package dependencies
and use the `scoped-work` export with the provided binding. This example establishes
the adapter integration at those versions; it does not validate every Effect API or
all six business guarantees for an arbitrary application.

See the [scenario contract](../scenarios/DECISIONS.md) for evidence limits.
