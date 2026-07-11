//! Minimal metadata-driven SCALE walker.
//!
//! Just enough to read one field out of a storage struct without a full dynamic
//! codec: `skip` advances a cursor past one value of a given type, and
//! `read_field_variant_name` walks a composite to a named field and returns its
//! enum variant name (used for `CollectionInfo.ring_size` -> `R2e9`/`R2e10`/`R2e14`).

use parity_scale_codec::{Compact, Decode};
use scale_info::{PortableRegistry, TypeDef, TypeDefPrimitive};

/// Advance `input` past exactly one SCALE-encoded value of `type_id`.
pub fn skip(registry: &PortableRegistry, type_id: u32, input: &mut &[u8]) -> Result<(), String> {
    let ty = registry
        .resolve(type_id)
        .ok_or_else(|| format!("unknown type id {type_id}"))?;
    match &ty.type_def {
        TypeDef::Composite(c) => {
            for field in &c.fields {
                skip(registry, field.ty.id, input)?;
            }
        }
        TypeDef::Tuple(t) => {
            for field in &t.fields {
                skip(registry, field.id, input)?;
            }
        }
        TypeDef::Array(a) => {
            for _ in 0..a.len {
                skip(registry, a.type_param.id, input)?;
            }
        }
        TypeDef::Sequence(s) => {
            let len = read_compact(input)?;
            for _ in 0..len {
                skip(registry, s.type_param.id, input)?;
            }
        }
        TypeDef::Variant(v) => {
            let index = read_u8(input)?;
            let variant = v
                .variants
                .iter()
                .find(|var| var.index == index)
                .ok_or_else(|| format!("unknown variant index {index}"))?;
            for field in &variant.fields {
                skip(registry, field.ty.id, input)?;
            }
        }
        TypeDef::Compact(_) => {
            read_compact(input)?;
        }
        TypeDef::BitSequence(_) => {
            let bits = read_compact(input)?;
            advance(input, bits.div_ceil(8))?;
        }
        TypeDef::Primitive(p) => {
            let len = match p {
                TypeDefPrimitive::Bool | TypeDefPrimitive::U8 | TypeDefPrimitive::I8 => 1,
                TypeDefPrimitive::U16 | TypeDefPrimitive::I16 => 2,
                TypeDefPrimitive::Char | TypeDefPrimitive::U32 | TypeDefPrimitive::I32 => 4,
                TypeDefPrimitive::U64 | TypeDefPrimitive::I64 => 8,
                TypeDefPrimitive::U128 | TypeDefPrimitive::I128 => 16,
                TypeDefPrimitive::U256 | TypeDefPrimitive::I256 => 32,
                // Length-prefixed UTF-8: compact byte length then the bytes.
                TypeDefPrimitive::Str => read_compact(input)?,
            };
            advance(input, len)?;
        }
    }
    Ok(())
}

/// Walk composite `struct_type_id` to `field_name` and return the enum variant
/// name selected there (the field must be a fieldless/simple enum).
pub fn read_field_variant_name(
    registry: &PortableRegistry,
    struct_type_id: u32,
    field_name: &str,
    bytes: &[u8],
) -> Result<String, String> {
    let ty = registry
        .resolve(struct_type_id)
        .ok_or_else(|| format!("unknown type id {struct_type_id}"))?;
    let TypeDef::Composite(composite) = &ty.type_def else {
        return Err(format!("type {struct_type_id} is not a composite"));
    };

    let mut input = bytes;
    for field in &composite.fields {
        if field.name.as_deref() == Some(field_name) {
            let field_ty = registry
                .resolve(field.ty.id)
                .ok_or_else(|| format!("unknown field type id {}", field.ty.id))?;
            let TypeDef::Variant(variant) = &field_ty.type_def else {
                return Err(format!("field `{field_name}` is not an enum"));
            };
            let index = read_u8(&mut input)?;
            return variant
                .variants
                .iter()
                .find(|var| var.index == index)
                .map(|var| var.name.clone())
                .ok_or_else(|| format!("unknown variant index {index} for `{field_name}`"));
        }
        skip(registry, field.ty.id, &mut input)?;
    }
    Err(format!("field `{field_name}` not found"))
}

/// Decode a SCALE compact-encoded length, advancing `input`.
fn read_compact(input: &mut &[u8]) -> Result<usize, String> {
    let Compact(value) = Compact::<u128>::decode(input).map_err(|err| format!("compact: {err}"))?;
    usize::try_from(value).map_err(|_| "compact length overflow".to_string())
}

/// Read one byte, advancing `input`.
fn read_u8(input: &mut &[u8]) -> Result<u8, String> {
    let (&first, rest) = input
        .split_first()
        .ok_or_else(|| "unexpected end".to_string())?;
    *input = rest;
    Ok(first)
}

/// Advance `input` by `n` bytes.
fn advance(input: &mut &[u8], n: usize) -> Result<(), String> {
    if input.len() < n {
        return Err(format!("need {n} bytes, have {}", input.len()));
    }
    *input = &input[n..];
    Ok(())
}
