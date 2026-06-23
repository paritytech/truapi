import 'dart:typed_data';

import 'package:truapi/host.dart';
import 'package:truapi/truapi.dart' as c;
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
  test('generated client request is dispatched by the generated host',
      () async {
    final channel = c.LoopbackChannel();
    final server =
        createHostServer(channel.host, buildAccountEntries(_AccountHandlers()));
    final client = c.createClient(channel.client);

    final res = await client.account.getAccount(
      const HostAccountGetRequest(
        productAccountId:
            ProductAccountId(dotNsIdentifier: 'a.dot', derivationIndex: 5),
      ),
    );

    expect(res.isOk, isTrue);
    final response = (res as Ok).value as HostAccountGetResponse;
    expect(response.account.publicKey, Uint8List.fromList([5]));
    server.dispose();
  });

  test('generated client subscription is streamed by the generated host',
      () async {
    final channel = c.LoopbackChannel();
    final server =
        createHostServer(channel.host, buildAccountEntries(_AccountHandlers()));
    final client = c.createClient(channel.client);

    final items =
        await client.account.connectionStatusSubscribe().take(2).toList();

    expect(items, [
      HostAccountConnectionStatusSubscribeItem.connected,
      HostAccountConnectionStatusSubscribeItem.disconnected,
    ]);
    server.dispose();
  });
}
