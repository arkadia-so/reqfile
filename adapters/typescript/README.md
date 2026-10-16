# TypeScript scenario adapter

Exports the six reliability checks through Vitest. The project binding's command
names the runner prefix, for example `["npx", "--no-install", "vitest"]`; the
adapter appends `run --reporter=json --outputFile=<fresh-path>`.

Map obligations to exact Vitest `fullName` values, including suite names. A selected
test must execute once and pass. Pending, todo and skipped tests prevent a pass;
configuration errors and malformed reports are tool errors. This adapter does not
parse TypeScript or infer business guarantees from the source.

Requires Python 3.10+, a Node version supported by the installed Vitest, and Vitest itself.
The example uses Node 24.12.0. The runner protocol is exercised
with Vitest 5.0.3 by the [Effect example](../effect/examples/scoped-work/).
Other versions must emit the same complete JSON protocol; incompatible output
fails as a tool error. For library semantics, compose the Effect adapter.

See the [scenario contract](../scenarios/DECISIONS.md) for project inputs and limits.
