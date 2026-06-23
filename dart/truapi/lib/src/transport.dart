/// Byte-level transport for the TrUAPI wire protocol — the Dart port of
/// `js/packages/truapi/src/{transport,client}.ts`.
///
/// A [Provider] is a raw bidirectional pipe that ships SCALE-encoded wire
/// frames to and from the host. [createTransport] layers request/response
/// correlation and subscription lifecycle on top of it. The frame format is:
///
/// ```text
/// [ requestId : SCALE str ][ discriminant : u8 ][ payload : SCALE bytes ]
/// ```
library;

import 'dart:async';
import 'dart:typed_data';

import 'result.dart';
import 'scale.dart' as s;

/// Cancels a previously registered listener.
typedef CancelFn = void Function();

/// Raw inbound frame handler.
typedef MessageHandler = void Function(Uint8List message);

/// Provider-level close/failure handler.
typedef CloseHandler = void Function(Object error);

/// Raw message pipe abstraction the transport rides on. Concrete providers
/// (loopback for tests, a native bridge for Flutter, a `MessagePort` for web)
/// implement this and nothing more.
abstract class Provider {
  /// Send a complete SCALE-encoded wire frame to the peer.
  void postMessage(Uint8List message);

  /// Register a callback for inbound SCALE-encoded wire frames. Returns a
  /// function that removes the listener.
  CancelFn subscribe(MessageHandler onMessage);

  /// Register a callback for provider-level close/failure events. Optional;
  /// the default registers nothing.
  CancelFn? subscribeClose(CloseHandler onClose) => null;

  /// Release provider resources and close the underlying pipe.
  void dispose();
}

/// Wire discriminants for a one-shot request method.
class RequestFrameIds {
  const RequestFrameIds({required this.request, required this.response});

  /// Outbound request frame discriminant.
  final int request;

  /// Inbound response frame discriminant.
  final int response;
}

/// Wire discriminants for a subscription method.
class SubscriptionFrameIds {
  const SubscriptionFrameIds({
    required this.start,
    required this.stop,
    required this.interrupt,
    required this.receive,
  });

  /// Outbound start frame discriminant.
  final int start;

  /// Outbound stop frame discriminant.
  final int stop;

  /// Inbound interrupt (stream-end) frame discriminant.
  final int interrupt;

  /// Inbound item frame discriminant.
  final int receive;
}

/// Handle to a live subscription started through [Transport.subscribeRaw].
class Subscription {
  Subscription({required this.subscriptionId, required this.unsubscribe});

  /// Transport-assigned id for the subscription start frame.
  final String subscriptionId;

  /// Stop the subscription. Idempotent.
  final void Function() unsubscribe;
}

/// Decoded TrUAPI wire frame.
class ProtocolMessage {
  const ProtocolMessage(this.requestId, this.id, this.value);

  /// Correlation id shared by a request/response or subscription frame set.
  final String requestId;

  /// Wire-table numeric discriminant.
  final int id;

  /// SCALE-encoded payload body.
  final Uint8List value;
}

/// Auto-handshake wiring supplied by the generated client so the transport can
/// answer inbound `host_handshake_request` frames without importing generated
/// code itself.
class HandshakeResponder {
  const HandshakeResponder({required this.ids, required this.respond});

  /// Request/response discriminants for `System::handshake`.
  final RequestFrameIds ids;

  /// Build the response payload bytes for an inbound handshake request payload,
  /// given the transport's negotiated codec version.
  final Uint8List Function(Uint8List requestPayload, int codecVersion) respond;
}

/// Encode a wire frame: `str(requestId) ++ u8(id) ++ payload`.
Uint8List encodeWireMessage(ProtocolMessage message) {
  if (message.id < 0 || message.id > 255) {
    throw ArgumentError('Invalid wire discriminant: ${message.id}');
  }
  final out = BytesBuilder(copy: false);
  s.str.encInto(out, message.requestId);
  out.addByte(message.id);
  out.add(message.value);
  return out.toBytes();
}

/// Decode a wire frame produced by [encodeWireMessage].
ProtocolMessage decodeWireMessage(Uint8List message) {
  final input = s.Input(message);
  final requestId = s.str.decFrom(input);
  if (input.atEnd) {
    throw const FormatException('Wire frame too short: missing discriminant');
  }
  final id = input.takeByte();
  final value = input.takeBytes(message.length - input.offset);
  return ProtocolMessage(requestId, id, value);
}

/// Request/response + subscription layer over a [Provider].
abstract class Transport {
  /// Highest TrUAPI protocol version this client speaks.
  int get truapiVersion;

