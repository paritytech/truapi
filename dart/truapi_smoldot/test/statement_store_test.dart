import 'dart:typed_data';

import 'package:truapi/truapi.dart';
import 'package:truapi_smoldot/truapi_smoldot.dart';
import 'package:test/test.dart';

import 'support/fake_chain.dart';

Uint8List _filled(int length, int value) =>
    Uint8List.fromList(List.filled(length, value));

SignedStatement _statement({List<Uint8List> topics = const []}) =>
    SignedStatement(
      proof: StatementProofSr25519(
        signature: _filled(64, 0x01),
        signer: _filled(32, 0x02),
      ),
      topics: topics,
    );

const _ctx = CallContext('req-1');

void main() {
  group('submit', () {
    test('encodes the statement and calls statement_submit', () async {
      final chain = FakeChain()..responders['statement_submit'] = (_) => null;
      final handlers = SmoldotStatementStoreHandlers(
        () async => JsonRpcClient(chain.provider),
      );

      final statement = _statement(topics: [_filled(32, 0xaa)]);
      final result = await handlers.submit(_ctx, statement);

      expect(result.isOk, isTrue);
      final sentHex = chain.paramsFor('statement_submit').single as String;
      // Compare via canonical bytes (SignedStatement.== is identity-based).
      expect(hexToBytes(sentHex), encodeStatement(statement));
    });

    test('a rejected submit maps to Err(GenericError)', () async {
      final chain = FakeChain(); // no responder → JSON-RPC error
      final handlers = SmoldotStatementStoreHandlers(
        () async => JsonRpcClient(chain.provider),
      );

      final result = await handlers.submit(_ctx, _statement());
      expect(result.isErr, isTrue);
    });
  });

  group('subscribe', () {
    test('decodes notifications and applies a MatchAll topic filter', () async {
      final chain = FakeChain()
        ..responders['statement_subscribeStatement'] = (_) => 'stmt-sub';
      final handlers = SmoldotStatementStoreHandlers(
        () async => JsonRpcClient(chain.provider),
      );

      final topicA = _filled(32, 0xa1);
      final topicB = _filled(32, 0xb2);

      final pages = <RemoteStatementStoreSubscribeItem>[];
      final sub = handlers
          .subscribe(
            _ctx,
            RemoteStatementStoreSubscribeRequestMatchAll([topicA, topicB]),
          )
          .listen(pages.add);

      await pumpEventQueue();
      // Matches: carries both topics.
      final both = _statement(topics: [topicA, topicB]);
      chain.notify('stmt-sub', bytesToHex(encodeStatement(both)));
      // Does not match: only one of the two required topics.
      chain.notify(
        'stmt-sub',
        bytesToHex(encodeStatement(_statement(topics: [topicA]))),
      );
      await pumpEventQueue();
      await sub.cancel();

      expect(pages, hasLength(1));
      expect(pages.single.isComplete, isTrue);
      expect(
        encodeStatement(pages.single.statements.single),
        encodeStatement(both),
      );
      // Cancelling unsubscribes.
      expect(
        chain.requests
            .any((r) => r['method'] == 'statement_unsubscribeStatement'),
        isTrue,
      );
    });

    test('MatchAny passes a statement sharing one topic', () async {
      final chain = FakeChain()
        ..responders['statement_subscribeStatement'] = (_) => 'stmt-sub';
      final handlers = SmoldotStatementStoreHandlers(
        () async => JsonRpcClient(chain.provider),
      );

      final wanted = _filled(32, 0xa1);
      final other = _filled(32, 0xff);

      final pages = <RemoteStatementStoreSubscribeItem>[];
      final sub = handlers
          .subscribe(
            _ctx,
            RemoteStatementStoreSubscribeRequestMatchAny([wanted]),
          )
          .listen(pages.add);

      await pumpEventQueue();
      chain.notify(
        'stmt-sub',
        bytesToHex(encodeStatement(_statement(topics: [other, wanted]))),
      );
      await pumpEventQueue();
      await sub.cancel();

      expect(pages, hasLength(1));
    });
  });

  group('createProof', () {
    test('createProof is unsupported (signing is the wallet)', () async {
      final handlers = SmoldotStatementStoreHandlers(
        () async => throw StateError('unused'),
      );
      final result = await handlers.createProof(
        _ctx,
        RemoteStatementStoreCreateProofRequest(
          productAccountId: const ProductAccountId(
            dotNsIdentifier: 'truapi-playground.dot',
            derivationIndex: 0,
          ),
          statement: const Statement(topics: []),
        ),
      );
      expect(result.isErr, isTrue);
      expect(
        (result as Err).error,
        isA<RemoteStatementStoreCreateProofErrorUnknown>(),
      );
    });

    test('createProofAuthorized is unsupported', () async {
      final handlers = SmoldotStatementStoreHandlers(
        () async => throw StateError('unused'),
      );
      final result = await handlers.createProofAuthorized(
        _ctx,
        const Statement(topics: []),
      );
      expect(result.isErr, isTrue);
    });
  });
}
