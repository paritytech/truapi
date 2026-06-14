/** SCALE codec primitives used by the generated client.
 *
 * Thin wrapper over `scale-ts`: re-exports its primitives and combinators,
 * plus the Polkadot-flavour helpers it does not ship (hex-encoded bytes,
 * lazy recursive codecs, and `V<N>`-indexed tagged unions).
 */

import {
  Bytes,
  Enum,
  Struct,
  _void,
  createCodec,
  createDecoder,
  enhanceCodec,
  str as scaleStr,
  u8,
  type Codec,
} from "scale-ts";
import {
  bytesToHex as encodeHex,
  hexToBytes as decodeHex,
} from "@noble/hashes/utils.js";

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
  compact,
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

/**
 * Substrate `OptionBool`: a one-byte `Option<bool>`.
 *
 * Canonical SCALE encoding (matches `parity_scale_codec::OptionBool`):
 * `undefined` → `0`, `true` → `1`, `false` → `2`.
 */
export const OptionBool: Codec<boolean | undefined> = enhanceCodec(
  u8,
  (value: boolean | undefined) => (value === undefined ? 0 : value ? 1 : 2),
  (byte: number) => {
    switch (byte) {
      case 0:
        return undefined;
      case 1:
        return true;
      case 2:
        return false;
      default:
        throw new Error(
          `Unknown OptionBool byte: ${byte}. Expected 0, 1, or 2.`,
        );
    }
  },
);

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
  return `0x${encodeHex(bytes)}`;
}

/** Decode a hex string into a byte array. Tolerates a missing `0x` prefix. */
export function hexToBytes(hex: string): Uint8Array {
  return decodeHex(hex.startsWith("0x") ? hex.slice(2) : hex);
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
 * Same wire format as `scale-ts`'s `Enum`, but exposes `value` as optional in
 * the public TS type when the variant codec is `Codec<undefined>`. Lets unit
 * variants of mixed enums round-trip as `{ tag: "X" }` (no `value` key).
 */
export function TaggedUnion<O extends TaggedUnionCodecs>(
  inner: O,
): Codec<TaggedUnionValue<O>> {
  return Enum(inner) as unknown as Codec<TaggedUnionValue<O>>;
}

/**
 * Wire codec for Rust `CallError<D>`, projected to the public domain error `D`.
 *
 * Generated TypeScript APIs expose only the domain error union in
 * `ResultAsync<Ok, D>`. The Rust host still wraps that value in
 * `CallError::Domain` on the wire so framework errors can share the response
 * channel. Encoding always emits `Domain`; decoding returns the inner domain
 * value and throws for framework-level failures that have no public `D` shape.
 */
export function CallError<D>(domain: Codec<D>): Codec<D> {
  type WireCallError =
    | { tag: "Domain"; value: D }
    | { tag: "Denied"; value?: undefined }
    | { tag: "Unsupported"; value?: undefined }
    | { tag: "MalformedFrame"; value: { reason: string } }
    | { tag: "HostFailure"; value: { reason: string } };

  const wire = Enum({
    Domain: domain,
    Denied: _void,
    Unsupported: _void,
    MalformedFrame: Struct({ reason: scaleStr }),
    HostFailure: Struct({ reason: scaleStr }),
  }) as unknown as Codec<WireCallError>;

  return enhanceCodec(
    wire,
    (value: D): WireCallError => ({ tag: "Domain", value }),
    (value: WireCallError): D => {
      switch (value.tag) {
        case "Domain":
          return value.value;
        case "Denied":
          throw new Error("Host denied the request");
        case "Unsupported":
          throw new Error("Host does not support this request");
        case "MalformedFrame":
          throw new Error(`Malformed request frame: ${value.value.reason}`);
        case "HostFailure":
          throw new Error(`Host failure: ${value.value.reason}`);
      }
    },
  );
}

type TaggedUnionCodecs = {
  [Sym: symbol]: never;
  [Num: number]: never;
  [Str: string]: Codec<any>;
};

type TaggedUnionValue<O extends TaggedUnionCodecs> = {
  [K in keyof O & string]: O[K] extends Codec<infer T>
    ? [T] extends [undefined]
      ? { tag: K; value?: undefined }
      : { tag: K; value: T }
    : never;
}[keyof O & string];

/**
 * Enum without payloads — maps string labels to SCALE discriminant bytes.
 *
 * `scale-ts` models `Enum({ Foo: _void, Bar: _void })` as tagged objects. For
 * user-facing TrUAPI enums with only unit variants, we keep the public TS shape
 * as a plain string union instead.
 */
export function Status<const T extends string>(
  ...variants: readonly T[]
): Codec<T> {
  return enhanceCodec(
    u8,
    (value: unknown) => {
      const index = variants.indexOf(value as T);
      if (index === -1) {
        throw new Error(`Unknown status value: ${String(value)}`);
      }
      return index;
    },
    (index: number) => {
      const value = variants[index];
      if (value === undefined) {
        throw new Error(`Unknown status index: ${index}`);
      }
      return value;
    },
  ) as unknown as Codec<T>;
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
    ? [T] extends [undefined]
      ? { tag: K; value?: undefined }
      : { tag: K; value: T }
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