  /// SCALE codec version advertised during the host handshake.
  int get codecVersion;

  /// Send one request frame and resolve with the typed outcome decoded from the
  /// response payload.
  Future<Result<T, E>> request<T, E>({
    required RequestFrameIds ids,
    required Uint8List payload,
    required Result<T, E> Function(Uint8List payload) decodeResponse,
  });

  /// Start a raw subscription and route receive/interrupt/close to callbacks.
  Subscription subscribeRaw({
    required SubscriptionFrameIds ids,
    required Uint8List payload,
    required void Function(Uint8List payload) onReceive,
    void Function(Uint8List payload)? onInterrupt,
    void Function(Object error)? onClose,
  });

  /// Tear down the transport and detach provider listeners. Idempotent.
  void dispose();
}

/// Terminal error delivered through a subscription [Stream] when the host
/// interrupted it with a typed reason. Streams that complete normally (empty
/// interrupt) emit `onDone` instead.
class SubscriptionInterrupted<Reason> implements Exception {
  const SubscriptionInterrupted(this.reason);

  /// The typed interrupt payload supplied by the host.
  final Reason reason;

  @override
  String toString() => 'SubscriptionInterrupted($reason)';
}

/// Wrap [Transport.subscribeRaw] as an idiomatic Dart [Stream].
///
/// The subscription starts on first listen and stops (sending the wire stop
/// frame) when the stream subscription is cancelled. A typed interrupt
/// ([decodeInterrupt] provided) surfaces as a [SubscriptionInterrupted] error;
/// an untyped interrupt completes the stream normally.
Stream<Item> subscribeStream<Item, Reason>({
  required Transport transport,
  required SubscriptionFrameIds ids,
  required Uint8List payload,
  required Item Function(Uint8List payload) decodeItem,
  Reason Function(Uint8List payload)? decodeInterrupt,
}) {
  late StreamController<Item> controller;
  Subscription? sub;

  void start() {
    sub = transport.subscribeRaw(
      ids: ids,
      payload: payload,
      onReceive: (p) {
        if (controller.isClosed) return;
        try {
          controller.add(decodeItem(p));
        } catch (error, stack) {
          controller.addError(error, stack);
        }
      },
      onInterrupt: (p) {
        if (controller.isClosed) return;
        if (decodeInterrupt != null) {
          try {
            controller.addError(SubscriptionInterrupted<Reason>(
              decodeInterrupt(p),
            ));
          } catch (error, stack) {
            controller.addError(error, stack);
          }
        }
        controller.close();
      },
      onClose: (error) {
        if (controller.isClosed) return;
        controller.addError(error);
        controller.close();
      },
    );
  }

  controller = StreamController<Item>(
    onListen: start,
    onCancel: () => sub?.unsubscribe(),
  );
  return controller.stream;
}

/// Build a [Transport] over [provider].
///
/// Pass [handshake] (the generated client does) to auto-answer inbound
/// `host_handshake_request` frames, keeping `@parity/truapi` drop-in
/// compatibility with hosts that initiate their own handshake.
Transport createTransport(
  Provider provider, {
  required int truapiVersion,
  required int codecVersion,
  HandshakeResponder? handshake,
}) =>
    _Transport(
      provider,
      truapiVersion: truapiVersion,
      codecVersion: codecVersion,
      handshake: handshake,
    );

class _Pending {
  _Pending(this.ids, this.completer, this.complete);
  final RequestFrameIds ids;
  final Completer<Uint8List> completer;
  final void Function(Uint8List payload) complete;
}

class _Sub {
  _Sub(this.ids, this.onReceive, this.onInterrupt, this.onClose);
  final SubscriptionFrameIds ids;
  final void Function(Uint8List payload) onReceive;
  final void Function(Uint8List payload)? onInterrupt;
  final void Function(Object error)? onClose;
}

class _Transport implements Transport {
  _Transport(
    this._provider, {
    required this.truapiVersion,
    required this.codecVersion,
    required HandshakeResponder? handshake,
  }) : _handshake = handshake {
    _unsubscribeClose = _provider.subscribeClose(_closeWithError);
    _unsubscribeMessage = _provider.subscribe(_onMessage);
  }

  final Provider _provider;
  final HandshakeResponder? _handshake;

  @override
  final int truapiVersion;

  @override
  final int codecVersion;

  int _idCounter = 0;
  Object? _closedError;
  final Map<String, _Pending> _pending = {};
  final Map<String, _Sub> _subscriptions = {};
  CancelFn? _unsubscribeMessage;
  CancelFn? _unsubscribeClose;

