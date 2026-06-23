/// SCALE codec primitives used by the generated TrUAPI client.
///
/// A hand-written, dependency-light port of the combinator codecs the
/// TypeScript client gets from `scale-ts` (see `js/packages/truapi/src/scale.ts`),
/// plus the Polkadot-flavour helpers it adds (fixed/var byte arrays, lazy
/// recursive codecs, `OptionBool`, and `V<N>`-indexed versioned wrappers).
///
/// The wire format is the contract: every codec here must produce bytes
/// identical to `parity_scale_codec` on the Rust side and `scale-ts` on the
/// TypeScript side. The `wire_vectors` test asserts that byte-for-byte.
library;

import 'dart:convert';
import 'dart:typed_data';

import 'result.dart';

/// Cursor over an immutable byte buffer that combinator decoders advance as
/// they consume fields.
class Input {
  Input(this.bytes);

  /// Backing buffer being decoded.
  final Uint8List bytes;

  /// Offset of the next unread byte.
  int offset = 0;

  /// Read a single byte, advancing the cursor.
  int takeByte() {
    if (offset >= bytes.length) {
      throw const FormatException('SCALE decode: unexpected end of input');
    }
    return bytes[offset++];
  }

  /// Read exactly [n] bytes as a fresh, independent slice.
  Uint8List takeBytes(int n) {
    if (offset + n > bytes.length) {
      throw FormatException(
        'SCALE decode: need $n bytes, ${bytes.length - offset} remain',
      );
    }
    final out = Uint8List.sublistView(bytes, offset, offset + n);
    offset += n;
    // Copy so callers may retain the slice independently of `bytes`.
    return Uint8List.fromList(out);
  }

  /// Whether the cursor has consumed the whole buffer.
  bool get atEnd => offset >= bytes.length;
}

/// A SCALE codec: encodes `T` into bytes and decodes `T` from an [Input].
///
/// Codecs compose through [encInto]/[decFrom], which thread a shared sink and
/// cursor so nested structures encode and decode in a single pass.
class Codec<T> {
  const Codec(this._enc, this._dec);

  final void Function(BytesBuilder out, T value) _enc;
  final T Function(Input input) _dec;

  /// Encode [value] into a freshly allocated byte buffer.
  Uint8List enc(T value) {
    final out = BytesBuilder(copy: false);
    _enc(out, value);
    return out.toBytes();
  }

  /// Encode [value] into a shared sink (used by composite codecs).
  void encInto(BytesBuilder out, T value) => _enc(out, value);

  /// Decode a `T` from a complete byte buffer.
  T dec(Uint8List bytes) => _dec(Input(bytes));

  /// Decode a `T` from a shared cursor (used by composite codecs).
  T decFrom(Input input) => _dec(input);

  /// Adapt this codec to a different value type via a pair of pure mappings.
  Codec<U> map<U>(T Function(U) toBase, U Function(T) fromBase) => Codec<U>(
        (out, value) => _enc(out, toBase(value)),
        (input) => fromBase(_dec(input)),
      );
}

// --- Fixed-width integers -------------------------------------------------

/// `bool`: one byte, `0` = false, `1` = true.
const Codec<bool> boolCodec = Codec<bool>(_encBool, _decBool);
void _encBool(BytesBuilder out, bool v) => out.addByte(v ? 1 : 0);
bool _decBool(Input i) {
  final b = i.takeByte();
  if (b > 1) throw FormatException('SCALE decode: invalid bool byte $b');
  return b == 1;
}

/// `u8`: a single unsigned byte.
const Codec<int> u8 = Codec<int>(_encU8, _decU8);
void _encU8(BytesBuilder out, int v) => out.addByte(v & 0xff);
int _decU8(Input i) => i.takeByte();

/// `u16`: little-endian unsigned, 2 bytes.
final Codec<int> u16 = _uintLe(2);

/// `u32`: little-endian unsigned, 4 bytes.
final Codec<int> u32 = _uintLe(4);

/// `i8`: signed byte (two's complement).
const Codec<int> i8 = Codec<int>(_encU8, _decI8);
int _decI8(Input i) {
  final b = i.takeByte();
  return b < 0x80 ? b : b - 0x100;
}

