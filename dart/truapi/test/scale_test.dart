import 'dart:typed_data';

import 'package:truapi/src/result.dart';
import 'package:truapi/src/scale.dart' as s;
import 'package:test/test.dart';

String hex(Uint8List b) =>
    b.map((x) => x.toRadixString(16).padLeft(2, '0')).join();

Uint8List bytesOf(List<int> v) => Uint8List.fromList(v);

void main() {
  group('integers', () {
    test('u8', () {
      expect(hex(s.u8.enc(1)), '01');
      expect(hex(s.u8.enc(255)), 'ff');
      expect(s.u8.dec(bytesOf([0x2a])), 42);
    });

    test('u16/u32 little-endian', () {
      expect(hex(s.u16.enc(1)), '0100');
      expect(hex(s.u32.enc(1)), '01000000');
      expect(hex(s.u32.enc(0xdeadbeef)), 'efbeadde');
      expect(s.u32.dec(bytesOf([0xef, 0xbe, 0xad, 0xde])), 0xdeadbeef);
    });

    test('u64/u128 as BigInt', () {
      expect(hex(s.u64.enc(BigInt.from(1))), '0100000000000000');
      final big = BigInt.parse('18446744073709551615'); // u64::MAX
      expect(hex(s.u64.enc(big)), 'ffffffffffffffff');
      expect(s.u64.dec(s.u64.enc(big)), big);
      final u128max = (BigInt.one << 128) - BigInt.one;
      expect(s.u128.dec(s.u128.enc(u128max)), u128max);
    });

    test('signed round-trips', () {
      for (final v in [0, 1, -1, 127, -128]) {
        expect(s.i8.dec(s.i8.enc(v)), v);
      }
      for (final v in [0, 1, -1, 32767, -32768]) {
        expect(s.i16.dec(s.i16.enc(v)), v);
      }
      expect(s.i64.dec(s.i64.enc(BigInt.from(-5))), BigInt.from(-5));
    });
  });

  group('compact', () {
    test('known vectors', () {
      expect(hex(s.compact.enc(BigInt.zero)), '00');
      expect(hex(s.compact.enc(BigInt.from(1))), '04');
      expect(hex(s.compact.enc(BigInt.from(63))), 'fc');
      expect(hex(s.compact.enc(BigInt.from(64))), '0101');
      expect(hex(s.compact.enc(BigInt.from(16383))), 'fdff');
      expect(hex(s.compact.enc(BigInt.from(16384))), '02000100');
      expect(hex(s.compact.enc(BigInt.from(1073741823))), 'feffffff');
      expect(hex(s.compact.enc(BigInt.from(1073741824))), '0300000040');
    });

    test('round-trips large', () {
      final big = BigInt.parse('1000000000000000000000');
      expect(s.compact.dec(s.compact.enc(big)), big);
    });
  });

  group('bool / option', () {
    test('bool', () {
      expect(hex(s.boolCodec.enc(true)), '01');
      expect(hex(s.boolCodec.enc(false)), '00');
    });

    test('option', () {
      final c = s.option(s.u8);
      expect(hex(c.enc(null)), '00');
      expect(hex(c.enc(7)), '0107');
      expect(c.dec(bytesOf([0x01, 0x07])), 7);
      expect(c.dec(bytesOf([0x00])), null);
    });

    test('optionBool', () {
      expect(hex(s.optionBool.enc(null)), '00');
      expect(hex(s.optionBool.enc(true)), '01');
      expect(hex(s.optionBool.enc(false)), '02');
    });
  });

  group('strings / bytes / vectors', () {
    test('str', () {
      expect(hex(s.str.enc('hello')), '1468656c6c6f');
      expect(s.str.dec(s.str.enc('héllo ☃')), 'héllo ☃');
    });

    test('Vec<u8>', () {
      expect(hex(s.bytes.enc(bytesOf([1, 2, 3]))), '0c010203');
      expect(s.bytes.dec(bytesOf([0x0c, 1, 2, 3])), bytesOf([1, 2, 3]));
    });

    test('fixed bytes [u8; 4]', () {
      final c = s.bytesFixed(4);
      expect(hex(c.enc(bytesOf([1, 2, 3, 4]))), '01020304');
      expect(() => c.enc(bytesOf([1, 2, 3])), throwsArgumentError);
    });

    test('Vec<u32>', () {
      final c = s.vector(s.u32);
      expect(hex(c.enc([1, 2])), '080100000002000000');
      expect(c.dec(c.enc([5, 6, 7])), [5, 6, 7]);
    });
  });

  group('result / tuples / versioned', () {
    test('Result', () {
      final c = s.result(s.u8, s.str);
      expect(hex(c.enc(const Ok(7))), '0007');
      expect(hex(c.enc(const Err('no'))), '01086e6f');
      expect(c.dec(c.enc(const Ok<int, String>(9))), const Ok<int, String>(9));
    });

    test('tuple2', () {
      final c = s.tuple2(s.u8, s.boolCodec);
      expect(hex(c.enc((7, true))), '0701');
      expect(c.dec(c.enc((3, false))), (3, false));
    });

    test('versioned wrapper writes the discriminant', () {
      final c = s.versioned(0, s.u8); // V1 → index 0
      expect(hex(c.enc(42)), '002a');
      expect(c.dec(bytesOf([0x00, 0x2a])), 42);
    });
  });
}
