import 'dart:async';
import 'dart:convert';
import 'dart:typed_data';

import 'package:truapi/truapi.dart';

import 'backend.dart';
import 'hex.dart';
import 'json_rpc_client.dart';

/// Resolves a [JsonRpcClient] for a chain identified by its genesis hash.
typedef ChainClientResolver = Future<JsonRpcClient> Function(
  Uint8List genesisHash,
);

/// One live `chainHead_v1_follow` subscription, keyed by the TrUAPI
/// `followSubscriptionId` (the wire request id of the follow start frame, which
/// the product echoes back on every follow-up operation).
class _FollowSession {
  _FollowSession(this.client, this.smoldotSubscriptionId);

  /// JSON-RPC client the follow runs on; follow-up operations reuse it.
  final JsonRpcClient client;

  /// Subscription id smoldot assigned to `chainHead_v1_follow`; passed as the
  /// first argument of every `chainHead_v1_*` operation method.
  final String smoldotSubscriptionId;
}

/// Backs the TrUAPI **Chain** service with a smoldot light client, mapping the
/// typed TrUAPI requests onto Polkadot JSON-RPC over a [JsonRpcClient].
///
/// The chain-head surface is the JSON-RPC v2 `chainHead_v1_*` family: a single
/// [followHeadSubscribe] stream carries both the chain lifecycle events and the
/// results of the operations (`getHeadBody`/`getHeadStorage`/`callHead`) started
/// against it. Operations correlate to their follow via the
/// `followSubscriptionId`, which equals the [CallContext.requestId] of the
/// originating [followHeadSubscribe] call.
class SmoldotChainHandlers implements ChainHostHandlers {
  SmoldotChainHandlers(this._clientFor);

  /// Resolve clients from a [SmoldotChainBackend].
  SmoldotChainHandlers.backend(SmoldotChainBackend backend)
      : _clientFor = backend.clientFor;

  final ChainClientResolver _clientFor;

  /// Live follow subscriptions, keyed by TrUAPI `followSubscriptionId`.
  final Map<String, Future<_FollowSession>> _sessions = {};

  /// Run [body] against the chain's client, mapping any throw to
  /// `Err(GenericError)`.
  Future<Result<T, GenericError>> _guard<T>(
    Uint8List genesisHash,
    Future<T> Function(JsonRpcClient client) body,
  ) async {
    try {
      final client = await _clientFor(genesisHash);
      return Ok(await body(client));
    } catch (error) {
      return Err(GenericError(reason: error.toString()));
    }
  }

  /// Run [body] against the follow session named by [followSubscriptionId],
  /// mapping any throw (including an unknown subscription) to `Err`.
  Future<Result<T, GenericError>> _guardSession<T>(
    String followSubscriptionId,
    Future<T> Function(_FollowSession session) body,
  ) async {
    try {
      final pending = _sessions[followSubscriptionId];
      if (pending == null) {
        throw StateError(
          'no active follow subscription "$followSubscriptionId"',
        );
      }
      return Ok(await body(await pending));
    } catch (error) {
      return Err(GenericError(reason: error.toString()));
    }
  }

  // --- chainHead_v1_follow (subscription) ------------------------------------

