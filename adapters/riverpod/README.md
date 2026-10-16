# Riverpod scenario adapter

Composes the Dart adapter, checks Dart's resolved dependency graph and adds one
provider-specific obligation to each reliability guarantee. The resolved `riverpod`
package must be stable major 3. An unresolved or unsupported version is a tool error;
a version constraint in `pubspec.yaml` alone is not accepted as an installed version.
Requires Python 3.10+ and a direct Dart command, such as `["dart"]`.

This export runs Dart provider tests, not `flutter test` or widget tests. A Flutter
application can put its provider scenarios in a Dart-testable package, or bind its
existing Flutter runner through the generic JUnit adapter. Flutter runner report
normalization is not implemented by this library adapter.

| Export | Additional scenario |
|---|---|
| `replay-safe` | `provider-recreation` |
| `concurrent-transitions` | `provider-contention` |
| `durable-work` | `container-restart` |
| `recoverable-effects` | `disposed-mutation` |
| `scoped-work` | `provider-dispose` |
| `data-isolation` | `provider-identity` |

Each requirement's DECISIONS.md specifies the scenario's intended assertions.
The project owns provider construction, containers, fake clocks and dependency
responses. An `autoDispose`, `onDispose` or `mounted` spelling alone proves nothing
about business state or a late callback outside the provider.

The [scoped-work example](examples/scoped-work/) pins Riverpod 3.0.3 and Dart test
1.26.3, compatible with Dart 3.9. Run `dart pub get` there, then adopt the
`scoped-work` export using its binding. Its tests use actual ProviderContainer,
FutureProvider, onDispose and mounted behavior. Compatibility of every Riverpod
API or every SDK combination is not claimed.

See the [scenario contract](../scenarios/DECISIONS.md) for evidence limits.
