# truapi (Dart)

The Dart/Flutter client for the **TrUAPI** protocol — the typed, SCALE-encoded
API surface a Polkadot host exposes to the products running inside it.

The client is **generated from the Rust trait definitions** in this repo
(`rust/crates/truapi`) by `truapi-codegen`, so the Rust crate stays the single
source of truth. This package contributes the hand-written runtime (SCALE codec,
transport, providers) and the generated surface under `lib/src/generated/`.

> Prototype / reference implementation. Not audited. Use at your own risk.

## Layout

This package ships **both sides** of the protocol:

- `package:truapi/truapi.dart` — the **client** (product-side): call host methods.
- `package:truapi/host.dart` — the **host** dispatcher: implement the methods a
  host (e.g. a Flutter desktop host) exposes to the products inside it.

```
lib/
  truapi.dart                 # client barrel: runtime + generated client
  host.dart                   # host barrel: runtime + generated dispatcher
  src/
    scale.dart                # SCALE codec primitives & combinators
    result.dart               # sealed Result<Ok, Err>
    transport.dart            # Provider, frame codec, createTransport, subscribeStream
    providers/
      loopback_provider.dart  # in-memory channel for tests/local harnesses
    host/
      host_server.dart        # host dispatcher runtime (createHostServer, CallContext, …)
    generated/                # git-ignored — produced by truapi-codegen
      types.dart              #   data classes, enums, sealed unions + codecs
      wire_table.dart         #   per-method frame-id constants
      client.dart             #   service clients, TruapiClient facade, createClient
      host.dart               #   per-service handler interfaces, build*Entries, createTruapiServer
      index.dart
test/
  scale_test.dart             # SCALE vectors + round-trips
  transport_test.dart         # request/response, subscriptions, handshake
  wire_vectors_test.dart      # cross-language parity vs Rust golden vectors
  generated_client_test.dart  # end-to-end generated client over loopback
  host_server_test.dart       # generated client <-> generated host over loopback
```

## Usage

```dart
import 'package:truapi/truapi.dart';

// `provider` is your transport to the host. For a native Flutter product this
// is the native-bridge provider agreed with the host; tests use LoopbackChannel.
final client = createClient(provider);

// One-shot request → Future<Result<Ok, Err>>:
final result = await client.account.getAccount(
  const HostAccountGetRequest(
    productAccountId: ProductAccountId(
      dotNsIdentifier: 'my-app.dot',
      derivationIndex: 0,
    ),
  ),
);
switch (result) {
  case Ok(value: final response):
    print('account: ${response.account.publicKey}');
  case Err(error: final e):
    print('failed: $e');
}

// Subscription → Stream<Item>:
final sub = client.theme.subscribe().listen((item) {
  // handle each themed update
});
// `sub.cancel()` sends the wire stop frame.
```

`Result<Ok, Err>` is a sealed type — pattern-match with `switch`, or use
`result.isOk` / `result.okOrNull` / `result.match(...)`. A subscription's typed
interrupt surfaces as a `SubscriptionInterrupted<Reason>` stream error; an
untyped interrupt completes the stream normally.

## Host

A host implements one typed handler group per service and wires them to a
`Provider`. Handlers receive and return the same inner protocol types the client
uses; the generated dispatch entries handle versioned wire wrapping, SCALE
encode/decode, and the request/subscription frame lifecycle.

```dart
import 'package:truapi/host.dart';

class MyAccountHandlers implements AccountHostHandlers {
  @override
  Future<Result<HostAccountGetResponse, HostAccountGetError>> getAccount(
    CallContext ctx,
    HostAccountGetRequest request,
  ) async {
    final account = myStore.lookup(request.productAccountId);
    return Ok(HostAccountGetResponse(account: account));
  }

  @override
  Stream<HostAccountConnectionStatusSubscribeItem> connectionStatusSubscribe(
    CallContext ctx,
  ) =>
      myConnectionStatusStream; // each event is forwarded as a receive frame
  // … the remaining AccountHostHandlers methods
}

// Implement TruapiHostHandlers (one getter per service) and start the server:
final server = createTruapiServer(provider, MyHostHandlers());
// … later: server.dispose();
```

A subscription handler returns a `Stream<Item>`. For a fallible
(`Result<Subscription>`) method, end the stream with a typed interrupt by adding
a `SubscriptionInterrupted<Reason>(reason)` error; otherwise the stream's normal
completion ends the subscription. To compose a server from a subset of services
(or add custom entries), use the public per-service `build<Service>Entries(...)`
builders with `createHostServer(provider, [...entries])`.

### Scaffold

[`example/host_scaffold.dart`](example/host_scaffold.dart) is a ready-to-edit
`ScaffoldHostHandlers` implementing **every** service method with
`throw UnimplementedError(...)`, plus heuristic notes on which services are
likely backed by a light client (smoldart), the wallet, or host-local state.
Copy it into your host app and fill in each method. Regenerate it any time the
trait surface changes:

```bash
make dart-scaffold     # overwrites example/host_scaffold.dart
```

## Type mapping

| Rust | Dart |
|---|---|
| `bool`, `u8`/`u16`/`u32`, `i8`/`i16`/`i32` | `bool`, `int` |
| `u64`/`u128`/`i64`/`i128`, `Compact<_>` | `BigInt` |
| `String` | `String` |
| `Vec<u8>`, `[u8; N]` | `Uint8List` |
| `Vec<T>` | `List<T>` |
| `Option<T>` | `T?` |
| `OptionBool` | `bool?` |
| tuple `(A, B)` | record `(A, B)` |
| struct | immutable class |
| unit-only enum | Dart `enum` |
| enum with payloads | `sealed class` + variant classes |
| `Result<Ok, Err>` | `Result<Ok, Err>` (`Ok` / `Err`) |
| `Subscription<T>` | `Stream<T>` |

## Transport

Everything above the `Provider` interface is transport-agnostic; only the
provider differs per host. This package ships `LoopbackChannel` (in-memory, for
tests). For a native Flutter product, implement `Provider` over the channel the
host exposes (e.g. a Flutter `BasicMessageChannel<ByteData>`, a local
socket/WebSocket, or stdio) — see the design doc's §6.1.

## Regenerate

From the repo root (regenerates TS **and** Dart, then refreshes golden vectors):

```bash
make dart          # codegen + golden vectors + analyze + test
# or just the codegen step:
./scripts/codegen.sh
```

## Develop

```bash
dart pub get
dart analyze
dart test
dart format .
```
