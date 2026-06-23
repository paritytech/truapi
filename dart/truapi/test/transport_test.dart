import 'dart:typed_data';

import 'package:truapi/src/providers/loopback_provider.dart';
import 'package:truapi/src/result.dart';
import 'package:truapi/src/scale.dart' as s;
import 'package:truapi/src/transport.dart';
import 'package:test/test.dart';

/// Minimal host that answers raw wire frames, for driving the client transport.
class _FakeHost {
  _FakeHost(this.provider) {
    provider.subscribe(_onFrame);
  }

  final Provider provider;
  final Map<int, void Function(String requestId, Uint8List payload)> handlers =
      {};

  void on(int id, void Function(String requestId, Uint8List payload) handler) {
    handlers[id] = handler;
  }

  void send(String requestId, int id, Uint8List payload) {
    provider.postMessage(
        encodeWireMessage(ProtocolMessage(requestId, id, payload)));
  }

  void _onFrame(Uint8List message) {
    final m = decodeWireMessage(message);
    handlers[m.id]?.call(m.requestId, m.value);
  }
}

void main() {
  test('request resolves with decoded Ok payload', () async {
    final channel = LoopbackChannel();
    final host = _FakeHost(channel.host);
    final transport =
        createTransport(channel.client, truapiVersion: 3, codecVersion: 1);

    const ids = RequestFrameIds(request: 22, response: 23);
    // Host echoes a Result::Ok(u32) when it sees the request frame.
    host.on(ids.request, (requestId, payload) {
      final n = s.u32.dec(payload);
      host.send(requestId, ids.response,
          s.result(s.u32, s.str).enc(Ok<int, String>(n + 1)));
    });

    final result = await transport.request<int, String>(
      ids: ids,
      payload: s.u32.enc(41),
      decodeResponse: (p) => s.result(s.u32, s.str).dec(p),
    );
    expect(result, const Ok<int, String>(42));
  });

  test('request resolves with decoded Err payload', () async {
    final channel = LoopbackChannel();
    final host = _FakeHost(channel.host);
    final transport =
        createTransport(channel.client, truapiVersion: 3, codecVersion: 1);

    const ids = RequestFrameIds(request: 8, response: 9);
    host.on(ids.request, (requestId, payload) {
      host.send(requestId, ids.response,
          s.result(s.u8, s.str).enc(const Err<int, String>('denied')));
    });

    final result = await transport.request<int, String>(
      ids: ids,
      payload: Uint8List(0),
      decodeResponse: (p) => s.result(s.u8, s.str).dec(p),
    );
    expect(result, const Err<int, String>('denied'));
  });

  test('subscription delivers items then completes on interrupt', () async {
    final channel = LoopbackChannel();
    final host = _FakeHost(channel.host);
    final transport =
        createTransport(channel.client, truapiVersion: 3, codecVersion: 1);

    const ids =
        SubscriptionFrameIds(start: 18, stop: 19, interrupt: 20, receive: 21);
    host.on(ids.start, (requestId, payload) {
      host.send(requestId, ids.receive, s.u8.enc(1));
      host.send(requestId, ids.receive, s.u8.enc(2));
      host.send(requestId, ids.interrupt, Uint8List(0));
    });

    final received = <int>[];
    final done = <bool>[];
    transport.subscribeRaw(
      ids: ids,
      payload: Uint8List(0),
      onReceive: (p) => received.add(s.u8.dec(p)),
      onInterrupt: (_) => done.add(true),
    );

    await Future<void>.delayed(const Duration(milliseconds: 10));
    expect(received, [1, 2]);
    expect(done, [true]);
  });

  test('unsubscribe sends a stop frame', () async {
    final channel = LoopbackChannel();
    final host = _FakeHost(channel.host);
    final transport =
        createTransport(channel.client, truapiVersion: 3, codecVersion: 1);

    const ids =
        SubscriptionFrameIds(start: 18, stop: 19, interrupt: 20, receive: 21);
    final stops = <String>[];
    host.on(ids.start, (requestId, payload) {});
    host.on(ids.stop, (requestId, payload) => stops.add(requestId));

    final sub = transport.subscribeRaw(
      ids: ids,
      payload: Uint8List(0),
      onReceive: (_) {},
    );
    await Future<void>.delayed(const Duration(milliseconds: 5));
    sub.unsubscribe();
    await Future<void>.delayed(const Duration(milliseconds: 5));
    expect(stops, [sub.subscriptionId]);
  });

  test('auto-handshake answers inbound handshake request', () async {
    final channel = LoopbackChannel();
    final host = _FakeHost(channel.host);
    const ids = RequestFrameIds(request: 0, response: 1);

    final responses = <Uint8List>[];
    host.on(ids.response, (_, payload) => responses.add(payload));

    createTransport(
      channel.client,
      truapiVersion: 3,
      codecVersion: 1,
      handshake: HandshakeResponder(
        ids: ids,
        respond: (requestPayload, codecVersion) =>
            s.u32.enc(codecVersion), // stand-in response body
      ),
    );

    // Host pings the client with a handshake request frame.
    host.send('p:host', ids.request, s.u32.enc(1));
    await Future<void>.delayed(const Duration(milliseconds: 10));
    expect(responses.length, 1);
    expect(s.u32.dec(responses.first), 1);
  });
}
