# Dart scenario adapter

Exports the six reliability checks through `dart test`. The project binding's
command is `["dart"]`; the adapter appends `test --reporter=json`. Map obligations
to exact Dart test names, including their group names.

Only completed visible test cases count as scenario evidence. Framework load and
teardown cases do not satisfy obligations; their failures still prevent a pass.
Missing completion events, skipped cases and malformed output are tool errors.
This adapter does not infer state ownership or authorization from Dart syntax.

Requires Python 3.10+, Dart and `package:test` installed in the checked project.
The [Riverpod example](../riverpod/examples/scoped-work/) pins its test dependency
and demonstrates the real runner. For provider semantics, compose Riverpod.

See the [scenario contract](../scenarios/DECISIONS.md) for project inputs and limits.
