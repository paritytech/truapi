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
} from 'scale-ts';

export type { Codec };
export type { ResultPayload } from 'scale-ts';

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
    if (typeof decoded === 'bigint') {
      throw new Error('compact big-int mode not supported');
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
  return createCodec(
    (value: Uint8Array) => {
      if (value.length !== length) {
        throw new Error(`Expected byte array length ${length}, got ${value.length}`);
      }
      return inner.enc(value);
    },
    inner[1],
  );
}

/**
 * Creates a fixed-length array codec that validates the element count before
 * delegating to `scale-ts`.
 */
export function array<T>(inner: Codec<T>, length: number): Codec<T[]> {
  const vec = Vector(inner, length);
  return createCodec(
    (value: T[]) => {
      if (value.length !== length) {
        throw new Error(`Expected array length ${length}, got ${value.length}`);
      }
      return vec.enc(value);
    },
    vec[1],
  );
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
        byteLength === 4 ? input.v.getFloat32(input.i, true) : input.v.getFloat64(input.i, true);
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
