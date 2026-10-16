# DATA_ISOLATION

Reads, writes and identity changes preserve the project's authorization boundaries,
including cached and pending data.

Required project scenarios:

- `cross-identity-read`: attempt a read as a different identity or tenant and assert the
  data is inaccessible according to the project's authorization contract.
- `cross-identity-write`: attempt a mutation across that boundary and assert both the
  rejection and unchanged protected state.
- `identity-switch`: switch identity with cached data and a pending response, then assert
  neither old data nor the old response becomes visible or writable in the new identity.

The project defines principals, tenancy, allowed sharing and authorization decisions.
Public/shared data is permitted by explicit policy. This requirement does not mandate
one authentication library or a particular database isolation mechanism.

Effect additionally requires `request-context`: run different identities through the
actual Effect service/context boundary and assert their state remains isolated. Riverpod
additionally requires `provider-identity`: switch the authenticated provider dependency
with cached/pending data and assert the new identity never receives the old state.

Execution and limitations follow [the scenario contract](../../adapters/scenarios/DECISIONS.md).
