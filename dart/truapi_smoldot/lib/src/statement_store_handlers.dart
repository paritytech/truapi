import 'dart:async';
import 'dart:typed_data';

import 'package:truapi/truapi.dart';

import 'backend.dart';
import 'hex.dart';
import 'json_rpc_client.dart';
import 'statement_codec.dart';

/// Resolves the [JsonRpcClient] for the statement-store-enabled chain.
typedef StatementClientResolver = Future<JsonRpcClient> Function();

/// Backs the TrUAPI **StatementStore** service with a smoldot light client.
///
/// `submit` and `subscribe` map onto smoldot's `statement_submit` /
/// `statement_subscribeStatement`; the typed [SignedStatement] is transcoded to
/// and from Substrate statement bytes (see `statement_codec.dart`). Topic
/// filtering ([RemoteStatementStoreSubscribeRequest]) is applied host-side over
/// the decoded statements.
///
/// StatementStore requests carry no genesis hash, so a handler is bound to a
/// single chain (the one added with `statementStore` enabled).
///
/// `createProof` / `createProofAuthorized` are **not** supported: statement
/// proofs are produced by the wallet, not a light client. Both return
/// `Err(RemoteStatementStoreCreateProofErrorUnknown)`.
class SmoldotStatementStoreHandlers implements StatementStoreHostHandlers {
  SmoldotStatementStoreHandlers(this._client);

  /// Resolve the statement-store chain client from a [SmoldotChainBackend] by
  /// its genesis hash. The chain must have been registered with a
  /// [ChainSource] whose `enableStatementStore` is `true`.
  SmoldotStatementStoreHandlers.backend(
    SmoldotChainBackend backend,
    Uint8List genesisHash,
  ) : _client = (() => backend.clientFor(genesisHash));

  final StatementClientResolver _client;

  static const _signingUnsupported =
      'statement proofs are created by the wallet, not the smoldot light client';

  @override
  Future<Result<Unit, GenericError>> submit(
    CallContext ctx,
    SignedStatement request,
  ) async {
    try {
      final client = await _client();
      final encoded = encodeStatement(request);
      await client.request('statement_submit', [bytesToHex(encoded)]);
      return Ok(unitValue);
    } catch (error) {
      return Err(GenericError(reason: error.toString()));
    }
  }

  @override
  Stream<RemoteStatementStoreSubscribeItem> subscribe(
    CallContext ctx,
    RemoteStatementStoreSubscribeRequest request,
  ) {
    final matches = _matcher(request);
    StreamSubscription<Object?>? inner;
    late StreamController<RemoteStatementStoreSubscribeItem> controller;

    Future<void> start() async {
      try {
        final client = await _client();
        final (_, events) = await client.subscribe(
          'statement_subscribeStatement',
          const [],
          'statement_unsubscribeStatement',
        );
        inner = events.listen(
          (event) {
            final statement = _tryDecode(event);
            if (statement == null || !matches(statement.topics)) return;
            if (!controller.isClosed) {
              controller.add(
                RemoteStatementStoreSubscribeItem(
                  statements: [statement],
                  isComplete: true,
                ),
              );
            }
          },
          onError: (Object error) {
            if (!controller.isClosed) controller.addError(error);
          },
          onDone: () {
            if (!controller.isClosed) controller.close();
          },
        );
      } catch (error) {
        if (!controller.isClosed) {
          controller.addError(error);
          await controller.close();
        }
      }
    }

    controller = StreamController<RemoteStatementStoreSubscribeItem>(
      onListen: start,
      onCancel: () async => inner?.cancel(),
    );
    return controller.stream;
  }

  @override
  Future<
      Result<RemoteStatementStoreCreateProofResponse,
          RemoteStatementStoreCreateProofError>> createProof(
    CallContext ctx,
    RemoteStatementStoreCreateProofRequest request,
  ) async =>
      Err<RemoteStatementStoreCreateProofResponse,
          RemoteStatementStoreCreateProofError>(
        const RemoteStatementStoreCreateProofErrorUnknown(
          reason: _signingUnsupported,
        ),
      );

  @override
  Future<
      Result<RemoteStatementStoreCreateProofResponse,
          RemoteStatementStoreCreateProofError>> createProofAuthorized(
    CallContext ctx,
    Statement request,
  ) async =>
      Err<RemoteStatementStoreCreateProofResponse,
          RemoteStatementStoreCreateProofError>(
        const RemoteStatementStoreCreateProofErrorUnknown(
          reason: _signingUnsupported,
        ),
      );

  /// Build the topic predicate for [request] (AND for MatchAll, OR for
  /// MatchAny). An empty filter matches every statement.
  bool Function(List<Uint8List> topics) _matcher(
    RemoteStatementStoreSubscribeRequest request,
  ) {
    final (wanted, all) = switch (request) {
      RemoteStatementStoreSubscribeRequestMatchAll(:final value) => (
          value.map(bytesToHex).toSet(),
          true,
        ),
      RemoteStatementStoreSubscribeRequestMatchAny(:final value) => (
          value.map(bytesToHex).toSet(),
          false,
        ),
    };
    return (topics) {
      if (wanted.isEmpty) return true;
      final have = topics.map(bytesToHex).toSet();
      return all ? wanted.every(have.contains) : wanted.any(have.contains);
    };
  }

  SignedStatement? _tryDecode(Object? event) {
    try {
      final hex = _statementHex(event);
      return hex == null ? null : decodeStatement(hexToBytes(hex));
    } catch (_) {
      return null;
    }
  }

  /// A `statement_subscribeStatement` notification is a hex statement, or an
  /// object wrapping one.
  String? _statementHex(Object? event) {
    if (event is String) return event;
    if (event is Map) {
      final value =
          event['statement'] ?? event['encoded'] ?? event['encodedStatement'];
      if (value is String) return value;
    }
    return null;
  }
}
