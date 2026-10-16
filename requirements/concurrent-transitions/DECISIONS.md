# CONCURRENT_TRANSITIONS

Competing transitions on shared state preserve its declared invariants.

Required project scenarios:

- `competing-transitions`: force overlapping conflicting transitions and inspect every outcome.
- `invariant-after-contention`: assert the resulting shared state satisfies declared
  invariants after contention, including rejected or retried transitions and persisted
  state where applicable.

The project supplies a deterministic contention barrier, shared state and invariants.
Merely starting promises or fibers at once does not demonstrate overlap. This guarantee
does not prescribe transactions, compare-and-swap or a locking implementation.

Effect additionally requires `fiber-contention`: force overlap through the application's
real Effect concurrency boundary. Riverpod additionally requires `provider-contention`:
force overlapping provider mutations and assert the final state and outcomes.

Execution and limitations follow [the scenario contract](../../adapters/scenarios/DECISIONS.md).