/// `i16`: little-endian signed, 2 bytes.
final Codec<int> i16 = _intLe(2);

/// `i32`: little-endian signed, 4 bytes.
final Codec<int> i32 = _intLe(4);

/// `u64`: little-endian unsigned, 8 bytes. Uses [BigInt] to avoid truncation.
final Codec<BigInt> u64 = _bigUintLe(8);

/// `u128`: little-endian unsigned, 16 bytes.
final Codec<BigInt> u128 = _bigUintLe(16);

/// `i64`: little-endian signed, 8 bytes.
final Codec<BigInt> i64 = _bigIntLe(8);

/// `i128`: little-endian signed, 16 bytes.
final Codec<BigInt> i128 = _bigIntLe(16);

Codec<int> _uintLe(int n) => Codec<int>(
      (out, v) {
        var value = v;
        for (var b = 0; b < n; b++) {
          out.addByte(value & 0xff);
          value >>= 8;
        }
      },
      (i) {
        final bytes = i.takeBytes(n);
        var value = 0;
        for (var b = n - 1; b >= 0; b--) {
          value = (value << 8) | bytes[b];
        }
        return value;
      },
    );

Codec<int> _intLe(int n) {
  final unsigned = _uintLe(n);
  final signBit = 1 << (n * 8 - 1);
  final wrap = 1 << (n * 8);
  return Codec<int>(
    (out, v) => unsigned.encInto(out, v < 0 ? v + wrap : v),
    (i) {
      final raw = unsigned.decFrom(i);
      return raw >= signBit ? raw - wrap : raw;
    },
  );
}

Codec<BigInt> _bigUintLe(int n) => Codec<BigInt>(
      (out, v) {
        var value = v;
        final mask = BigInt.from(0xff);
        for (var b = 0; b < n; b++) {
          out.addByte((value & mask).toInt());
          value >>= 8;
        }
      },
      (i) {
        final bytes = i.takeBytes(n);
        var value = BigInt.zero;
        for (var b = n - 1; b >= 0; b--) {
          value = (value << 8) | BigInt.from(bytes[b]);
        }
        return value;
      },
    );

Codec<BigInt> _bigIntLe(int n) {
  final unsigned = _bigUintLe(n);
  final signBit = BigInt.one << (n * 8 - 1);
  final wrap = BigInt.one << (n * 8);
  return Codec<BigInt>(
    (out, v) => unsigned.encInto(out, v.isNegative ? v + wrap : v),
    (i) {
      final raw = unsigned.decFrom(i);
      return raw >= signBit ? raw - wrap : raw;
    },
  );
}

// --- Compact --------------------------------------------------------------

/// SCALE compact integer (`Compact<uN>`). Accepts and yields [BigInt] so the
/// full unsigned range round-trips regardless of magnitude.
const Codec<BigInt> compact = Codec<BigInt>(_encCompact, _decCompact);

void _encCompact(BytesBuilder out, BigInt value) {
  if (value.isNegative) {
    throw ArgumentError('SCALE compact cannot encode a negative value: $value');
  }
  if (value < BigInt.from(64)) {
    out.addByte((value.toInt() << 2));
  } else if (value < BigInt.from(1 << 14)) {
    final v = (value.toInt() << 2) | 0x01;
    out
      ..addByte(v & 0xff)
      ..addByte((v >> 8) & 0xff);
  } else if (value < BigInt.from(1 << 30)) {
    final v = (value.toInt() << 2) | 0x02;
    out
      ..addByte(v & 0xff)
      ..addByte((v >> 8) & 0xff)
      ..addByte((v >> 16) & 0xff)
      ..addByte((v >> 24) & 0xff);
  } else {
    final bytes = <int>[];
    var v = value;
    final mask = BigInt.from(0xff);
    while (v > BigInt.zero) {
      bytes.add((v & mask).toInt());
      v >>= 8;
    }
    out.addByte(((bytes.length - 4) << 2) | 0x03);
    for (final b in bytes) {
      out.addByte(b);
    }
  }
}

