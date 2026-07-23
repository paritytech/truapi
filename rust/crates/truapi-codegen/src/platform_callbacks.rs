//! Shared callback naming and selection rules for generated platform bridges.

use std::collections::BTreeSet;

use crate::platform::{PlatformDefinition, PlatformInner, PlatformMethod, PlatformTrait};
use crate::rustdoc::TypeRef;

/// Traits the platform surface actually composes: the super trait's
/// constituents when one exists, otherwise every collected trait.
pub(crate) fn composed_traits(definition: &PlatformDefinition) -> Vec<&PlatformTrait> {
    let composed: BTreeSet<String> = match &definition.super_trait {
        Some(s) => s.composes.iter().cloned().collect(),
        None => definition.traits.iter().map(|t| t.name.clone()).collect(),
    };
    definition
        .traits
        .iter()
        .filter(|t| composed.contains(&t.name))
        .collect()
}

/// JS-side callback name for a platform method (camelCase of the Rust name).
pub(crate) fn raw_callback_name(method: &PlatformMethod) -> String {
    to_camel_case(&method.name)
}

/// Set of all platform trait names, used to recognize trait-object returns.
pub(crate) fn platform_trait_names(definition: &PlatformDefinition) -> BTreeSet<String> {
    definition.traits.iter().map(|t| t.name.clone()).collect()
}

/// Name of the platform trait a method returns as a handle, if any.
pub(crate) fn trait_object_return_name<'a>(
    method: &'a PlatformMethod,
    platform_trait_names: &BTreeSet<String>,
) -> Option<&'a str> {
    match &method.return_shape.inner {
        PlatformInner::TraitObject(name) => Some(name.as_str()),
        PlatformInner::Result { ok, .. } | PlatformInner::Plain(ok) => {
            named_platform_trait(ok, platform_trait_names)
        }
        PlatformInner::Unit | PlatformInner::Stream(_) => None,
    }
}

/// Wire name of a raw callback. Handle-returning methods get a trait
/// namespace prefix so equally named methods on different traits stay
/// distinct.
pub(crate) fn raw_callback_wire_name(
    trait_def: &PlatformTrait,
    method: &PlatformMethod,
    platform_trait_names: &BTreeSet<String>,
) -> String {
    let raw = raw_callback_name(method);
    if trait_object_return_name(method, platform_trait_names).is_some() {
        return format!(
            "{}{}",
            callback_namespace(&trait_def.name),
            upper_first(&raw)
        );
    }
    raw
}

/// Field name holding the callback in the generated Rust bridge struct.
pub(crate) fn raw_callback_field_name(
    trait_def: &PlatformTrait,
    method: &PlatformMethod,
    platform_trait_names: &BTreeSet<String>,
) -> String {
    snake_case(&raw_callback_wire_name(
        trait_def,
        method,
        platform_trait_names,
    ))
}

/// TS type name for the raw callback in the generated host-callback bridge.
pub(crate) fn raw_callback_type_name(
    trait_def: &PlatformTrait,
    method: &PlatformMethod,
    platform_trait_names: &BTreeSet<String>,
) -> String {
    upper_first(&raw_callback_wire_name(
        trait_def,
        method,
        platform_trait_names,
    ))
}

/// Name of the TS adapter that wraps a typed host callback into its raw form.
pub(crate) fn raw_callback_adapter_name(
    trait_def: &PlatformTrait,
    method: &PlatformMethod,
    platform_trait_names: &BTreeSet<String>,
) -> String {
    format!(
        "{}Adapter",
        raw_callback_wire_name(trait_def, method, platform_trait_names)
    )
}

/// Callback-object namespace for a trait: its name with the role suffix
/// (`Provider`, `Presenter`, `Host`) stripped, lower-cased first letter.
pub(crate) fn callback_namespace(trait_name: &str) -> String {
    let stem = ["Provider", "Presenter", "Host"]
        .into_iter()
        .find_map(|suffix| trait_name.strip_suffix(suffix))
        .unwrap_or(trait_name);
    lower_pascal_case(stem)
}

fn named_platform_trait<'a>(
    ty: &'a TypeRef,
    platform_trait_names: &BTreeSet<String>,
) -> Option<&'a str> {
    let TypeRef::Named { name, args } = ty else {
        return None;
    };
    if args.is_empty() && platform_trait_names.contains(name) {
        return Some(name.as_str());
    }
    None
}

/// Unwrap a `Result<T, E>` stream item to its `T`; other item types pass
/// through. Streams carry `Result`s on the Rust side but the JS raw bridge
/// already unwraps them before handing each item to the WASM callback sink.
pub(crate) fn stream_item(item: &TypeRef) -> &TypeRef {
    if let TypeRef::Named { name, args } = item
        && name == "Result"
        && let Some(ok) = args.first()
    {
        return ok;
    }
    item
}

/// Convert a snake_case identifier to camelCase.
pub(crate) fn to_camel_case(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut upper_next = false;
    for (idx, ch) in name.chars().enumerate() {
        if ch == '_' {
            upper_next = idx != 0;
            continue;
        }
        if upper_next {
            out.extend(ch.to_uppercase());
            upper_next = false;
        } else {
            out.push(ch);
        }
    }
    out
}

fn lower_pascal_case(name: &str) -> String {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return String::new();
    };
    format!(
        "{}{}",
        first.to_ascii_lowercase(),
        chars.collect::<String>()
    )
}

fn upper_first(name: &str) -> String {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return String::new();
    };
    format!(
        "{}{}",
        first.to_ascii_uppercase(),
        chars.collect::<String>()
    )
}

/// Convert a camelCase identifier to snake_case.
pub(crate) fn snake_case(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 4);
    for (idx, ch) in name.chars().enumerate() {
        if ch.is_ascii_uppercase() {
            if idx != 0 {
                out.push('_');
            }
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}
