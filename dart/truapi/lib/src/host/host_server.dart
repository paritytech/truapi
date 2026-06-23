/// Host-side dispatcher for the TrUAPI wire protocol — the Dart port of
/// `js/packages/truapi-host/src/index.ts`.
///
/// A host implements the typed handler interfaces in the generated
/// `host.dart` and wires them to a [Provider] with `createTruapiServer`. This
/// file is the transport-agnostic core: it decodes inbound frames, routes them
/// by wire id to request/subscription handlers, and emits response, receive,
/// and interrupt frames back through the same provider.
library;

import 'dart:async';
import 'dart:typed_data';

import '../transport.dart';

/// Per-call context handed to every host handler. Carries the wire
/// `requestId` so handlers can correlate audit logs or per-call state.
class CallContext {
  const CallContext(this.requestId);

  /// Transport-assigned request id for the originating client frame.
  final String requestId;
}

/// Cleanup invoked when a subscription stops (client stop frame, host
/// interrupt, or provider close).
typedef SubscriptionCleanup = void Function();

/// Raw byte port a subscription handler uses to push receive/interrupt frames
/// back to the client.
abstract class SubscriptionFramePort {
  /// Emit a receive frame carrying the supplied encoded item bytes.
  void sendReceive(Uint8List payload);

  /// Emit an interrupt frame carrying the supplied encoded reason bytes and
  /// close the subscription locally.
  void sendInterrupt(Uint8List payload);

  /// `true` once the subscription has ended.
  bool get isClosed;
}

/// A dispatch entry: either a one-shot [RequestEntry] or a [SubscriptionEntry].
sealed class HostDispatchEntry {
  const HostDispatchEntry();
}

/// Handler entry for a one-shot request method. The dispatcher decodes inbound
/// bytes, runs [handle], and forwards the returned bytes as the response frame.
class RequestEntry extends HostDispatchEntry {
  const RequestEntry({required this.ids, required this.handle});

  /// Wire discriminants for this request method.
  final RequestFrameIds ids;

  /// Decode the request bytes, run the handler, and produce the response bytes.
  final Future<Uint8List> Function(CallContext ctx, Uint8List payload) handle;
}

/// Handler entry for a subscription method. [start] decodes the start payload,
/// subscribes the handler's stream, and returns a cleanup function.
class SubscriptionEntry extends HostDispatchEntry {
  const SubscriptionEntry({required this.ids, required this.start});

  /// Wire discriminants for this subscription method.
  final SubscriptionFrameIds ids;

  /// Decode the start bytes, begin streaming through [port], and return the
  /// cleanup that tears the stream down.
  final FutureOr<SubscriptionCleanup> Function(
    CallContext ctx,
    Uint8List payload,
    SubscriptionFramePort port,
  ) start;
}

/// Optional hooks for visibility into protocol drift or handler errors.
class HostServerHooks {
  const HostServerHooks({
    this.onUnknownFrame,
    this.onRequestHandlerError,
    this.onSubscriptionStartError,
  });

  /// Called when an inbound frame's wire id is not in the dispatch table.
  final void Function(int id, Uint8List value)? onUnknownFrame;

  /// Called when a request handler throws or rejects. No response frame is
  /// sent; the client request times out per its own policy.
  final void Function(RequestFrameIds ids, Object error, CallContext ctx)?
      onRequestHandlerError;

  /// Called when a subscription handler throws during `start`.
  final void Function(SubscriptionFrameIds ids, Object error, CallContext ctx)?
      onSubscriptionStartError;
}

/// Handle returned by [createHostServer].
abstract class TruapiHostServer {
  /// Detach provider listeners, drop pending subscription state, and release
  /// resources. Does not dispose the underlying [Provider]. Idempotent.
  void dispose();
}

/// Wire a host server to [provider], routing inbound frames to [entries].
TruapiHostServer createHostServer(
  Provider provider,
  List<HostDispatchEntry> entries, [
  HostServerHooks hooks = const HostServerHooks(),
]) =>
    _HostServer(provider, entries, hooks);

class _Pending implements _Slot {
  _Pending(this.port);
  @override
  final SubscriptionFramePort port;
  bool stopped = false;
}

class _Active implements _Slot {
  _Active(this.cleanup, this.port);
  final SubscriptionCleanup cleanup;
  @override
  final SubscriptionFramePort port;
}

abstract class _Slot {
  SubscriptionFramePort get port;
}

class _HostServer implements TruapiHostServer {
  _HostServer(this._provider, List<HostDispatchEntry> entries, this._hooks) {
    for (final entry in entries) {
      switch (entry) {
        case RequestEntry(:final ids):
          if (_byRequest.containsKey(ids.request)) {
            throw StateError('duplicate request wire id ${ids.request}');
          }
          _byRequest[ids.request] = entry;
        case SubscriptionEntry(:final ids):
          if (_byStart.containsKey(ids.start)) {
            throw StateError(
                'duplicate subscription start wire id ${ids.start}');
          }
          _byStart[ids.start] = entry;
          _stopIds.add(ids.stop);
      }
    }
    _unsubscribeMessage = _provider.subscribe(_handleInbound);
    _unsubscribeClose = _provider.subscribeClose((_) => dispose());
  }