BigInt _decCompact(Input i) {
  final first = i.takeByte();
  switch (first & 0x03) {
    case 0:
      return BigInt.from(first >> 2);
    case 1:
      final second = i.takeByte();
      return BigInt.from(((first >> 2) | (second << 6)) & 0x3fff);
    case 2:
      final b1 = i.takeByte();
      final b2 = i.takeByte();
      final b3 = i.takeByte();
      final v =
          ((first >> 2) | (b1 << 6) | (b2 << 14) | (b3 << 22)) & 0xffffffff;
      return BigInt.from(v).toUnsigned(32);
    default:
      final len = (first >> 2) + 4;
      final bytes = i.takeBytes(len);
      var value = BigInt.zero;
      for (var b = len - 1; b >= 0; b--) {
        value = (value << 8) | BigInt.from(bytes[b]);
      }
      return value;
  }
}

// --- Strings & bytes ------------------------------------------------------

/// `String`: compact length prefix + UTF-8 bytes.
final Codec<String> str = Codec<String>(
  (out, v) {
    final encoded = utf8.encode(v);
    _encCompact(out, BigInt.from(encoded.length));
    out.add(encoded);
  },
  (i) {
    final len = _decCompact(i).toInt();
    return utf8.decode(i.takeBytes(len));
  },
);

/// `Vec<u8>`: compact length prefix + raw bytes.
final Codec<Uint8List> bytes = Codec<Uint8List>(
  (out, v) {
    _encCompact(out, BigInt.from(v.length));
    out.add(v);
  },
  (i) {
    final len = _decCompact(i).toInt();
    return i.takeBytes(len);
  },
);

/// `[u8; N]`: exactly [length] raw bytes, no length prefix.
Codec<Uint8List> bytesFixed(int length) => Codec<Uint8List>(
      (out, v) {
        if (v.length != length) {
          throw ArgumentError(
            'fixed byte array expected $length bytes, got ${v.length}',
          );
        }
        out.add(v);
      },
      (i) => i.takeBytes(length),
    );

// --- Containers -----------------------------------------------------------

/// The Rust unit type `()`. A real type (not `void`) so it can instantiate
/// generics such as `Component<()>` → `Component<Unit>`.
class Unit {
  const Unit();

  @override
  bool operator ==(Object other) => other is Unit;

  @override
  int get hashCode => 0;

  @override
  String toString() => '()';
}

/// The single inhabitant of [Unit].
const Unit unitValue = Unit();

/// The unit type `()`: zero bytes on the wire.
const Codec<Unit> unit = Codec<Unit>(_encUnit, _decUnit);
void _encUnit(BytesBuilder out, Unit v) {}
Unit _decUnit(Input i) => unitValue;

/// `Option<T>`: one tag byte (`0` = none, `1` = some) then the inner value.
/// Encoded/decoded as a nullable Dart value.
Codec<T?> option<T>(Codec<T> inner) => Codec<T?>(
      (out, v) {
        if (v == null) {
          out.addByte(0);
        } else {
          out.addByte(1);
          inner.encInto(out, v);
        }
      },
      (i) {
        final tag = i.takeByte();
        if (tag == 0) return null;
        if (tag != 1) {
          throw FormatException('SCALE decode: invalid Option tag $tag');
        }
        return inner.decFrom(i);
      },
    );

/// Substrate `OptionBool`: a one-byte tri-state. `null` → 0, `true` → 1,
/// `false` → 2. Matches `parity_scale_codec::OptionBool`.
final Codec<bool?> optionBool = Codec<bool?>(
  (out, v) => out.addByte(v == null ? 0 : (v ? 1 : 2)),
  (i) {
    switch (i.takeByte()) {
      case 0:
        return null;
      case 1:
        return true;
      case 2:
        return false;
      default:
        throw const FormatException('SCALE decode: invalid OptionBool byte');
    }
  },
);

