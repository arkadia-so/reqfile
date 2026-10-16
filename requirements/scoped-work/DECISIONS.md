# SCOPED_WORK

Tasks stop with their declared owner and cannot commit late results after that owner ends.

Required project scenarios:

- `owner-close`: close an owner while its task is active and assert the task is stopped
  or transferred explicitly to another live owner.
- `late-completion`: complete an uncancellable dependency after the owner ends and assert
  the obsolete result cannot mutate the former owner's state.
- `failure-cleanup`: fail a task and assert its owned resources are released exactly once.

An owner may be a request, scope, session, component or provider. Cancellation alone
does not prevent every late write; test the application's commit boundary. Durable work
can transfer to a persistent worker instead of being cancelled, with an explicit owner.

Effect additionally requires `scope-interruption`: close an Effect Scope with a live child
fiber and assert interruption, finalization and no late commit. Riverpod additionally
requires `provider-dispose`: dispose the actual provider/container with a pending
dependency and assert cleanup and rejection of its late completion.

Execution and limitations follow [the scenario contract](../../adapters/scenarios/DECISIONS.md).
