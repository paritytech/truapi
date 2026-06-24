import 'dart:async';
import 'dart:convert';

import 'package:smoldot_provider/smoldot_provider.dart';

/// A controllable JSON-RPC chain for tests: answers requests from a
/// `method -> handler` table (recording every request it sees) and can push
/// subscription notifications. No FFI / native library involved.
class FakeChain {
  void Function(String message)? _onMessage;
  bool _closed = false;

  /// Every decoded request the client sent, in order.
  final List<Map<String, dynamic>> requests = [];

  /// Per-method responders. A responder receives the request params and returns
  /// the JSON-RPC `result`; throwing yields a JSON-RPC error response.
  final Map<String, Object? Function(List<Object?> params)> responders = {};

  JsonRpcProvider get provider => (onMessage) {
        _onMessage = onMessage;
        return _FakeConnection(this);
      };

  void _send(String message) {
    final request = jsonDecode(message) as Map<String, dynamic>;
    requests.add(request);
    final method = request['method'] as String;
    final params = (request['params'] as List).cast<Object?>();
    final responder = responders[method];
    scheduleMicrotask(() {
      if (_closed) return;
      if (responder == null) {
        _emit({
          'jsonrpc': '2.0',
          'id': request['id'],
          'error': {'code': -32601, 'message': 'no responder for $method'},
        });
        return;
      }
      try {
        _emit({
          'jsonrpc': '2.0',
          'id': request['id'],
          'result': responder(params),
        });
      } catch (error) {
        _emit({
          'jsonrpc': '2.0',
          'id': request['id'],
          'error': {'code': -32000, 'message': '$error'},
        });
      }
    });
  }

  /// Push a subscription notification for [subscription].
  void notify(
    String subscription,
    Object? result, {
    String method = 'subscription_event',
  }) {
    _emit({
      'jsonrpc': '2.0',
      'method': method,
      'params': {'subscription': subscription, 'result': result},
    });
  }

  void _emit(Object json) => _onMessage?.call(jsonEncode(json));

  /// The recorded params of the first request to [method].
  List<Object?> paramsFor(String method) =>
      (requests.firstWhere((r) => r['method'] == method)['params'] as List)
          .cast<Object?>();
}

class _FakeConnection implements JsonRpcConnection {
  _FakeConnection(this._chain);
  final FakeChain _chain;

  @override
  void send(String message) => _chain._send(message);

  @override
  void disconnect() => _chain._closed = true;
}
