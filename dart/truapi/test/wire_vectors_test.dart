import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';

import 'package:truapi/truapi.dart';
import 'package:truapi/src/scale.dart' as s;
import 'package:test/test.dart';

/// Cross-language conformance: the Dart generated codecs must produce bytes
/// identical to `parity_scale_codec` on the Rust side. The golden vectors are
/// produced by `cargo run -p truapi --example wire_vectors`.
String hex(Uint8List b) =>
    b.map((x) => x.toRadixString(16).padLeft(2, '0')).join();

Uint8List u8(List<int> v) => Uint8List.fromList(v);

void main() {
  final file = File('test/wire_vectors.json');
  if (!file.existsSync()) {
    test(
      'wire vectors (regenerate with `make dart`)',
      () {},
      skip: 'test/wire_vectors.json not generated; run '
          '`cargo run -p truapi --example wire_vectors -- dart/truapi/test/wire_vectors.json`',
    );
    return;
  }
  final golden =
      (jsonDecode(file.readAsStringSync()) as Map).cast<String, String>();

  void check(String name, Uint8List bytes) {
    final expected = golden[name];
    expect(expected, isNotNull, reason: 'no golden vector named "$name"');
    expect(hex(bytes), expected, reason: 'codec mismatch for "$name"');
  }

  test('struct: ProductAccountId', () {
    check(
      'product_account_id',
      productAccountIdCodec.enc(
        const ProductAccountId(
          dotNsIdentifier: 'my-product.dot',
          derivationIndex: 7,
        ),
      ),
    );
  });

  test('struct with Vec<u8>: ProductAccount', () {
    check('product_account',
        productAccountCodec.enc(ProductAccount(publicKey: u8([1, 2, 3, 4]))));
  });

  test('Option<String> Some/None: LegacyAccount', () {
    check(
      'legacy_account_some',
      legacyAccountCodec
          .enc(LegacyAccount(publicKey: u8([0xaa, 0xbb]), name: 'Wallet')),
    );
    check(
      'legacy_account_none',
      legacyAccountCodec.enc(LegacyAccount(publicKey: u8([]), name: null)),
    );
  });

  test('handshake request struct', () {
    check(
        'handshake_request',
        hostHandshakeRequestCodec
            .enc(const HostHandshakeRequest(codecVersion: 1)));
  });

  test('sealed enum unit variant: HostHandshakeError', () {
    check(
      'handshake_error_unsupported',
      hostHandshakeErrorCodec
          .enc(const HostHandshakeErrorUnsupportedProtocolVersion()),
    );
  });

  test('unit enum: TypographyStyle', () {
    check('typography_body_large',
        typographyStyleCodec.enc(TypographyStyle.bodyLargeRegular));
  });

  test('compact + Option<compact>: Dimensions', () {
    check(
      'dimensions',
      dimensionsCodec.enc(Dimensions(
        top: BigInt.from(10),
        end: BigInt.from(20),
        bottom: null,
        start: BigInt.from(5),
      )),
    );
  });

  test('OptionBool: ButtonProps', () {
    check(
      'button_props',
      buttonPropsCodec.enc(const ButtonProps(
        text: 'Go',
        variant: ButtonVariant.primary,
        enabled: true,
        loading: null,
        clickAction: 'go',
      )),
    );
  });

  test('fixed [u8; 32]: HostAccountGetAliasResponse', () {
    check(
      'account_get_alias_response',
      hostAccountGetAliasResponseCodec.enc(HostAccountGetAliasResponse(
        context: u8(List.filled(32, 7)),
        alias: u8([9, 9]),
      )),
    );
  });

  test('versioned V1 envelope writes 0x00 discriminant', () {
    check(
      'versioned_handshake_request_v1',
      s
          .versioned(0, hostHandshakeRequestCodec)
          .enc(const HostHandshakeRequest(codecVersion: 1)),
    );
  });
}