  void _closeWithError(Object error) {
    if (_closedError != null) return;
    _closedError = error;
    for (final entry in _pending.values.toList()) {
      entry.completer.completeError(error);
    }
    _pending.clear();
    for (final sub in _subscriptions.values.toList()) {
      sub.onClose?.call(error);
    }
    _subscriptions.clear();
  }

  void _send(ProtocolMessage message) {
    final closed = _closedError;
    if (closed != null) throw closed;
    final Uint8List encoded;
    try {
      encoded = encodeWireMessage(message);
    } catch (error) {
      _closeWithError(error);
      rethrow;
    }
    try {
      _provider.postMessage(encoded);
    } catch (error) {
      _closeWithError(error);
      rethrow;
    }
  }

  void _onMessage(Uint8List message) {
    if (_closedError != null) return;
    final ProtocolMessage decoded;
    try {
      decoded = decodeWireMessage(message);
    } catch (error) {
      _closeWithError(error);
      return;
    }

    final handshake = _handshake;
    if (handshake != null && decoded.id == handshake.ids.request) {
      try {
        final response = handshake.respond(decoded.value, codecVersion);
        _send(ProtocolMessage(
          decoded.requestId,
          handshake.ids.response,
          response,
        ));
      } catch (_) {
        // Provider already closed, or a malformed handshake; ignore.
      }
      return;
    }

    final pending = _pending[decoded.requestId];
    if (pending != null) {
      if (decoded.id != pending.ids.response) return;
      _pending.remove(decoded.requestId);
      try {
        pending.complete(decoded.value);
      } catch (error) {
        if (!pending.completer.isCompleted) {
          pending.completer.completeError(error);
        }
      }
      return;
    }

    final sub = _subscriptions[decoded.requestId];
    if (sub != null) {
      if (decoded.id == sub.ids.receive) {
        try {
          sub.onReceive(decoded.value);
        } catch (error) {
          // A consumer decode/handler error must not tear down sibling
          // subscriptions on this transport. Surface via onClose and drop.
          _subscriptions.remove(decoded.requestId);
          sub.onClose?.call(error);
        }
      } else if (decoded.id == sub.ids.interrupt) {
        _subscriptions.remove(decoded.requestId);
        sub.onInterrupt?.call(decoded.value);
      }
    }
  }

  @override
  Future<Result<T, E>> request<T, E>({
    required RequestFrameIds ids,
    required Uint8List payload,
    required Result<T, E> Function(Uint8List payload) decodeResponse,
  }) {
    final closed = _closedError;
    if (closed != null) return Future.error(closed);

    final completer = Completer<Uint8List>();
    final requestId = 'p:${++_idCounter}';
    _pending[requestId] = _Pending(ids, completer, completer.complete);
    try {
      _send(ProtocolMessage(requestId, ids.request, payload));
    } catch (error) {
      _pending.remove(requestId);
      return Future.error(error);
    }
    return completer.future.then((bytes) => decodeResponse(bytes));
  }

  @override
  Subscription subscribeRaw({
    required SubscriptionFrameIds ids,
    required Uint8List payload,
    required void Function(Uint8List payload) onReceive,
    void Function(Uint8List payload)? onInterrupt,
    void Function(Object error)? onClose,
  }) {
    final closed = _closedError;
    if (closed != null) {
      onClose?.call(closed);
      return Subscription(subscriptionId: '', unsubscribe: () {});
    }

    final requestId = 'p:${++_idCounter}';
    _subscriptions[requestId] = _Sub(ids, onReceive, onInterrupt, onClose);
    try {
      _send(ProtocolMessage(requestId, ids.start, payload));
    } catch (error) {
      _subscriptions.remove(requestId);
      onClose?.call(error);
      return Subscription(subscriptionId: requestId, unsubscribe: () {});
    }

    return Subscription(
      subscriptionId: requestId,
      unsubscribe: () {
        // Skip the stop frame if the host already ended the stream via an
        // interrupt (which removed the entry).
        if (!_subscriptions.containsKey(requestId)) return;
        _subscriptions.remove(requestId);
        try {
          _send(ProtocolMessage(requestId, ids.stop, s.unit.enc(s.unitValue)));
        } catch (_) {
          // Provider already closed.
        }
      },
    );
  }

  @override
  void dispose() {
    _closeWithError(StateError('transport disposed'));
    _unsubscribeMessage?.call();
    _unsubscribeClose?.call();
    _unsubscribeMessage = null;
    _unsubscribeClose = null;
  }
}
