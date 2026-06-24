import 'dart:async';
import 'dart:convert';
import 'dart:typed_data';

import 'package:smoldot_provider/smoldot_provider.dart';
import 'package:truapi/truapi.dart';
import 'package:truapi_smoldot/truapi_smoldot.dart';
import 'package:test/test.dart';

/// A [JsonRpcProvider] that auto-answers requests from a `method -> result` map.
JsonRpcProvider scriptedProvider(Map<String, Object?> responses) =>
    (onMessage) => _ScriptedConnection(responses, onMessage);

class _ScriptedConnection implements JsonRpcConnection {
  _ScriptedConnection(this._responses, this._onMessage);
  final Map<String, Object?> _responses;
  final void Function(String message) _onMessage;
  bool _closed = false;

  @override
  void send(String message) {
    final request = jsonDecode(message) as Map<String, dynamic>;
    final method = request['method'] as String;
    final id = request['id'];
    scheduleMicrotask(() {
      if (_closed) return;
      if (_responses.containsKey(method)) {
        _onMessage(jsonEncode(
            {'jsonrpc': '2.0', 'id': id, 'result': _responses[method]}));
      } else {
        _onMessage(jsonEncode({
          'jsonrpc': '2.0',
          'id': id,
          'error': {'code': -32601, 'message': 'no script for $method'},
        }));
      }
    });
  }

  @override
  void disconnect() => _closed = true;
}

SmoldotChainHandlers handlersWith(Map<String, Object?> responses) {
  final client = JsonRpcClient(scriptedProvider(responses));
  return SmoldotChainHandlers((_) async => client);
}

const _ctx = CallContext('p:1');
final _genesis = Uint8List.fromList([1, 2, 3]);

void main() {
  test('getSpecGenesisHash maps chainSpec_v1_genesisHash', () async {
    final handlers = handlersWith({'chainSpec_v1_genesisHash': '0xdeadbeef'});
    final result = await handlers.getSpecGenesisHash(
      _ctx,
      RemoteChainSpecGenesisHashRequest(genesisHash: _genesis),
    );
    final ok = result as Ok<RemoteChainSpecGenesisHashResponse, GenericError>;
    expect(ok.value.genesisHash, hexToBytes('0xdeadbeef'));
  });

  test('getSpecChainName maps chainSpec_v1_chainName', () async {
    final handlers = handlersWith({'chainSpec_v1_chainName': 'Westend'});
    final result = await handlers.getSpecChainName(
      _ctx,
      RemoteChainSpecChainNameRequest(genesisHash: _genesis),
    );
    final ok = result as Ok<RemoteChainSpecChainNameResponse, GenericError>;
    expect(ok.value.chainName, 'Westend');
  });

  test('getSpecProperties JSON-encodes chainSpec_v1_properties', () async {
    final handlers = handlersWith({
      'chainSpec_v1_properties': {'ss58Format': 42, 'tokenDecimals': 12},
    });
    final result = await handlers.getSpecProperties(
      _ctx,
      RemoteChainSpecPropertiesRequest(genesisHash: _genesis),
    );
    final ok = result as Ok<RemoteChainSpecPropertiesResponse, GenericError>;
    expect(
      jsonDecode(ok.value.properties),
      {'ss58Format': 42, 'tokenDecimals': 12},
    );
  });

  test('a JSON-RPC error maps to Err(GenericError)', () async {
    final handlers = handlersWith(const {}); // no script → error response
    final result = await handlers.getSpecGenesisHash(
      _ctx,
      RemoteChainSpecGenesisHashRequest(genesisHash: _genesis),
    );
    expect(result.isErr, isTrue);
    expect((result as Err).error.reason, contains('no script'));
  });
}