  @override
  Stream<RemoteChainHeadFollowItem> followHeadSubscribe(
    CallContext ctx,
    RemoteChainHeadFollowRequest request,
  ) {
    final ready = Completer<_FollowSession>();
    // Keep the stored future's errors handled even if no operation awaits it.
    unawaited(ready.future.then((_) {}, onError: (_) {}));
    StreamSubscription<Object?>? inner;
    late StreamController<RemoteChainHeadFollowItem> controller;

    void forget() {
      if (identical(_sessions[ctx.requestId], ready.future)) {
        _sessions.remove(ctx.requestId);
      }
    }

    Future<void> start() async {
      try {
        final client = await _clientFor(request.genesisHash);
        final (subscriptionId, events) = await client.subscribe(
          'chainHead_v1_follow',
          [request.withRuntime],
          'chainHead_v1_unfollow',
        );
        ready.complete(_FollowSession(client, subscriptionId));
        inner = events.listen(
          (event) {
            final item = _mapFollowEvent(event);
            if (item != null && !controller.isClosed) controller.add(item);
          },
          onError: (Object error) {
            if (!controller.isClosed) controller.addError(error);
          },
          onDone: () {
            forget();
            if (!controller.isClosed) controller.close();
          },
        );
      } catch (error) {
        if (!ready.isCompleted) ready.completeError(error);
        forget();
        if (!controller.isClosed) {
          controller.addError(error);
          await controller.close();
        }
      }
    }

    controller = StreamController<RemoteChainHeadFollowItem>(
      onListen: start,
      onCancel: () async {
        forget();
        await inner?.cancel();
      },
    );
    _sessions[ctx.requestId] = ready.future;
    return controller.stream;
  }

  // --- chainHead_v1_* operations ---------------------------------------------

  @override
  Future<Result<RemoteChainHeadHeaderResponse, GenericError>> getHeadHeader(
    CallContext ctx,
    RemoteChainHeadHeaderRequest request,
  ) =>
      _guardSession(request.followSubscriptionId, (session) async {
        final header = await session.client.request(
          'chainHead_v1_header',
          [session.smoldotSubscriptionId, bytesToHex(request.hash)],
        );
        return RemoteChainHeadHeaderResponse(
          header: header == null ? null : hexToBytes(header as String),
        );
      });

  @override
  Future<Result<RemoteChainHeadBodyResponse, GenericError>> getHeadBody(
    CallContext ctx,
    RemoteChainHeadBodyRequest request,
  ) =>
      _guardSession(request.followSubscriptionId, (session) async {
        final started = await session.client.request(
          'chainHead_v1_body',
          [session.smoldotSubscriptionId, bytesToHex(request.hash)],
        );
        return RemoteChainHeadBodyResponse(
          operation: _operationStarted(started),
        );
      });

  @override
  Future<Result<RemoteChainHeadStorageResponse, GenericError>> getHeadStorage(
    CallContext ctx,
    RemoteChainHeadStorageRequest request,
  ) =>
      _guardSession(request.followSubscriptionId, (session) async {
        final items = [
          for (final item in request.items)
            {'key': bytesToHex(item.key), 'type': _queryType(item.queryType)},
        ];
        final started = await session.client.request(
          'chainHead_v1_storage',
          [
            session.smoldotSubscriptionId,
            bytesToHex(request.hash),
            items,
            if (request.childTrie != null)
              bytesToHex(request.childTrie!)
            else
              null,
          ],
        );
        return RemoteChainHeadStorageResponse(
          operation: _operationStarted(started),
        );
      });

  @override
  Future<Result<RemoteChainHeadCallResponse, GenericError>> callHead(
    CallContext ctx,
    RemoteChainHeadCallRequest request,
  ) =>
      _guardSession(request.followSubscriptionId, (session) async {
        final started = await session.client.request(
          'chainHead_v1_call',
          [
            session.smoldotSubscriptionId,
            bytesToHex(request.hash),
            request.function,
            bytesToHex(request.callParameters),
          ],
        );
        return RemoteChainHeadCallResponse(
          operation: _operationStarted(started),
        );
      });

  @override
  Future<Result<Unit, GenericError>> unpinHead(
    CallContext ctx,
    RemoteChainHeadUnpinRequest request,
  ) =>
      _guardSession(request.followSubscriptionId, (session) async {
        await session.client.request(
          'chainHead_v1_unpin',
          [
            session.smoldotSubscriptionId,
            [for (final hash in request.hashes) bytesToHex(hash)],
          ],
        );
        return unitValue;
      });

  @override
  Future<Result<Unit, GenericError>> continueHead(
    CallContext ctx,
    RemoteChainHeadContinueRequest request,
  ) =>
      _guardSession(request.followSubscriptionId, (session) async {
        await session.client.request(
          'chainHead_v1_continue',
          [session.smoldotSubscriptionId, request.operationId],
        );
        return unitValue;
      });

