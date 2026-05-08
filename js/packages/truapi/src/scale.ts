/** SCALE codec primitives used by the generated client.
 *
 * Thin wrapper over `scale-ts`: re-exports its primitives and combinators,
 * plus the handful of helpers scale-ts does not ship (lazy, f32/f64,
 * length-validated byte/array codecs).
 */

import {
  _void,
  Bytes,
  bool,
  compact as scaleCompact,
  createCodec,
  createDecoder,
  Enum,
  Option,
  Result,
  Struct,
  Tuple,
  Vector,
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
  type Codec,
} from "scale-ts";

export type { Codec };
export type { ResultPayload } from "scale-ts";

export {
  bool,
  str,
  u8,
  u16,
  u32,
  u64,
  u128,
  i8,
  i16,
  i32,
  i64,
  i128,
  Option as option,
  Result as result,
  Struct as struct,
  Tuple as tuple,
  Vector as vec,
  Enum as taggedUnion,
  _void as unit,
};

// scale-ts's `compact` widens to bigint above 2^32. Our wire protocol only
// uses compact for sizes that fit in u32, so guard explicitly.
export const compact: Codec<number> = createCodec(
  (value) => scaleCompact.enc(value),
  createDecoder((input) => {
    const decoded = scaleCompact.dec(input);
    if (typeof decoded === "bigint") {
      throw new Error("compact big-int mode not supported");
    }
    return decoded;
  }),
);

// Unsized bytes (compact length-prefixed) and fixed-size byte array.
// scale-ts's `Bytes(n)` silently truncates over-long encoder input, so wrap
// with explicit validation.
export const bytes = Bytes();

/**
 * Creates a fixed-length byte-array codec that rejects values whose encoded
 * length does not exactly match `length`.
 */
export function byteArray(length: number): Codec<Uint8Array> {
  const inner = Bytes(length);
  return createCodec((value: Uint8Array) => {
    if (value.length !== length) {
      throw new Error(
        `Expected byte array length ${length}, got ${value.length}`,
      );
    }
    return inner.enc(value);
  }, inner[1]);
}

/**
 * Creates a fixed-length array codec that validates the element count before
 * delegating to `scale-ts`.
 */
export function array<T>(inner: Codec<T>, length: number): Codec<T[]> {
  const vec = Vector(inner, length);
  return createCodec((value: T[]) => {
    if (value.length !== length) {
      throw new Error(`Expected array length ${length}, got ${value.length}`);
    }
    return vec.enc(value);
  }, vec[1]);
}

// scale-ts has no float codecs — little-endian IEEE-754. Uses scale-ts's
// internal `.v` (DataView) + `.i` (cursor) shape; cast around the missing
// types.
type InternalBuffer = Uint8Array & { v: DataView; i: number };
function floatCodec(byteLength: 4 | 8): Codec<number> {
  return createCodec(
    (value) => {
      const out = new Uint8Array(byteLength);
      const view = new DataView(out.buffer);
      if (byteLength === 4) view.setFloat32(0, value, true);
      else view.setFloat64(0, value, true);
      return out;
    },
    createDecoder((raw) => {
      const input = raw as InternalBuffer;
      const value =
        byteLength === 4
          ? input.v.getFloat32(input.i, true)
          : input.v.getFloat64(input.i, true);
      input.i += byteLength;
      return value;
    }),
  );
}

export const f32 = floatCodec(4);
export const f64 = floatCodec(8);

// Forward-reference wrapper for recursive generated types. scale-ts codecs
// are `[enc, dec]` tuples with matching `.enc`/`.dec` properties, so a lazy
// wrapper just forwards both.
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
