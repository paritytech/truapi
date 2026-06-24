import 'dart:convert';

import 'package:smoldot_provider/smoldot_provider.dart';
import 'package:truapi_smoldot/src/json_rpc_client.dart';
import 'package:test/test.dart';

/// In-memory [JsonRpcProvider] that records sent frames and lets a test deliver
/// inbound frames.
class _FakeProvider {
  void Function(String message)? _onMessage;
  final List<String> sent = [];
  bool disconnected = false;

  JsonRpcProvider get provider => (onMessage) {
        _onMessage = onMessage;
        return _FakeConnection(this);
      };

  void deliver(String message) => _onMessage?.call(message);

  /// The `id` of the most recently sent frame.
  Object? get lastId => (jsonDecode(sent.last) as Map<String, dynamic>)['id'];

  /// The `method` of the most recently sent frame.
  Object? get lastMethod =>
      (jsonDecode(sent.last) as Map<String, dynamic>)['method'];
}

class _FakeConnection implements JsonRpcConnection {
  _FakeConnection(this._provider);
  final _FakeProvider _provider;
  @override
  void send(String message) => _provider.sent.add(message);
  @override
  void disconnect() => _provider.disconnected = true;
}

void main() {
  late _FakeProvider fake;
  late JsonRpcClient client;

  setUp(() {
    fake = _FakeProvider();
    client = JsonRpcClient(fake.provider);
  });

  test('request resolves with the result', () async {
    final future = client.request('system_chain');
    fake.deliver(jsonEncode({'id': fake.lastId, 'result': 'Westend'}));
    expect(await future, 'Westend');
  });

  test('request throws JsonRpcException on an error response', () async {
    final future = client.request('bad_method');
    fake.deliver(jsonEncode({
      'id': fake.lastId,
      'error': {'code': -32601, 'message': 'Method not found'},
    }));
    await expectLater(
      future,
      throwsA(isA<JsonRpcException>()
          .having((e) => e.code, 'code', -32601)
          .having((e) => e.message, 'message', 'Method not found')),
    );
  });

  test('subscribe streams notifications and unsubscribes on cancel', () async {
    final subFuture = client.subscribe(
        'chainHead_v1_follow', [false], 'chainHead_v1_unfollow');
    fake.deliver(jsonEncode({'id': fake.lastId, 'result': 'sub-A'}));
    final (subId, stream) = await subFuture;
    expect(subId, 'sub-A');

    final items = <Object?>[];
    final sub = stream.listen(items.add);
    fake.deliver(jsonEncode({
      'method': 'chainHead_v1_followEvent',
      'params': {
        'subscription': 'sub-A',
        'result': {'event': 'initialized'},
      },
    }));
    await Future<void>.delayed(Duration.zero);
    expect(items, [
      {'event': 'initialized'},
    ]);

    await sub.cancel();
    expect(fake.lastMethod, 'chainHead_v1_unfollow');
  });

  test('notifications arriving before registration are buffered', () async {
    final subFuture = client.subscribe('m', const [], 'unsub');
    final reqId = fake.lastId;
    // Response (carries the subscription id) then a notification, both before
    // the subscribe continuation registers the controller.
    fake.deliver(jsonEncode({'id': reqId, 'result': 'sub-B'}));
    fake.deliver(jsonEncode({
      'method': 'x',
      'params': {'subscription': 'sub-B', 'result': 42},
    }));
    final (_, stream) = await subFuture;
    expect(await stream.first, 42);
  });

  test('close fails pending requests and disconnects', () async {
    final future = client.request('system_chain');
    // Attach the error expectation before closing so the failure isn't an
    // unhandled async error.
    final expectation = expectLater(future, throwsStateError);
    await client.close();
    expect(fake.disconnected, isTrue);
    await expectation;
  });
}
