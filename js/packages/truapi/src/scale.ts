/** SCALE codec primitives used by the generated client.
 *
 * Thin wrapper over `scale-ts`: re-exports its primitives and combinators,
 * plus the Polkadot-flavour helpers it does not ship (hex-encoded bytes,
 * lazy recursive codecs, and `V<N>`-indexed tagged unions).
 */

import {
  Bytes,
  createCodec,
  createDecoder,
  enhanceCodec,
  u8,
  type Codec,
} from "scale-ts";

export type { Codec };
export type { ResultPayload } from "scale-ts";

export {
  Enum,
  Option,
  Result,
  Struct,
  Tuple,
  Vector,
  _void,
  bool,
  i8,
  i16,
  i32,
  i64,
  i128,
  str,
  u8,
  u16,
  u32,
  u64,
  u128,
} from "scale-ts";

/** Hex-encoded byte string, e.g. `"0xdeadbeef"`. */
export type HexString = `0x${string}`;

/** Assert that a string is a valid hex string (`0x...`). */
export function toHexString(value: string): HexString {
  if (!value.startsWith("0x")) {
    throw new Error(
      `Expected hex string starting with 0x, got: ${value.slice(0, 20)}`,
    );
  }
  return value as HexString;
}

/** Encode a byte array as a lower-case hex string with a `0x` prefix. */
export function bytesToHex(bytes: Uint8Array): HexString {
  let hex = "0x";
  for (let i = 0; i < bytes.length; i++) {
    hex += bytes[i]!.toString(16).padStart(2, "0");
  }
  return hex as HexString;
}

/** Decode a hex string into a byte array. Tolerates a missing `0x` prefix. */
export function hexToBytes(hex: string): Uint8Array {
  const start = hex.startsWith("0x") ? 2 : 0;
  const length = (hex.length - start) >> 1;
  const bytes = new Uint8Array(length);
  for (let i = 0; i < length; i++) {
    bytes[i] = parseInt(hex.substring(start + i * 2, start + i * 2 + 2), 16);
  }
  return bytes;
}

/**
 * SCALE codec for hex-encoded byte strings.
 *
 * Encode accepts a `0x`-prefixed hex string and emits SCALE bytes; decode
 * returns the bytes as a hex string. Pass `length` for fixed-size byte arrays
 * (`[u8; N]`); omit it for variable-length byte vectors (`Vec<u8>`).
 */
export function Hex(length?: number): Codec<HexString> {
  return enhanceCodec(
    Bytes(length),
    hexToBytes as unknown as (v: HexString) => Uint8Array,
    bytesToHex,
  ) as unknown as Codec<HexString>;
}

/**
 * Defers codec construction until first use so recursive generated codecs can
 * reference each other safely.
 */
export function lazy<T>(factory: () => Codec<T>): Codec<T> {
  let resolved: Codec<T> | undefined;
  const get = () => (resolved ??= factory());
  return createCodec(
    (value) => get().enc(value),
    (input) => get().dec(input),
  );
}

type IndexedVariantCodec<T> = readonly [index: number, codec: Codec<T>];
type IndexedVariantValue<
  Variants extends Record<string, IndexedVariantCodec<any>>,
  K extends keyof Variants & string,
> =
  Variants[K] extends IndexedVariantCodec<infer T>
    ? { tag: K; value: T }
    : never;

/**
 * Builds a tagged union codec with explicit SCALE discriminants.
 *
 * `scale-ts` assigns enum indexes by object key order. TrUAPI versioned enums pin
 * `V<N>` to index `N - 1`, including V2-only enums, so generated codecs use this
 * helper for versioned wire wrappers.
 */
export function indexedTaggedUnion<
  Variants extends Record<string, IndexedVariantCodec<any>>,
>(
  variants: Variants,
): Codec<
  {
    [K in keyof Variants & string]: IndexedVariantValue<Variants, K>;
  }[keyof Variants & string]
> {
  type Output = {
    [K in keyof Variants & string]: IndexedVariantValue<Variants, K>;
  }[keyof Variants & string];

  const byIndex = new Map<number, [string, Codec<unknown>]>();
  for (const [tag, [index, codec]] of Object.entries(variants)) {
    if (!Number.isInteger(index) || index < 0 || index > 255) {
      throw new Error(`Invalid enum discriminant for ${tag}: ${index}`);
    }
    if (byIndex.has(index)) {
      throw new Error(`Duplicate enum discriminant: ${index}`);
    }
    byIndex.set(index, [tag, codec]);
  }

  return createCodec(
    (value: Output) => {
      const variant = variants[value.tag];
      if (!variant) {
        throw new Error(`Unknown enum variant: ${value.tag}`);
      }
      const [index, codec] = variant;
      const payload = codec.enc(value.value);
      const out = new Uint8Array(payload.length + 1);
      out[0] = index;
      out.set(payload, 1);
      return out;
    },
    createDecoder((input) => {
      const index = u8.dec(input);
      const variant = byIndex.get(index);
      if (!variant) {
        throw new Error(`Unknown enum discriminant: ${index}`);
      }
      const [tag, codec] = variant;
      return { tag, value: codec.dec(input) } as Output;
    }),
  );
}
