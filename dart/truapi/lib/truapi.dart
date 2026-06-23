/// TrUAPI Dart client.
///
/// The typed, SCALE-encoded protocol client that lets Flutter/Dart products
/// talk to their Polkadot host. The protocol surface under [generated] is
/// produced by `truapi-codegen` from the Rust trait definitions, so the Rust
/// crates remain the single source of truth.
///
/// ```dart
/// final channel = LoopbackChannel();
/// final client = createClient(channel.client);
/// final result = await client.account.accountGet(
///   AccountGetRequest(/* ... */),
/// );
/// ```
library;

export 'src/result.dart';
export 'src/scale.dart' show Codec, Input;
export 'src/transport.dart'
    show
        CancelFn,
        HandshakeResponder,
        Provider,
        ProtocolMessage,
        RequestFrameIds,
        Subscription,
        SubscriptionFrameIds,
        Transport,
        createTransport,
        decodeWireMessage,
        encodeWireMessage;
export 'src/providers/loopback_provider.dart';

// The generated client facade (`createClient`, service classes, types, and
// wire table), produced by `truapi-codegen --dart-output`.
export 'src/generated/index.dart';
