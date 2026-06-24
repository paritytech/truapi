@Tags(['network'])
@Timeout(Duration(minutes: 3))
library;

import 'dart:async';
import 'dart:io';

import 'package:truapi/truapi.dart';
import 'package:truapi_smoldot/truapi_smoldot.dart';
import 'package:test/test.dart';

/// Westend's well-known genesis hash (the registry key for its [ChainSource]).
final _westendGenesis = hexToBytes(
  '0xe143f23803ac50e8f6f8e62695d1ce9e4e1d68aa36c1cd2cfd15340213f3423e',
);

const _specPath =
    '../../../polkadart-snowpinelabs/packages/smoldot/test/fixtures/westend.json';

void main() {
  group('SmoldotChainHandlers against live Westend', () {
    late SmoldotChainBackend backend;
    late SmoldotChainHandlers chain;

    setUpAll(() async {
      final spec = await File(_specPath).readAsString();
      backend = await SmoldotChainBackend.create(
        chains: {bytesToHex(_westendGenesis): ChainSource(chainSpec: spec)},
      );
      chain = SmoldotChainHandlers.backend(backend);
    });

    tearDownAll(() => backend.dispose());

    test('getSpecChainName returns Westend', () async {
      final result = await chain.getSpecChainName(
        const CallContext('1'),
        RemoteChainSpecChainNameRequest(genesisHash: _westendGenesis),
      );
      final ok = result as Ok<RemoteChainSpecChainNameResponse, GenericError>;
      expect(ok.value.chainName, 'Westend');
    });

    test('getSpecGenesisHash matches the known genesis', () async {
      final result = await chain.getSpecGenesisHash(
        const CallContext('2'),
        RemoteChainSpecGenesisHashRequest(genesisHash: _westendGenesis),
      );
      final ok = result as Ok<RemoteChainSpecGenesisHashResponse, GenericError>;
      expect(ok.value.genesisHash, _westendGenesis);
    });

    test('follow → getHeadHeader returns the finalized block header', () async {
      const followId = 'follow-1';
      final initialized = Completer<RemoteChainHeadFollowItemInitialized>();
      final sub = chain
          .followHeadSubscribe(
        const CallContext(followId),
        RemoteChainHeadFollowRequest(
          genesisHash: _westendGenesis,
          withRuntime: false,
        ),
      )
          .listen((event) {
        if (event is RemoteChainHeadFollowItemInitialized &&
            !initialized.isCompleted) {
          initialized.complete(event);
        }
      });

      final init = await initialized.future;
      expect(init.finalizedBlockHashes, isNotEmpty);

      // The follow is still live, so the operation can reference its id.
      final header = await chain.getHeadHeader(
        const CallContext('h-1'),
        RemoteChainHeadHeaderRequest(
          genesisHash: _westendGenesis,
          followSubscriptionId: followId,
          hash: init.finalizedBlockHashes.first,
        ),
      );
      final ok = header as Ok<RemoteChainHeadHeaderResponse, GenericError>;
      expect(ok.value.header, isNotNull);

      await sub.cancel();
    });
  });
}
