# REPLAY_SAFE

Repeated delivery of the same identified operation does not repeat its business effect,
while distinct operations can each take effect.

Required project scenarios:

- `same-operation`: deliver the same operation more than once; assert one business effect.
- `distinct-operations`: deliver distinct operation identities; assert each intended effect.
- `retry-after-failure`: retry after a failure at the effect/acknowledgement boundary;
  assert the final effect is neither duplicated nor lost.

The project defines operation identity, business effect and the applicable lifetime.
No storage technology or key spelling is required. Retries and acknowledgements alone
are not evidence. Concurrent duplicates also need CONCURRENT_TRANSITIONS.

The Effect adapter additionally requires `effect-retry`: exercise the actual Effect
retry boundary around the operation and assert the same business effect count.
The Riverpod adapter additionally requires `provider-recreation`: recreate the provider
during a retry and assert the same operation does not produce an additional effect.

Execution and limitations follow [the scenario contract](../../adapters/scenarios/DECISIONS.md).
