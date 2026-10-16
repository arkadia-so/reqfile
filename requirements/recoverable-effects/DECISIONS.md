# RECOVERABLE_EFFECTS

When an operation partially applies several effects, the partial outcome is observable
and can be recovered without losing or repeating its intended business effects.

Required project scenarios:

- `partial-failure`: fail after one effect succeeds and assert the durable or observable
  outcome records enough information to distinguish partial success from total failure.
- `recovery`: recover the partial operation and assert its declared final business outcome.
- `repeated-recovery`: repeat recovery and assert completed effects are not duplicated.

Recovery may compensate or finish forward. The project defines which final outcomes are
valid and what evidence an operator or worker consumes. Logging an exception without
identifying the applied effects does not establish recoverability.

Effect additionally requires `interrupted-sibling`: interrupt or fail a concurrent fiber
after a sibling effect commits and recover the recorded partial outcome. Riverpod
additionally requires `disposed-mutation`: dispose a provider during a partially applied
mutation and recover its observable outcome in a new owner.

Execution and limitations follow [the scenario contract](../../adapters/scenarios/DECISIONS.md).
