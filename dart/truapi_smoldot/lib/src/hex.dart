import 'dart:typed_data';

/// Encode bytes as a lower-case `0x`-prefixed hex string (JSON-RPC convention).
String bytesToHex(Uint8List bytes) {
  final out = StringBuffer('0x');
  for (final b in bytes) {
    out.write(b.toRadixString(16).padLeft(2, '0'));
  }
  return out.toString();
}

/// Decode a hex string (with or without a `0x` prefix) into bytes.
Uint8List hexToBytes(String hex) {
  final start = hex.startsWith('0x') ? 2 : 0;
  final length = (hex.length - start) ~/ 2;
  final out = Uint8List(length);
  for (var i = 0; i < length; i++) {
    out[i] =
        int.parse(hex.substring(start + i * 2, start + i * 2 + 2), radix: 16);
  }
  return out;
}