  @override
  Future<Result<Unit, GenericError>> stopHeadOperation(
    CallContext ctx,
    RemoteChainHeadStopOperationRequest request,
  ) =>
      _guardSession(request.followSubscriptionId, (session) async {
        await session.client.request(
          'chainHead_v1_stopOperation',
          [session.smoldotSubscriptionId, request.operationId],
        );
        return unitValue;
      });

  // --- chainSpec_v1_* --------------------------------------------------------

  @override
  Future<Result<RemoteChainSpecGenesisHashResponse, GenericError>>
      getSpecGenesisHash(
    CallContext ctx,
    RemoteChainSpecGenesisHashRequest request,
  ) =>
          _guard(request.genesisHash, (client) async {
            final hash =
                await client.request('chainSpec_v1_genesisHash') as String;
            return RemoteChainSpecGenesisHashResponse(
              genesisHash: hexToBytes(hash),
            );
          });

  @override
  Future<Result<RemoteChainSpecChainNameResponse, GenericError>>
      getSpecChainName(
    CallContext ctx,
    RemoteChainSpecChainNameRequest request,
  ) =>
          _guard(request.genesisHash, (client) async {
            final name =
                await client.request('chainSpec_v1_chainName') as String;
            return RemoteChainSpecChainNameResponse(chainName: name);
          });

  @override
  Future<Result<RemoteChainSpecPropertiesResponse, GenericError>>
      getSpecProperties(
    CallContext ctx,
    RemoteChainSpecPropertiesRequest request,
  ) =>
          _guard(request.genesisHash, (client) async {
            final properties = await client.request('chainSpec_v1_properties');
            return RemoteChainSpecPropertiesResponse(
              properties: jsonEncode(properties),
            );
          });

  // --- transaction_v1_* ------------------------------------------------------

  @override
  Future<Result<RemoteChainTransactionBroadcastResponse, GenericError>>
      broadcastTransaction(
    CallContext ctx,
    RemoteChainTransactionBroadcastRequest request,
  ) =>
          _guard(request.genesisHash, (client) async {
            final operationId = await client.request(
              'transaction_v1_broadcast',
              [bytesToHex(request.transaction)],
            );
            return RemoteChainTransactionBroadcastResponse(
              operationId: operationId as String?,
            );
          });

  @override
  Future<Result<Unit, GenericError>> stopTransaction(
    CallContext ctx,
    RemoteChainTransactionStopRequest request,
  ) =>
      _guard(request.genesisHash, (client) async {
        await client.request('transaction_v1_stop', [request.operationId]);
        return unitValue;
      });

  // --- JSON → typed event mapping --------------------------------------------

