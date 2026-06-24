/// SCALE codec for Substrate's `sp_statement_store::Statement`, which smoldot's
/// `statement_submit` / `statement_subscribeStatement` speak.
///
/// A statement encodes as a SCALE `Vec<Field>`: a compact length followed by
/// each present field as `tag(u8) ++ content`, emitted in ascending tag order:
///
/// | tag | field            | content                                  |
/// |-----|------------------|------------------------------------------|
/// | 0   | AuthenticityProof | the [StatementProof] (same bytes as TrUAPI) |
/// | 1   | DecryptionKey    | `[u8; 32]`                                |
/// | 2   | Priority         | `u32`                                     |
/// | 3   | Channel          | `[u8; 32]`                                |
/// | 4–7 | Topic1–Topic4    | `[u8; 32]` each (max four topics)         |
/// | 8   | Data             | `Vec<u8>`                                 |
///
/// TrUAPI's `SignedStatement` has no `priority` and an `expiry: u64` that the
/// product packs as `unixSeconds << 32`. We map that to Substrate's `Priority`
/// as `priority = (expiry >> 32)` (the high 32 bits), so a larger expiry sorts
/// as higher priority; decoding reverses it (`expiry = priority << 32`). This
/// expiry⇄priority mapping is the part the live statement-store test validates.
library;

import 'dart:typed_data';

import 'package:truapi/truapi.dart';

const int _tagProof = 0;
const int _tagDecryptionKey = 1;
const int _tagPriority = 2;
const int _tagChannel = 3;
const int _tagTopic1 = 4;
const int _tagTopic2 = 5;
const int _tagTopic3 = 6;
const int _tagTopic4 = 7;
const int _tagData = 8;

/// Maximum number of topics a Substrate statement can carry (Topic1..Topic4).
const int maxStatementTopics = 4;

/// Encode a [SignedStatement] into Substrate statement bytes.
Uint8List encodeStatement(SignedStatement statement) {
  if (statement.topics.length > maxStatementTopics) {
    throw ArgumentError(
      'a statement carries at most $maxStatementTopics topics, '
      'got ${statement.topics.length}',
    );
  }

  final fields = <(int, Uint8List)>[];
  fields.add((_tagProof, _encodeProof(statement.proof)));
  if (statement.decryptionKey != null) {
    fields.add((_tagDecryptionKey, _fixed(statement.decryptionKey!, 32)));
  }
  if (statement.expiry != null) {
    final priority = (statement.expiry! >> 32).toUnsigned(32).toInt();
    fields.add((_tagPriority, _u32(priority)));
  }
  if (statement.channel != null) {
    fields.add((_tagChannel, _fixed(statement.channel!, 32)));
  }
  for (var i = 0; i < statement.topics.length; i++) {
    fields.add((_tagTopic1 + i, _fixed(statement.topics[i], 32)));
  }
  if (statement.data != null) {
    fields.add((_tagData, _bytes(statement.data!)));
  }

  final out = BytesBuilder(copy: false);
  _writeCompact(out, fields.length);
  for (final (tag, content) in fields) {
    out.addByte(tag);
    out.add(content);
  }
  return out.toBytes();
}

/// Decode Substrate statement bytes into a [SignedStatement].
///
/// Throws [FormatException] if the proof field (required by [SignedStatement])
/// is absent or the bytes are malformed.
SignedStatement decodeStatement(Uint8List bytes) {
  final reader = _Reader(bytes);
  final fieldCount = reader.readCompact();

  StatementProof? proof;
  Uint8List? decryptionKey;
  BigInt? expiry;
  Uint8List? channel;
  final topics = <Uint8List>[];
  Uint8List? data;

  for (var i = 0; i < fieldCount; i++) {
    final tag = reader.readByte();
    switch (tag) {
      case _tagProof:
        proof = _decodeProof(reader);
      case _tagDecryptionKey:
        decryptionKey = reader.readFixed(32);
      case _tagPriority:
        expiry = BigInt.from(reader.readU32()) << 32;
      case _tagChannel:
        channel = reader.readFixed(32);
      case _tagTopic1:
      case _tagTopic2:
      case _tagTopic3:
      case _tagTopic4:
        topics.add(reader.readFixed(32));
      case _tagData:
        data = reader.readBytes();
      default:
        throw FormatException('unknown statement field tag $tag');
    }
  }

  if (proof == null) {
    throw const FormatException('statement has no authenticity proof');
  }
  return SignedStatement(
    proof: proof,
    decryptionKey: decryptionKey,
    expiry: expiry,
    channel: channel,
    topics: topics,
    data: data,
  );
}

