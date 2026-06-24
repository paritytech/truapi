import 'dart:typed_data';

import 'package:truapi/truapi.dart';
import 'package:truapi_smoldot/truapi_smoldot.dart';
import 'package:test/test.dart';

Uint8List _filled(int length, int value) =>
    Uint8List.fromList(List.filled(length, value));

void main() {
  group('encodeStatement', () {
    test('a proof-only statement is a one-field Vec', () {
      final statement = SignedStatement(
        proof: StatementProofSr25519(
          signature: _filled(64, 0x01),
          signer: _filled(32, 0x02),
        ),
        topics: const [],
      );

      final bytes = encodeStatement(statement);

      // compact(1) ++ tag(0) ++ proof variant(0) ++ sig(64) ++ signer(32).
      expect(bytes[0], 0x04); // compact-encoded field count 1
      expect(bytes[1], 0x00); // AuthenticityProof field tag
      expect(bytes[2], 0x00); // Sr25519 proof variant
      expect(bytes.length, 3 + 64 + 32);
    });

    test('emits topics as Topic1..Topic4 in ascending tag order', () {
      final statement = SignedStatement(
        proof: StatementProofEd25519(
          signature: _filled(64, 0x00),
          signer: _filled(32, 0x00),
        ),
        topics: [_filled(32, 0xa1), _filled(32, 0xa2)],
      );

      final bytes = encodeStatement(statement);

      // 3 fields: proof + 2 topics.
      expect(bytes[0], 3 << 2);
      // Find the topic field tags (4 and 5) after the proof field.
      final proofLen = 1 + 1 + 64 + 32; // tag + variant + sig + signer
      expect(bytes[1 + proofLen], 0x04); // Topic1 tag
      expect(bytes[1 + proofLen + 1 + 32], 0x05); // Topic2 tag
    });

    test('rejects more than four topics', () {
      final statement = SignedStatement(
        proof: StatementProofSr25519(
          signature: _filled(64, 0),
          signer: _filled(32, 0),
        ),
        topics: [for (var i = 0; i < 5; i++) _filled(32, i)],
      );
      expect(() => encodeStatement(statement), throwsArgumentError);
    });
  });

  group('round-trip', () {
    final proofs = <StatementProof>[
      StatementProofSr25519(signature: _filled(64, 1), signer: _filled(32, 2)),
      StatementProofEd25519(signature: _filled(64, 3), signer: _filled(32, 4)),
      StatementProofEcdsa(signature: _filled(65, 5), signer: _filled(33, 6)),
      StatementProofOnChain(
        who: _filled(32, 7),
        blockHash: _filled(32, 8),
        event: BigInt.from(9),
      ),
    ];

    for (final proof in proofs) {
      test('encode/decode preserves a ${proof.runtimeType} statement', () {
        final statement = SignedStatement(
          proof: proof,
          decryptionKey: _filled(32, 0xd0),
          expiry: BigInt.from(1893456000) << 32,
          channel: _filled(32, 0xc0),
          topics: [_filled(32, 0x11), _filled(32, 0x22), _filled(32, 0x33)],
          data: Uint8List.fromList([1, 2, 3, 4, 5]),
        );

        // SignedStatement.== is identity-based for its byte fields, so assert
        // round-trip via the canonical encoding: decode then re-encode.
        final bytes = encodeStatement(statement);
        expect(encodeStatement(decodeStatement(bytes)), bytes);
      });
    }

    test('preserves a minimal statement (proof only, no optionals)', () {
      final statement = SignedStatement(
        proof: StatementProofEcdsa(
          signature: _filled(65, 0x42),
          signer: _filled(33, 0x43),
        ),
        topics: const [],
      );
      final bytes = encodeStatement(statement);
      expect(encodeStatement(decodeStatement(bytes)), bytes);
    });

    test('expiry maps to the high 32 bits (priority) and back', () {
      const seconds = 1893456000;
      final statement = SignedStatement(
        proof: StatementProofSr25519(
          signature: _filled(64, 0),
          signer: _filled(32, 0),
        ),
        expiry: BigInt.from(seconds) << 32,
        topics: const [],
      );
      final decoded = decodeStatement(encodeStatement(statement));
      expect(decoded.expiry, BigInt.from(seconds) << 32);
    });
  });
}
