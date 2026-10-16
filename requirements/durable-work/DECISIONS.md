# DURABLE_WORK

Accepted work survives process failure until it reaches an explicit terminal outcome.

Required project scenarios:

- `accepted-restart`: accept work, stop its worker before completion, restart from durable
  state, and assert the work remains discoverable and is completed or explicitly rejected.
- `retryable-failure`: inject a transient failure and assert accepted work stays pending
  and can later reach its terminal outcome.
- `terminal-outcome`: assert the durable terminal result distinguishes success from an
  explicit permanent refusal and no longer appears as pending work.

Acceptance and terminal outcomes are project contracts. In-memory retries, graceful
shutdown and background promises alone do not establish persistence across failure.
Restart tests must discard volatile worker state rather than reuse the original object.

Effect additionally requires `runtime-restart`: discard the Effect runtime and reconstruct
the worker from persisted state. Riverpod additionally requires `container-restart`:
discard the provider container and reconstruct accepted work from persisted state.

Execution and limitations follow [the scenario contract](../../adapters/scenarios/DECISIONS.md).
