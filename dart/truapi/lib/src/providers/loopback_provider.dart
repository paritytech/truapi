/// In-memory loopback transport: two linked [Provider]s that deliver each
/// other's frames. Used by tests and local harnesses; no host process needed.
library;

import 'dart:async';
import 'dart:typed_data';

import '../transport.dart';

/// A pair of [Provider]s wired back-to-back. A frame posted on [client] is
/// delivered to [host]'s subscribers, and vice versa, on a microtask to mimic
/// the asynchrony of a real channel.
class LoopbackChannel {
  LoopbackChannel() {
    _client._peer = _host;
    _host._peer = _client;
  }

  final _LoopbackEndpoint _client = _LoopbackEndpoint();
  final _LoopbackEndpoint _host = _LoopbackEndpoint();

  /// The product/client side of the channel.
  Provider get client => _client;

  /// The host side of the channel.
  Provider get host => _host;
}

class _LoopbackEndpoint extends Provider {
  _LoopbackEndpoint? _peer;
  final Set<MessageHandler> _listeners = {};
  final Set<CloseHandler> _closeListeners = {};
  Object? _closed;

  void _deliver(Uint8List message) {
    if (_closed != null) return;
    for (final listener in _listeners.toList()) {
      listener(message);
    }
  }

  @override
  void postMessage(Uint8List message) {
    final closed = _closed;
    if (closed != null) throw closed;
    final peer = _peer;
    if (peer == null) return;
    final copy = Uint8List.fromList(message);
    scheduleMicrotask(() => peer._deliver(copy));
  }

  @override
  CancelFn subscribe(MessageHandler onMessage) {
    if (_closed != null) return () {};
    _listeners.add(onMessage);
    return () => _listeners.remove(onMessage);
  }

  @override
  CancelFn? subscribeClose(CloseHandler onClose) {
    final closed = _closed;
    if (closed != null) {
      onClose(closed);
      return () {};
    }
    _closeListeners.add(onClose);
    return () => _closeListeners.remove(onClose);
  }

  @override
  void dispose() {
    if (_closed != null) return;
    _closed = StateError('loopback endpoint disposed');
    for (final listener in _closeListeners.toList()) {
      listener(_closed!);
    }
    _listeners.clear();
    _closeListeners.clear();
  }
}
