import 'dart:async';
import 'dart:convert';

import 'package:smoldot_provider/smoldot_provider.dart';

/// A JSON-RPC error returned by the peer.
class JsonRpcException implements Exception {
  JsonRpcException(this.code, this.message, [this.data]);

  factory JsonRpcException.fromJson(Map<String, dynamic> json) =>
      JsonRpcException(
        (json['code'] as num?)?.toInt() ?? 0,
        json['message']?.toString() ?? 'JSON-RPC error',
        json['data'],
      );

  /// JSON-RPC error code.
  final int code;

  /// Human-readable message.
  final String message;

  /// Optional structured error data.
  final Object? data;

  @override
  String toString() => 'JsonRpcException($code): $message';
}

/// A request/response + subscription JSON-RPC client over a [JsonRpcProvider].
///
/// Owns request ids and subscription correlation: a single read-loop over the
/// provider's `onMessage` routes responses by `id` and notifications by
/// `params.subscription`. This is the consumer the TrUAPI Chain/StatementStore
/// handlers run on top of a smoldot light client, mirroring substrate-connect /
/// polkadot-api.
class JsonRpcClient {
  JsonRpcClient(JsonRpcProvider provider) {
    _connection = provider(_onMessage);
  }

  late final JsonRpcConnection _connection;
  int _nextId = 1;
  final Map<int, Completer<Map<String, dynamic>>> _pending = {};
  final Map<String, StreamController<dynamic>> _subscriptions = {};
  // Notifications that arrive in the microtask window between a subscribe
  // response and the controller being registered are buffered here, then
  // flushed on registration.
  final Map<String, List<dynamic>> _orphanNotifications = {};
  bool _closed = false;

  /// Send a JSON-RPC request and resolve with its `result`.
  ///
  /// Throws [JsonRpcException] on a JSON-RPC error response, or [StateError]
  /// once [close] has been called.
  Future<Object?> request(
    String method, [
    List<Object?> params = const [],
  ]) async {
    if (_closed) throw StateError('JsonRpcClient is closed');
    final id = _nextId++;
    final completer = Completer<Map<String, dynamic>>();
    _pending[id] = completer;
    _connection.send(
      jsonEncode(
          {'jsonrpc': '2.0', 'id': id, 'method': method, 'params': params}),
    );
    final message = await completer.future;
    final error = message['error'];
    if (error is Map<String, dynamic>) {
      throw JsonRpcException.fromJson(error);
    }
    return message['result'];
  }

  /// Start a subscription. Returns its id and a broadcast-free stream of
  /// notification payloads (`params.result`). Cancelling the stream sends
  /// [unsubscribeMethod] with the subscription id.
  Future<(String id, Stream<Object?> notifications)> subscribe(
    String method,
    List<Object?> params,
    String unsubscribeMethod,
  ) async {
    final subscriptionId = (await request(method, params)).toString();
    final controller = StreamController<Object?>(
      onCancel: () {
        _subscriptions.remove(subscriptionId);
        if (!_closed) {
          // Fire-and-forget the unsubscribe.
          request(unsubscribeMethod, [subscriptionId]).ignore();
        }
      },
    );
    _subscriptions[subscriptionId] = controller;
    final buffered = _orphanNotifications.remove(subscriptionId);
    if (buffered != null) {
      for (final item in buffered) {
        controller.add(item);
      }
    }
    return (subscriptionId, controller.stream);
  }

  void _onMessage(String raw) {
    if (_closed) return;
    final Map<String, dynamic> message;
    try {
      message = jsonDecode(raw) as Map<String, dynamic>;
    } catch (_) {
      return; // ignore malformed frames
    }

    final id = message['id'];
    if (id != null) {
      _pending.remove((id as num).toInt())?.complete(message);
      return;
    }

    // Notification: { method, params: { subscription, result } }.
    final params = message['params'];
    if (params is Map<String, dynamic>) {
      final subscription = params['subscription']?.toString();
      if (subscription == null) return;
      final result = params['result'];
      final controller = _subscriptions[subscription];
      if (controller != null) {
        controller.add(result);
      } else {
        (_orphanNotifications[subscription] ??= []).add(result);
      }
    }
  }

  /// Stop the client: closes the provider connection and all subscription
  /// streams, and fails any in-flight requests.
  Future<void> close() async {
    if (_closed) return;
    _closed = true;
    for (final completer in _pending.values) {
      if (!completer.isCompleted) {
        completer.completeError(StateError('JsonRpcClient closed'));
      }
    }
    _pending.clear();
    for (final controller in _subscriptions.values) {
      await controller.close();
    }
    _subscriptions.clear();
    _orphanNotifications.clear();
    _connection.disconnect();
  }
}
