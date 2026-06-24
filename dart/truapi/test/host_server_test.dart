import 'dart:async';
import 'dart:typed_data';

import 'package:truapi/truapi.dart';
import 'package:truapi/src/scale.dart' as s;
import 'package:test/test.dart';

/// A minimal host implementation of the Account service. Only the two methods
/// exercised here have real bodies; the rest satisfy the interface.
class _AccountHandlers implements AccountHostHandlers {
  @override
  Stream<HostAccountConnectionStatusSubscribeItem> connectionStatusSubscribe(
    CallContext ctx,
  ) async* {
    yield HostAccountConnectionStatusSubscribeItem.connected;
    yield HostAccountConnectionStatusSubscribeItem.disconnected;
  }

  @override
  Future<Result<HostAccountGetResponse, HostAccountGetError>> getAccount(
    CallContext ctx,
    HostAccountGetRequest request,
  ) async {
    // Echo the derivation index into the returned public key so the test can
    // confirm the decoded request reached the handler intact.
    return Ok(
      HostAccountGetResponse(
        account: ProductAccount(
          publicKey:
              Uint8List.fromList([request.productAccountId.derivationIndex]),
        ),
      ),
    );
  }

  @override
  Future<Result<HostAccountGetAliasResponse, HostAccountGetError>>
      getAccountAlias(CallContext ctx, HostAccountGetAliasRequest request) =>
          throw UnimplementedError();

  @override
  Future<Result<HostAccountCreateProofResponse, HostAccountCreateProofError>>
      createAccountProof(
    CallContext ctx,
    HostAccountCreateProofRequest request,
  ) =>
          throw UnimplementedError();

  @override
  Future<Result<HostGetLegacyAccountsResponse, HostAccountGetError>>
      getLegacyAccounts(CallContext ctx) => throw UnimplementedError();

  @override
  Future<Result<HostGetUserIdResponse, HostGetUserIdError>> getUserId(
    CallContext ctx,
  ) =>
      throw UnimplementedError();

  @override
  Future<Result<HostRequestLoginResponse, HostRequestLoginError>> requestLogin(
    CallContext ctx,
    HostRequestLoginRequest request,
  ) =>
      throw UnimplementedError();
}

void main() {
  test('host dispatches a request frame to the handler and replies', () async {
    final channel = LoopbackChannel();
    final server = createHostServer(
      channel.host,
      buildAccountEntries(_AccountHandlers()),
    );

    // Product side: listen for the response frame.
    final response = Completer<HostAccountGetResponse>();
    channel.client.subscribe((frame) {
      final message = decodeWireMessage(frame);
      if (message.id != accountGetAccount.response) return;
      final result = s
          .versioned(0,
              s.result(hostAccountGetResponseCodec, hostAccountGetErrorCodec))
          .dec(message.value);
      switch (result) {
        case Ok(value: final value):
          response.complete(value);
        case Err():
          response.completeError(StateError('unexpected Err'));
      }
    });

    // Product side: send a raw accountGetAccount request frame.
    final payload = s.versioned(0, hostAccountGetRequestCodec).enc(
          const HostAccountGetRequest(
            productAccountId:
                ProductAccountId(dotNsIdentifier: 'a.dot', derivationIndex: 5),
          ),
        );
    channel.client.postMessage(
      encodeWireMessage(
        ProtocolMessage('p:1', accountGetAccount.request, payload),
      ),
    );

    final result = await response.future.timeout(const Duration(seconds: 2));
    expect(result.account.publicKey, Uint8List.fromList([5]));
    server.dispose();
  });

  test('host streams a subscription back as receive + interrupt frames',
      () async {
    final channel = LoopbackChannel();
    final server = createHostServer(
      channel.host,
      buildAccountEntries(_AccountHandlers()),
    );

    final items = <HostAccountConnectionStatusSubscribeItem>[];
    final done = Completer<void>();
    channel.client.subscribe((frame) {
      final message = decodeWireMessage(frame);
      if (message.id == accountConnectionStatusSubscribe.receive) {
        items.add(
          s
              .versioned(0, hostAccountConnectionStatusSubscribeItemCodec)
              .dec(message.value),
        );
      } else if (message.id == accountConnectionStatusSubscribe.interrupt) {
        if (!done.isCompleted) done.complete();
      }
    });

    // Start the subscription (no payload → versioned unit).
    channel.client.postMessage(
      encodeWireMessage(
        ProtocolMessage(
          'p:2',
          accountConnectionStatusSubscribe.start,
          s.versioned(0, s.unit).enc(s.unitValue),
        ),
      ),
    );

    await done.future.timeout(const Duration(seconds: 2));
    expect(items, [
      HostAccountConnectionStatusSubscribeItem.connected,
      HostAccountConnectionStatusSubscribeItem.disconnected,
    ]);
    server.dispose();
  });
}