  final Provider _provider;
  final HostServerHooks _hooks;
  final Map<int, RequestEntry> _byRequest = {};
  final Map<int, SubscriptionEntry> _byStart = {};
  final Set<int> _stopIds = {};
  final Map<String, _Slot> _subscriptions = {};
  bool _disposed = false;
  CancelFn? _unsubscribeMessage;
  CancelFn? _unsubscribeClose;

  void _send(String requestId, int id, Uint8List value) {
    if (_disposed) return;
    try {
      _provider.postMessage(
        encodeWireMessage(ProtocolMessage(requestId, id, value)),
      );
    } catch (_) {
      // Provider closed mid-send; disposal handles teardown.
    }
  }

  void _tearDown(String requestId) {
    final slot = _subscriptions[requestId];
    if (slot == null) return;
    if (slot is _Pending) {
      slot.stopped = true;
      return;
    }
    _subscriptions.remove(requestId);
    if (slot is _Active) {
      try {
        slot.cleanup();
      } catch (_) {
        // handler cleanup errors are isolated from the dispatcher
      }
    }
  }

  SubscriptionFramePort _makeFramePort(
          String requestId, SubscriptionFrameIds ids) =>
      _FramePort(this, requestId, ids);

  void _handleInbound(Uint8List message) {
    if (_disposed) return;
    final ProtocolMessage decoded;
    try {
      decoded = decodeWireMessage(message);
    } catch (_) {
      return;
    }
    final ctx = CallContext(decoded.requestId);

    final requestEntry = _byRequest[decoded.id];
    if (requestEntry != null) {
      final Future<Uint8List> pending;
      try {
        pending = requestEntry.handle(ctx, decoded.value);
      } catch (error) {
        _hooks.onRequestHandlerError?.call(requestEntry.ids, error, ctx);
        return;
      }
      pending.then(
        (bytes) => _send(decoded.requestId, requestEntry.ids.response, bytes),
        onError: (Object error) =>
            _hooks.onRequestHandlerError?.call(requestEntry.ids, error, ctx),
      );
      return;
    }

    final subEntry = _byStart[decoded.id];
    if (subEntry != null) {
      if (_subscriptions.containsKey(decoded.requestId)) {
        return; // duplicate start for the same requestId
      }
      final port = _makeFramePort(decoded.requestId, subEntry.ids);
      final pending = _Pending(port);
      _subscriptions[decoded.requestId] = pending;

      void finish(SubscriptionCleanup cleanup) {
        final current = _subscriptions[decoded.requestId];
        if (!identical(current, pending) ||
            _disposed ||
            pending.stopped ||
            port.isClosed) {
          if (identical(current, pending)) {
            _subscriptions.remove(decoded.requestId);
          }
          try {
            cleanup();
          } catch (_) {/* ignore */}
          return;
        }
        _subscriptions[decoded.requestId] = _Active(cleanup, port);
      }

      void fail(Object error) {
        if (identical(_subscriptions[decoded.requestId], pending)) {
          _subscriptions.remove(decoded.requestId);
        }
        _hooks.onSubscriptionStartError?.call(subEntry.ids, error, ctx);
      }

      final FutureOr<SubscriptionCleanup> startResult;
      try {
        startResult = subEntry.start(ctx, decoded.value, port);
      } catch (error) {
        fail(error);
        return;
      }
      if (startResult is Future<SubscriptionCleanup>) {
        startResult.then(finish, onError: fail);
      } else {
        finish(startResult);
      }
      return;
    }

    if (_stopIds.contains(decoded.id)) {
      _tearDown(decoded.requestId);
      return;
    }

    _hooks.onUnknownFrame?.call(decoded.id, decoded.value);
  }

  @override
  void dispose() {
    if (_disposed) return;
    _disposed = true;
    for (final entry in _subscriptions.entries.toList()) {
      _subscriptions.remove(entry.key);
      final slot = entry.value;
      if (slot is _Pending) {
        slot.stopped = true;
        continue;
      }
      if (slot is _Active) {
        try {
          slot.cleanup();
        } catch (_) {/* ignore */}
      }
    }
    try {
      _unsubscribeMessage?.call();
    } catch (_) {/* ignore */}
    try {
      _unsubscribeClose?.call();
    } catch (_) {/* ignore */}
  }
}

class _FramePort implements SubscriptionFramePort {
  _FramePort(this._server, this._requestId, this._ids);

  final _HostServer _server;
  final String _requestId;
  final SubscriptionFrameIds _ids;
  bool _closed = false;

  @override
  void sendReceive(Uint8List payload) {
    if (_closed || _server._disposed) return;
    _server._send(_requestId, _ids.receive, payload);
  }

  @override
  void sendInterrupt(Uint8List payload) {
    if (_closed || _server._disposed) return;
    _closed = true;
    _server._send(_requestId, _ids.interrupt, payload);
    // Host interrupted locally; drop the slot so later stop frames are no-ops.
    _server._subscriptions.remove(_requestId);
  }

  @override
  bool get isClosed => _closed || _server._disposed;
}