/// `Vec<T>`: compact length prefix then [count] encoded items.
Codec<List<T>> vector<T>(Codec<T> inner) => Codec<List<T>>(
      (out, v) {
        _encCompact(out, BigInt.from(v.length));
        for (final item in v) {
          inner.encInto(out, item);
        }
      },
      (i) {
        final len = _decCompact(i).toInt();
        return List<T>.generate(len, (_) => inner.decFrom(i), growable: false);
      },
    );

/// `[T; N]`: exactly [length] items with no length prefix (the fixed-array
/// analogue of [vector]).
Codec<List<T>> vectorFixed<T>(Codec<T> inner, int length) => Codec<List<T>>(
      (out, v) {
        if (v.length != length) {
          throw ArgumentError(
            'fixed array expected $length items, got ${v.length}',
          );
        }
        for (final item in v) {
          inner.encInto(out, item);
        }
      },
      (i) => List<T>.generate(length, (_) => inner.decFrom(i), growable: false),
    );

/// `Result<O, E>`: one tag byte (`0` = Ok, `1` = Err) then the inner value.
Codec<Result<O, E>> result<O, E>(Codec<O> ok, Codec<E> err) =>
    Codec<Result<O, E>>(
      (out, v) {
        switch (v) {
          case Ok<O, E>(value: final value):
            out.addByte(0);
            ok.encInto(out, value);
          case Err<O, E>(error: final error):
            out.addByte(1);
            err.encInto(out, error);
        }
      },
      (i) {
        final tag = i.takeByte();
        switch (tag) {
          case 0:
            return Ok<O, E>(ok.decFrom(i));
          case 1:
            return Err<O, E>(err.decFrom(i));
          default:
            throw FormatException('SCALE decode: invalid Result tag $tag');
        }
      },
    );

/// Defers codec construction until first use so mutually recursive generated
/// codecs can reference each other safely.
Codec<T> lazy<T>(Codec<T> Function() factory) {
  Codec<T>? resolved;
  Codec<T> get() => resolved ??= factory();
  return Codec<T>(
    (out, v) => get().encInto(out, v),
    (i) => get().decFrom(i),
  );
}

/// A `V<N>`-indexed versioned wrapper around a single inner codec.
///
/// The TrUAPI client encodes exactly one selected wire version: it writes the
/// SCALE enum discriminant [index] (`N - 1`) then the inner payload, and on
/// decode consumes the discriminant and returns the inner value. This keeps
/// the public Dart surface the inner type, matching the TypeScript client's
/// `.value` stripping.
Codec<T> versioned<T>(int index, Codec<T> inner) => Codec<T>(
      (out, v) {
        out.addByte(index);
        inner.encInto(out, v);
      },
      (i) {
        i.takeByte(); // consume the version discriminant
        return inner.decFrom(i);
      },
    );

// --- Tuples ---------------------------------------------------------------

/// 2-tuple `(A, B)` encoded as its fields in order.
Codec<(A, B)> tuple2<A, B>(Codec<A> a, Codec<B> b) => Codec<(A, B)>(
      (out, v) {
        a.encInto(out, v.$1);
        b.encInto(out, v.$2);
      },
      (i) => (a.decFrom(i), b.decFrom(i)),
    );

/// 3-tuple `(A, B, C)` encoded as its fields in order.
Codec<(A, B, C)> tuple3<A, B, C>(Codec<A> a, Codec<B> b, Codec<C> c) =>
    Codec<(A, B, C)>(
      (out, v) {
        a.encInto(out, v.$1);
        b.encInto(out, v.$2);
        c.encInto(out, v.$3);
      },
      (i) => (a.decFrom(i), b.decFrom(i), c.decFrom(i)),
    );

/// 4-tuple `(A, B, C, D)` encoded as its fields in order.
Codec<(A, B, C, D)> tuple4<A, B, C, D>(
  Codec<A> a,
  Codec<B> b,
  Codec<C> c,
  Codec<D> d,
) =>
    Codec<(A, B, C, D)>(
      (out, v) {
        a.encInto(out, v.$1);
        b.encInto(out, v.$2);
        c.encInto(out, v.$3);
        d.encInto(out, v.$4);
      },
      (i) => (a.decFrom(i), b.decFrom(i), c.decFrom(i), d.decFrom(i)),
    );
