import 'dart:typed_data';

import 'package:smoldot_provider/smoldot_provider.dart';

import 'hex.dart';
import 'json_rpc_client.dart';

/// A chain the backend can serve, keyed (in [SmoldotChainBackend]) by its
/// genesis hash.
class ChainSource {
  const ChainSource({
    required this.chainSpec,
    this.relayChainSpec,
    this.enableStatementStore = false,
  });

  /// Chain specification JSON for this chain.
  final String chainSpec;

  /// Relay chain spec, for a parachain. Added first and passed as a potential
  /// relay chain when adding [chainSpec].
  final String? relayChainSpec;

  /// Enable the statement-store protocol on this chain (needed by the
  /// StatementStore service).
  final bool enableStatementStore;
}

/// Owns a smoldot light client and lazily adds one chain per requested genesis
/// hash, exposing a [JsonRpcClient] per chain.
///
/// TrUAPI Chain/StatementStore requests carry a `genesisHash`; the handlers
/// resolve a client with [clientFor]. App-supplied [ChainSource]s provide the
/// chain specs (smoldot needs a spec to add a chain).
class SmoldotChainBackend {
  SmoldotChainBackend._(this._client, this._sources);

  /// Initialize a smoldot client and register the chain sources, keyed by their
  /// genesis hash (lower-case `0x`-prefixed hex).
  static Future<SmoldotChainBackend> create({
    required Map<String, ChainSource> chains,
    SmoldotConfig? config,
  }) async {
    final client = SmoldotClient(config: config);
    await client.initialize();
    return SmoldotChainBackend._(client, Map.of(chains));
  }

  final SmoldotClient _client;
  final Map<String, ChainSource> _sources;
  final Map<String, Future<JsonRpcClient>> _clients = {};

  /// Resolve (adding the chain on first use) a [JsonRpcClient] for [genesisHash].
  ///
  /// Throws [StateError] if no [ChainSource] was registered for that genesis
  /// hash. Concurrent calls for the same chain share one add.
  Future<JsonRpcClient> clientFor(Uint8List genesisHash) {
    final key = bytesToHex(genesisHash);
    return _clients[key] ??= _addChain(key);
  }

  Future<JsonRpcClient> _addChain(String genesisHashHex) async {
    final source = _sources[genesisHashHex];
    if (source == null) {
      throw StateError(
        'no chain spec registered for genesis hash $genesisHashHex',
      );
    }

    List<int>? potentialRelayChains;
    if (source.relayChainSpec != null) {
      final relay = await _client
          .addChain(AddChainConfig(chainSpec: source.relayChainSpec!));
      potentialRelayChains = [relay.chainId];
    }

    final chain = await _client.addChain(
      AddChainConfig(
        chainSpec: source.chainSpec,
        potentialRelayChains: potentialRelayChains,
        statementStore:
            source.enableStatementStore ? const StatementStoreConfig() : null,
      ),
    );
    return JsonRpcClient(getSmProvider(chain));
  }

  /// Close every chain client and dispose the smoldot client.
  Future<void> dispose() async {
    for (final pending in _clients.values) {
      try {
        await (await pending).close();
      } catch (_) {
        // ignore teardown failures
      }
    }
    _clients.clear();
    await _client.dispose();
  }
}
