import 'dart:typed_data';

import 'package:truapi/truapi.dart';
import 'package:truapi/src/scale.dart' as s;
import 'package:test/test.dart';

/// Drives the generated `TruapiClient` against a fake host over the loopback
/// channel, where the host decodes/encodes with the same generated codecs. This
/// proves the generated client's payload encoding, versioned wrapping, and
/// response decoding all line up end-to-end.
void main() {
  Uint8List u8(List<int> v) => Uint8List.fromList(v);

  test('account.getAccount round-trips an Ok response', () async {
    final channel = LoopbackChannel();
    final client = createClient(channel.client);

    // Fake host: answer accountGetAccount with an Ok(ProductAccount).
    channel.host.subscribe((frame) {
      final msg = decodeWireMessage(frame);
      if (msg.id != accountGetAccount.request) return;
      final request = s.versioned(0, hostAccountGetRequestCodec).dec(msg.value);
      expect(request.productAccountId.dotNsIdentifier, 'my-app.dot');
      expect(request.productAccountId.derivationIndex, 3);

      final responseCodec = s.versioned(
        0,
        s.result(hostAccountGetResponseCodec, hostAccountGetErrorCodec),
      );
      final response = responseCodec.enc(
        Ok(HostAccountGetResponse(
          account: ProductAccount(publicKey: u8([0xde, 0xad])),
        )),
      );
      channel.host.postMessage(
        encodeWireMessage(
          ProtocolMessage(msg.requestId, accountGetAccount.response, response),
        ),
      );
    });

    final result = await client.account.getAccount(
      const HostAccountGetRequest(
        productAccountId: ProductAccountId(
          dotNsIdentifier: 'my-app.dot',
          derivationIndex: 3,
        ),
      ),
    );

    expect(result.isOk, isTrue);
    final account = (result as Ok).value as HostAccountGetResponse;
    expect(account.account.publicKey, u8([0xde, 0xad]));
  });

  test('account.getAccount round-trips a typed Err response', () async {
    final channel = LoopbackChannel();
    final client = createClient(channel.client);

    channel.host.subscribe((frame) {
      final msg = decodeWireMessage(frame);
      if (msg.id != accountGetAccount.request) return;
      final responseCodec = s.versioned(
        0,
        s.result(hostAccountGetResponseCodec, hostAccountGetErrorCodec),
      );
      final response = responseCodec.enc(
        const Err<HostAccountGetResponse, HostAccountGetError>(
          HostAccountGetErrorNotConnected(),
        ),
      );
      channel.host.postMessage(
        encodeWireMessage(
          ProtocolMessage(msg.requestId, accountGetAccount.response, response),
        ),
      );
    });

    final result = await client.account.getAccount(
      const HostAccountGetRequest(
        productAccountId:
            ProductAccountId(dotNsIdentifier: 'x.dot', derivationIndex: 0),
      ),
    );

    expect(result.isErr, isTrue);
    expect((result as Err).error, isA<HostAccountGetErrorNotConnected>());
  });
}
