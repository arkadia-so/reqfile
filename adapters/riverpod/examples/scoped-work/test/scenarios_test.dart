import 'dart:async';
import 'package:riverpod/riverpod.dart';
import 'package:test/test.dart';

class PendingRequest {
  final response = Completer<String>();
  int releases = 0;
  String? committed;
  bool active = true;
}

({ProviderContainer container, PendingRequest request}) startRequest() {
  final request = PendingRequest();
  final provider = FutureProvider<String>((ref) async {
    ref.onDispose(() {
      request.active = false;
      request.releases += 1;
    });
    final value = await request.response.future;
    if (ref.mounted) request.committed = value;
    return value;
  });
  final container = ProviderContainer();
  container.listen(provider, (_, _) {}, fireImmediately: true);
  return (container: container, request: request);
}

void main() {
  test('owner close stops active work', () {
    final owner = startRequest();
    owner.container.dispose();
    expect(owner.request.active, isFalse);
    expect(owner.request.releases, 1);
    owner.request.response.complete('closed');
  });

  test('late completion cannot commit', () async {
    final owner = startRequest();
    owner.container.dispose();
    owner.request.response.complete('obsolete response');
    await Future<void>.delayed(Duration.zero);
    expect(owner.request.committed, isNull);
  });

  test('failure cleans up the owned resource once', () async {
    var releases = 0;
    final provider = FutureProvider<String>((ref) async {
      ref.onDispose(() { releases += 1; });
      throw StateError('dependency unavailable');
    });
    final container = ProviderContainer(retry: (_, _) => null);
    await expectLater(container.read(provider.future), throwsStateError);
    container.dispose();
    expect(releases, 1);
  });

  test('provider disposal rejects the pending result', () async {
    final owner = startRequest();
    owner.container.dispose();
    expect(owner.request.releases, 1);
    owner.request.response.complete('finished after disposal');
    await Future<void>.delayed(Duration.zero);
    expect(owner.request.committed, isNull);
    expect(owner.request.active, isFalse);
  });
}
