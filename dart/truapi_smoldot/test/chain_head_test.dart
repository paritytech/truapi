import 'dart:async';

import 'package:truapi/truapi.dart';
import 'package:truapi_smoldot/truapi_smoldot.dart';
import 'package:test/test.dart';

import 'support/fake_chain.dart';

const _genesisHex = '0x0102';
final _genesis = hexToBytes(_genesisHex);

void main() {
  group('followHeadSubscribe', () {
    test('maps follow events into typed items', () async {
      final chain = FakeChain()
        ..responders['chainHead_v1_follow'] = (_) => 'sub-1';
      final handlers = SmoldotChainHandlers(
        (_) async => JsonRpcClient(chain.provider),
      );

      final items = <RemoteChainHeadFollowItem>[];
      final sub = handlers
          .followHeadSubscribe(
            const CallContext('req-1'),
            RemoteChainHeadFollowRequest(
              genesisHash: _genesis,
              withRuntime: false,
            ),
          )
          .listen(items.add);

      await pumpEventQueue();
      chain.notify('sub-1', {
        'event': 'initialized',
        'finalizedBlockHashes': ['0xaa'],
      });
      chain.notify('sub-1', {
        'event': 'newBlock',
        'blockHash': '0xbb',
        'parentBlockHash': '0xaa',
      });
      chain.notify(
          'sub-1', {'event': 'bestBlockChanged', 'bestBlockHash': '0xbb'});
      chain.notify('sub-1', {
        'event': 'finalized',
        'finalizedBlockHashes': ['0xbb'],
        'prunedBlockHashes': <String>[],
      });
      chain.notify('sub-1', {
        'event': 'operationCallDone',
        'operationId': 'op-1',
        'output': '0xc0de',
      });
      chain.notify('sub-1', {'event': 'stop'});
      await pumpEventQueue();
      await sub.cancel();

      expect(chain.paramsFor('chainHead_v1_follow'), [false]);
      expect(items, hasLength(6));
      expect(
        (items[0] as RemoteChainHeadFollowItemInitialized).finalizedBlockHashes,
        [hexToBytes('0xaa')],
      );
      final newBlock = items[1] as RemoteChainHeadFollowItemNewBlock;
      expect(newBlock.blockHash, hexToBytes('0xbb'));
      expect(newBlock.parentBlockHash, hexToBytes('0xaa'));
      expect(
        (items[2] as RemoteChainHeadFollowItemBestBlockChanged).bestBlockHash,
        hexToBytes('0xbb'),
      );
      expect(
        (items[3] as RemoteChainHeadFollowItemFinalized).finalizedBlockHashes,
        [hexToBytes('0xbb')],
      );
      final callDone = items[4] as RemoteChainHeadFollowItemOperationCallDone;
      expect(callDone.operationId, 'op-1');
      expect(callDone.output, hexToBytes('0xc0de'));
      expect(items[5], isA<RemoteChainHeadFollowItemStop>());
    });

    test('decodes a withRuntime "valid" runtime spec', () async {
      final chain = FakeChain()
        ..responders['chainHead_v1_follow'] = (_) => 'sub-1';
      final handlers = SmoldotChainHandlers(
        (_) async => JsonRpcClient(chain.provider),
      );

      RemoteChainHeadFollowItem? first;
      final sub = handlers
          .followHeadSubscribe(
            const CallContext('req-1'),
            RemoteChainHeadFollowRequest(
              genesisHash: _genesis,
              withRuntime: true,
            ),
          )
          .listen((item) => first ??= item);

      await pumpEventQueue();
      chain.notify('sub-1', {
        'event': 'initialized',
        'finalizedBlockHashes': ['0xaa'],
        'finalizedBlockRuntime': {
          'type': 'valid',
          'spec': {
            'specName': 'westend',
            'implName': 'parity-westend',
            'specVersion': 1014000,
            'implVersion': 0,
            'transactionVersion': 26,
            'apis': {'0xdf6acb689907609b': 5},
          },
        },
      });
      await pumpEventQueue();
      await sub.cancel();

      final runtime = (first as RemoteChainHeadFollowItemInitialized)
          .finalizedBlockRuntime as RuntimeTypeValid;
      expect(runtime.value.specName, 'westend');
      expect(runtime.value.specVersion, 1014000);
      expect(runtime.value.transactionVersion, 26);
      expect(runtime.value.apis.single.name, '0xdf6acb689907609b');
      expect(runtime.value.apis.single.version, 5);
    });
  });

  group('chainHead operations', () {
    Future<(FakeChain, SmoldotChainHandlers, StreamSubscription<void>)>
        startFollow() async {
      final chain = FakeChain()
        ..responders['chainHead_v1_follow'] = (_) => 'sub-1';
      final handlers = SmoldotChainHandlers(
        (_) async => JsonRpcClient(chain.provider),
      );
      final sub = handlers
          .followHeadSubscribe(
            const CallContext('req-1'),
            RemoteChainHeadFollowRequest(
              genesisHash: _genesis,
              withRuntime: false,
            ),
          )
          .listen((_) {});
      await pumpEventQueue();
      return (chain, handlers, sub);
    }

    test('getHeadHeader passes the follow sub id and returns the header',
        () async {
      final (chain, handlers, sub) = await startFollow();
      chain.responders['chainHead_v1_header'] = (_) => '0xdead';

      final result = await handlers.getHeadHeader(
        const CallContext('req-2'),
        RemoteChainHeadHeaderRequest(
          genesisHash: _genesis,
          followSubscriptionId: 'req-1',
          hash: hexToBytes('0xbb'),
        ),
      );

      final ok = result as Ok<RemoteChainHeadHeaderResponse, GenericError>;
      expect(ok.value.header, hexToBytes('0xdead'));
      expect(chain.paramsFor('chainHead_v1_header'), ['sub-1', '0xbb']);
      await sub.cancel();
    });

    test('getHeadBody returns a started operation', () async {
      final (chain, handlers, sub) = await startFollow();
      chain.responders['chainHead_v1_body'] =
          (_) => {'result': 'started', 'operationId': 'op-7'};

      final result = await handlers.getHeadBody(
        const CallContext('req-2'),
        RemoteChainHeadBodyRequest(
          genesisHash: _genesis,
          followSubscriptionId: 'req-1',
          hash: hexToBytes('0xbb'),
        ),
      );

      final ok = result as Ok<RemoteChainHeadBodyResponse, GenericError>;
      final started = ok.value.operation as OperationStartedResultStarted;
      expect(started.operationId, 'op-7');
      await sub.cancel();
    });

    test('getHeadStorage encodes query items and a child trie', () async {
      final (chain, handlers, sub) = await startFollow();
      chain.responders['chainHead_v1_storage'] =
          (_) => {'result': 'started', 'operationId': 'op-8'};

      await handlers.getHeadStorage(
        const CallContext('req-2'),
        RemoteChainHeadStorageRequest(
          genesisHash: _genesis,
          followSubscriptionId: 'req-1',
          hash: hexToBytes('0xbb'),
          items: [
            StorageQueryItem(
              key: hexToBytes('0x26aa'),
              queryType: StorageQueryType.value,
            ),
            StorageQueryItem(
              key: hexToBytes('0x3a63'),
              queryType: StorageQueryType.descendantsHashes,
            ),
          ],
          childTrie: hexToBytes('0x99'),
        ),
      );

      expect(chain.paramsFor('chainHead_v1_storage'), [
        'sub-1',
        '0xbb',
        [
          {'key': '0x26aa', 'type': 'value'},
          {'key': '0x3a63', 'type': 'descendantsHashes'},
        ],
        '0x99',
      ]);
      await sub.cancel();
    });

    test('unpinHead / continueHead / stopHeadOperation return unit', () async {
      final (chain, handlers, sub) = await startFollow();
      chain.responders['chainHead_v1_unpin'] = (_) => null;
      chain.responders['chainHead_v1_continue'] = (_) => null;
      chain.responders['chainHead_v1_stopOperation'] = (_) => null;

      final unpin = await handlers.unpinHead(
        const CallContext('req-2'),
        RemoteChainHeadUnpinRequest(
          genesisHash: _genesis,
          followSubscriptionId: 'req-1',
          hashes: [hexToBytes('0xbb'), hexToBytes('0xcc')],
        ),
      );
      expect(unpin.isOk, isTrue);
      expect(chain.paramsFor('chainHead_v1_unpin'), [
        'sub-1',
        ['0xbb', '0xcc'],
      ]);

      final cont = await handlers.continueHead(
        const CallContext('req-2'),
        RemoteChainHeadContinueRequest(
          genesisHash: _genesis,
          followSubscriptionId: 'req-1',
          operationId: 'op-1',
        ),
      );
      expect(cont.isOk, isTrue);
      expect(chain.paramsFor('chainHead_v1_continue'), ['sub-1', 'op-1']);

      final stop = await handlers.stopHeadOperation(
        const CallContext('req-2'),
        RemoteChainHeadStopOperationRequest(
          genesisHash: _genesis,
          followSubscriptionId: 'req-1',
          operationId: 'op-1',
        ),
      );
      expect(stop.isOk, isTrue);
      await sub.cancel();
    });

    test('an operation against an unknown follow id is an Err', () async {
      final chain = FakeChain();
      final handlers = SmoldotChainHandlers(
        (_) async => JsonRpcClient(chain.provider),
      );

      final result = await handlers.getHeadHeader(
        const CallContext('req-2'),
        RemoteChainHeadHeaderRequest(
          genesisHash: _genesis,
          followSubscriptionId: 'missing',
          hash: hexToBytes('0xbb'),
        ),
      );

      expect(result.isErr, isTrue);
      expect((result as Err).error.reason, contains('no active follow'));
    });
  });

  group('transaction', () {
    test('broadcastTransaction returns the operation id', () async {
      final chain = FakeChain()
        ..responders['transaction_v1_broadcast'] = (_) => 'tx-op';
      final handlers = SmoldotChainHandlers(
        (_) async => JsonRpcClient(chain.provider),
      );

      final result = await handlers.broadcastTransaction(
        const CallContext('req-1'),
        RemoteChainTransactionBroadcastRequest(
          genesisHash: _genesis,
          transaction: hexToBytes('0xabcd'),
        ),
      );

      final ok =
          result as Ok<RemoteChainTransactionBroadcastResponse, GenericError>;
      expect(ok.value.operationId, 'tx-op');
      expect(chain.paramsFor('transaction_v1_broadcast'), ['0xabcd']);
    });

    test('stopTransaction returns unit', () async {
      final chain = FakeChain()
        ..responders['transaction_v1_stop'] = (_) => null;
      final handlers = SmoldotChainHandlers(
        (_) async => JsonRpcClient(chain.provider),
      );

      final result = await handlers.stopTransaction(
        const CallContext('req-1'),
        RemoteChainTransactionStopRequest(
          genesisHash: _genesis,
          operationId: 'tx-op',
        ),
      );

      expect(result.isOk, isTrue);
      expect(chain.paramsFor('transaction_v1_stop'), ['tx-op']);
    });
  });
}
