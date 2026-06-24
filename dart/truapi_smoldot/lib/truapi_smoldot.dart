/// Backs the TrUAPI Dart host's chain-facing services with a smoldot light
/// client.
///
/// Maps the typed TrUAPI **Chain** and **StatementStore** SCALE protocols onto
/// Polkadot JSON-RPC over a [JsonRpcClient], which runs on a `smoldot_provider`
/// `JsonRpcProvider`. Plug [SmoldotChainHandlers] / [SmoldotStatementStoreHandlers]
/// into your generated `TruapiHostHandlers`:
///
/// ```dart
/// final backend = await SmoldotChainBackend.create(chains: {
///   '0x<genesis-hash-hex>': ChainSource(chainSpec: westendSpec),
/// });
/// final chain = SmoldotChainHandlers.backend(backend);
/// // ... wire `chain` into your TruapiHostHandlers.chain getter.
/// ```
library;

export 'src/backend.dart' show ChainSource, SmoldotChainBackend;
export 'src/chain_handlers.dart' show ChainClientResolver, SmoldotChainHandlers;
export 'src/hex.dart' show bytesToHex, hexToBytes;
export 'src/json_rpc_client.dart' show JsonRpcClient, JsonRpcException;
export 'src/statement_codec.dart'
    show decodeStatement, encodeStatement, maxStatementTopics;
export 'src/statement_store_handlers.dart'
    show SmoldotStatementStoreHandlers, StatementClientResolver;
// Re-export the smoldot configuration types a consumer needs.
export 'package:smoldot_provider/smoldot_provider.dart'
    show SmoldotConfig, StatementStoreConfig;