Uint8List _encodeProof(StatementProof proof) {
  final out = BytesBuilder(copy: false);
  switch (proof) {
    case StatementProofSr25519():
      out.addByte(0);
      out.add(_fixed(proof.signature, 64));
      out.add(_fixed(proof.signer, 32));
    case StatementProofEd25519():
      out.addByte(1);
      out.add(_fixed(proof.signature, 64));
      out.add(_fixed(proof.signer, 32));
    case StatementProofEcdsa():
      out.addByte(2);
      out.add(_fixed(proof.signature, 65));
      out.add(_fixed(proof.signer, 33));
    case StatementProofOnChain():
      out.addByte(3);
      out.add(_fixed(proof.who, 32));
      out.add(_fixed(proof.blockHash, 32));
      out.add(_u64(proof.event));
  }
  return out.toBytes();
}

StatementProof _decodeProof(_Reader reader) {
  final variant = reader.readByte();
  switch (variant) {
    case 0:
      return StatementProofSr25519(
        signature: reader.readFixed(64),
        signer: reader.readFixed(32),
      );
    case 1:
      return StatementProofEd25519(
        signature: reader.readFixed(64),
        signer: reader.readFixed(32),
      );
    case 2:
      return StatementProofEcdsa(
        signature: reader.readFixed(65),
        signer: reader.readFixed(33),
      );
    case 3:
      return StatementProofOnChain(
        who: reader.readFixed(32),
        blockHash: reader.readFixed(32),
        event: reader.readU64(),
      );
    default:
      throw FormatException('unknown statement proof variant $variant');
  }
}

Uint8List _fixed(Uint8List value, int length) {
  if (value.length != length) {
    throw ArgumentError('expected $length bytes, got ${value.length}');
  }
  return value;
}

Uint8List _u32(int value) {
  final bytes = Uint8List(4);
  ByteData.view(bytes.buffer).setUint32(0, value, Endian.little);
  return bytes;
}

Uint8List _u64(BigInt value) {
  final bytes = Uint8List(8);
  final data = ByteData.view(bytes.buffer);
  data.setUint64(0, value.toUnsigned(64).toInt(), Endian.little);
  return bytes;
}

/// `Vec<u8>`: a compact length prefix followed by the raw bytes.
Uint8List _bytes(Uint8List value) {
  final out = BytesBuilder(copy: false);
  _writeCompact(out, value.length);
  out.add(value);
  return out.toBytes();
}

/// Write a SCALE compact-encoded non-negative integer.
void _writeCompact(BytesBuilder out, int value) {
  if (value < 0) throw ArgumentError('compact value must be non-negative');
  if (value < 1 << 6) {
    out.addByte(value << 2);
  } else if (value < 1 << 14) {
    final v = (value << 2) | 0x01;
    out.addByte(v & 0xff);
    out.addByte((v >> 8) & 0xff);
  } else if (value < 1 << 30) {
    final v = (value << 2) | 0x02;
    out.addByte(v & 0xff);
    out.addByte((v >> 8) & 0xff);
    out.addByte((v >> 16) & 0xff);
    out.addByte((v >> 24) & 0xff);
  } else {
    final bytes = <int>[];
    var v = value;
    while (v > 0) {
      bytes.add(v & 0xff);
      v >>= 8;
    }
    out.addByte(((bytes.length - 4) << 2) | 0x03);
    for (final b in bytes) {
      out.addByte(b);
    }
  }
}

class _Reader {
  _Reader(this._bytes);
  final Uint8List _bytes;
  int _offset = 0;

  int readByte() {
    if (_offset >= _bytes.length) {
      throw const FormatException('unexpected end of statement bytes');
    }
    return _bytes[_offset++];
  }

  Uint8List readFixed(int length) {
    if (_offset + length > _bytes.length) {
      throw const FormatException('unexpected end of statement bytes');
    }
    final slice = Uint8List.sublistView(_bytes, _offset, _offset + length);
    _offset += length;
    return Uint8List.fromList(slice);
  }

  int readU32() {
    final slice = readFixed(4);
    return ByteData.view(slice.buffer).getUint32(0, Endian.little);
  }

  BigInt readU64() {
    final slice = readFixed(8);
    final value = ByteData.view(slice.buffer).getUint64(0, Endian.little);
    return BigInt.from(value).toUnsigned(64);
  }

  Uint8List readBytes() => readFixed(readCompact());

  int readCompact() {
    final first = readByte();
    switch (first & 0x03) {
      case 0:
        return first >> 2;
      case 1:
        return (first >> 2) | (readByte() << 6);
      case 2:
        final b1 = readByte();
        final b2 = readByte();
        final b3 = readByte();
        return (first >> 2) | (b1 << 6) | (b2 << 14) | (b3 << 22);
      default:
        final length = (first >> 2) + 4;
        var value = 0;
        for (var i = 0; i < length; i++) {
          value |= readByte() << (8 * i);
        }
        return value;
    }
  }
}
