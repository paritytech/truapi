/// Shared wire-protocol primitives for the TrUAPI host: the [Provider] byte
/// pipe, frame encode/decode, and the wire-discriminant types.
///
/// The frame format is:
///
/// ```text
/// [ requestId : SCALE str ][ discriminant : u8 ][ payload : SCALE bytes ]
/// ```
///
/// (Product/client transport lives in the TS/JS `@parity/truapi` package; this
/// Dart package is host-only.)
library;

import 'dart:typed_data';

import 'scale.dart' as s;

/// Cancels a previously registered listener.
typedef CancelFn = void Function();

/// Raw inbound frame handler.
typedef MessageHandler = void Function(Uint8List message);

/// Provider-level close/failure handler.
typedef CloseHandler = void Function(Object error);

/// Raw message pipe the host server rides on. Concrete providers (loopback for
/// tests, a native bridge for the host) implement this and nothing more.
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

  /// Inbound request frame discriminant.
  final int request;

  /// Outbound response frame discriminant.
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

  /// Inbound start frame discriminant.
  final int start;

  /// Inbound stop frame discriminant.
  final int stop;

  /// Outbound interrupt (stream-end) frame discriminant.
  final int interrupt;

  /// Outbound item frame discriminant.
  final int receive;
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

/// Signal raised by a subscription handler to end a (fallible) subscription
/// with a typed interrupt reason. Add it as a stream error from a host
/// subscription handler; the dispatcher encodes [reason] into the interrupt
/// frame. A normal stream completion ends the subscription without a reason.
class SubscriptionInterrupted<Reason> implements Exception {
  const SubscriptionInterrupted(this.reason);

  /// The typed interrupt payload.
  final Reason reason;

  @override
  String toString() => 'SubscriptionInterrupted($reason)';
}