  /// Map a `chainHead_v1_follow` event payload to a [RemoteChainHeadFollowItem],
  /// or `null` for an event shape we don't model.
  RemoteChainHeadFollowItem? _mapFollowEvent(Object? raw) {
    if (raw is! Map) return null;
    switch (raw['event']) {
      case 'initialized':
        final hashes = raw['finalizedBlockHashes'] ??
            (raw['finalizedBlockHash'] != null
                ? [raw['finalizedBlockHash']]
                : const <Object?>[]);
        return RemoteChainHeadFollowItemInitialized(
          finalizedBlockHashes: _hashList(hashes),
          finalizedBlockRuntime: _runtime(raw['finalizedBlockRuntime']),
        );
      case 'newBlock':
        return RemoteChainHeadFollowItemNewBlock(
          blockHash: hexToBytes(raw['blockHash'] as String),
          parentBlockHash: hexToBytes(raw['parentBlockHash'] as String),
          newRuntime: _runtime(raw['newRuntime']),
        );
      case 'bestBlockChanged':
        return RemoteChainHeadFollowItemBestBlockChanged(
          bestBlockHash: hexToBytes(raw['bestBlockHash'] as String),
        );
      case 'finalized':
        return RemoteChainHeadFollowItemFinalized(
          finalizedBlockHashes: _hashList(raw['finalizedBlockHashes']),
          prunedBlockHashes: _hashList(raw['prunedBlockHashes']),
        );
      case 'operationBodyDone':
        return RemoteChainHeadFollowItemOperationBodyDone(
          operationId: raw['operationId'] as String,
          value: _hashList(raw['value']),
        );
      case 'operationCallDone':
        return RemoteChainHeadFollowItemOperationCallDone(
          operationId: raw['operationId'] as String,
          output: hexToBytes(raw['output'] as String),
        );
      case 'operationStorageItems':
        return RemoteChainHeadFollowItemOperationStorageItems(
          operationId: raw['operationId'] as String,
          items: _storageItems(raw['items']),
        );
      case 'operationStorageDone':
        return RemoteChainHeadFollowItemOperationStorageDone(
          operationId: raw['operationId'] as String,
        );
      case 'operationWaitingForContinue':
        return RemoteChainHeadFollowItemOperationWaitingForContinue(
          operationId: raw['operationId'] as String,
        );
      case 'operationInaccessible':
        return RemoteChainHeadFollowItemOperationInaccessible(
          operationId: raw['operationId'] as String,
        );
      case 'operationError':
        return RemoteChainHeadFollowItemOperationError(
          operationId: raw['operationId'] as String,
          error: raw['error']?.toString() ?? 'operation error',
        );
      case 'stop':
        return const RemoteChainHeadFollowItemStop();
      default:
        return null;
    }
  }

  OperationStartedResult _operationStarted(Object? raw) {
    if (raw is Map && raw['result'] == 'started') {
      return OperationStartedResultStarted(
        operationId: raw['operationId'] as String,
      );
    }
    return const OperationStartedResultLimitReached();
  }

  RuntimeType? _runtime(Object? raw) {
    if (raw is! Map) return null;
    if (raw['type'] == 'valid') {
      return RuntimeTypeValid(_runtimeSpec(raw['spec'] as Map));
    }
    return RuntimeTypeInvalid(
      error: raw['error']?.toString() ?? 'invalid runtime',
    );
  }

  RuntimeSpec _runtimeSpec(Map spec) => RuntimeSpec(
        specName: spec['specName'] as String,
        implName: spec['implName'] as String,
        specVersion: (spec['specVersion'] as num).toInt(),
        implVersion: (spec['implVersion'] as num).toInt(),
        transactionVersion: (spec['transactionVersion'] as num?)?.toInt(),
        apis: _runtimeApis(spec['apis']),
      );

  List<RuntimeApi> _runtimeApis(Object? raw) {
    if (raw is! Map) return const [];
    return [
      for (final entry in raw.entries)
        RuntimeApi(
          name: entry.key as String,
          version: (entry.value as num).toInt(),
        ),
    ];
  }

  List<StorageResultItem> _storageItems(Object? raw) {
    if (raw is! List) return const [];
    return [
      for (final item in raw.cast<Map>())
        StorageResultItem(
          key: hexToBytes(item['key'] as String),
          value: _optionalHex(item['value']),
          hash: _optionalHex(item['hash']),
          closestDescendantMerkleValue:
              _optionalHex(item['closestDescendantMerkleValue']),
        ),
    ];
  }

  List<Uint8List> _hashList(Object? raw) {
    if (raw is! List) return const [];
    return [for (final hash in raw) hexToBytes(hash as String)];
  }

  Uint8List? _optionalHex(Object? raw) =>
      raw == null ? null : hexToBytes(raw as String);

  String _queryType(StorageQueryType type) => switch (type) {
        StorageQueryType.value => 'value',
        StorageQueryType.hash => 'hash',
        StorageQueryType.closestDescendantMerkleValue =>
          'closestDescendantMerkleValue',
        StorageQueryType.descendantsValues => 'descendantsValues',
        StorageQueryType.descendantsHashes => 'descendantsHashes',
      };
}
