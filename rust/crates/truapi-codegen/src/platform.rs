//! Parse `truapi-platform`-style "plain capability traits" from rustdoc JSON.
//!
//! Unlike the `truapi` API crate, the platform crate has no `#[wire(id = N)]`
//! annotations: it is a set of host-facing capability traits whose methods
//! use `async_trait` (rustdoc exposes those as boxed `Future` trait objects) or
//! plain synchronous functions returning trait objects / `BoxStream`. This
//! module walks the rustdoc index for every public trait in the platform crate
//! and produces a [`PlatformDefinition`] the TS emitter can render directly.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context, Result, bail};

use crate::rustdoc::{
    Crate, Item, NameContext, TypeDef, TypeDefKind, TypeRef, VariantFields, clean_docs,
    extract_enum, extract_struct, resolve_type, summarize_json,
};

/// Top-level extracted shape of a `truapi-platform`-style crate.
#[derive(Debug, PartialEq, Eq)]
pub struct PlatformDefinition {
    /// Capability traits sorted alphabetically by name.
    pub traits: Vec<PlatformTrait>,
    /// Local structs and enums referenced from trait method signatures,
    /// sorted alphabetically by name. Emitted alongside the trait interfaces
    /// so the generated TS does not have to import them from the API client.
    pub types: Vec<TypeDef>,
    /// Composite super-trait (`Platform: Storage + Navigation + ...`), if any.
    pub super_trait: Option<PlatformSuperTrait>,
}

/// Single capability trait extracted from the platform crate.
#[derive(Debug, PartialEq, Eq)]
pub struct PlatformTrait {
    /// Trait name as it appears in source.
    pub name: String,
    /// Rustdoc comment on the trait.
    pub docs: Option<String>,
    /// Methods declared on the trait, in declaration order.
    pub methods: Vec<PlatformMethod>,
}

/// A trait method on a capability trait.
#[derive(Debug, PartialEq, Eq)]
pub struct PlatformMethod {
    /// Method name as it appears in source.
    pub name: String,
    /// Rustdoc comment on the method.
    pub docs: Option<String>,
    /// Parameter list with names preserved (excluding `&self`).
    pub params: Vec<PlatformParam>,
    /// Return shape decoded from the method signature.
    pub return_shape: PlatformReturn,
    /// Whether the trait provides a default body, making the method optional
    /// for host implementations.
    pub has_default: bool,
}

/// Method parameter (name + type).
#[derive(Debug, PartialEq, Eq)]
pub struct PlatformParam {
    /// Parameter name as written in the trait method signature.
    pub name: String,
    /// Parameter type expressed as a [`TypeRef`].
    pub type_ref: TypeRef,
}

/// Return shape after stripping async-trait `Pin<Box<dyn Future<Output = T>>>`
/// / `Box<dyn _>` wrappers.
#[derive(Debug, PartialEq, Eq)]
pub struct PlatformReturn {
    /// Whether the method returns an async-trait boxed future (i.e. is async).
    pub is_async: bool,
    /// Unwrapped inner shape.
    pub inner: PlatformInner,
}

/// Classification of the unwrapped return type.
#[derive(Debug, PartialEq, Eq)]
pub enum PlatformInner {
    /// `()` (or no return).
    Unit,
    /// `Result<Ok, Err>`. The TS surface returns `Promise<Ok>` and rejects with `Err`.
    Result { ok: TypeRef, err: TypeRef },
    /// `BoxStream<'static, T>`, a stream of `T` items.
    Stream(TypeRef),
    /// `Box<dyn TraitName>`, a trait object handle to a named trait.
    TraitObject(String),
    /// Any other concrete type, returned as-is.
    Plain(TypeRef),
}

/// Composite super-trait that aggregates capability traits.
#[derive(Debug, PartialEq, Eq)]
pub struct PlatformSuperTrait {
    /// Name of the super-trait (e.g. `Platform`).
    pub name: String,
    /// Rustdoc comment on the super-trait.
    pub docs: Option<String>,
    /// Capability trait names this super-trait composes, in source order.
    pub composes: Vec<String>,
}

/// Walk the platform crate and extract every public trait + its methods.
pub fn extract(krate: &Crate) -> Result<PlatformDefinition> {
    let trait_ids = collect_local_trait_ids(krate);
    let names = NameContext::default();

    let mut traits = Vec::new();
    let mut super_trait = None;
    for item_id in &trait_ids {
        let item = krate
            .index
            .get(item_id)
            .with_context(|| format!("Missing rustdoc item `{item_id}` for trait"))?;
        let name = item
            .name
            .as_ref()
            .cloned()
            .with_context(|| format!("Trait `{item_id}` has no name"))?;
        let trait_inner = item
            .inner
            .get("trait")
            .with_context(|| format!("Trait `{name}` missing rustdoc trait body"))?;

        if is_super_trait(trait_inner) {
            if super_trait.is_some() {
                bail!("Multiple super-traits with method-less bodies found; only one is supported");
            }
            super_trait = Some(extract_super_trait(&name, item, trait_inner)?);
            continue;
        }

        traits.push(extract_capability_trait(
            &name,
            item,
            trait_inner,
            krate,
            &names,
        )?);
    }

    traits.sort_by(|a, b| a.name.cmp(&b.name));
    let types = collect_referenced_local_types(krate, &traits, &names)?;

    Ok(PlatformDefinition {
        traits,
        types,
        super_trait,
    })
}

/// Extract every local struct or enum whose name appears in a trait method
/// signature.
fn collect_referenced_local_types(
    krate: &Crate,
    traits: &[PlatformTrait],
    names: &NameContext,
) -> Result<Vec<TypeDef>> {
    let mut referenced = BTreeSet::new();
    for trait_def in traits {
        for method in &trait_def.methods {
            for param in &method.params {
                collect_named_types(&param.type_ref, &mut referenced);
            }
            match &method.return_shape.inner {
                // Err types never reach the TS signature (errors throw), so
                // their names are not emitted either.
                PlatformInner::Result { ok, .. } => collect_named_types(ok, &mut referenced),
                PlatformInner::Stream(inner) | PlatformInner::Plain(inner) => {
                    collect_named_types(inner, &mut referenced)
                }
                PlatformInner::TraitObject(_) | PlatformInner::Unit => {}
            }
        }
    }

    let mut local_type_candidates = BTreeMap::new();
    for (item_id, item_path) in &krate.paths {
        if item_path.crate_id != 0 || !matches!(item_path.kind.as_str(), "struct" | "enum") {
            continue;
        }
        let Some(name) = item_path.path.last() else {
            continue;
        };
        local_type_candidates
            .entry(name.clone())
            .or_insert_with(Vec::new)
            .push((item_id, item_path));
    }
    for candidates in local_type_candidates.values_mut() {
        candidates.sort_by(|(left_id, left_path), (right_id, right_path)| {
            left_path
                .path
                .cmp(&right_path.path)
                .then_with(|| left_id.cmp(right_id))
        });
    }

    // Local types can reference further local types from their fields or
    // variant payloads (e.g. `AuthState::Connected(SessionUiInfo)`), so keep
    // extracting until the referenced set stops growing.
    let mut types: Vec<TypeDef> = Vec::new();
    let mut extracted: BTreeSet<String> = BTreeSet::new();
    loop {
        let mut grew = false;
        let pending = referenced
            .iter()
            .filter(|name| !extracted.contains(*name))
            .cloned()
            .collect::<Vec<_>>();
        for name in pending {
            let Some(candidates) = local_type_candidates.get(&name) else {
                continue;
            };
            if candidates.len() > 1 {
                let paths = candidates
                    .iter()
                    .map(|(_, item_path)| item_path.path.join("::"))
                    .collect::<Vec<_>>()
                    .join(", ");
                bail!("platform type name `{name}` is ambiguous: defined by {paths}");
            }
            let (item_id, item_path) = candidates[0];
            let item = krate.index.get(item_id).with_context(|| {
                format!(
                    "Missing rustdoc item `{item_id}` for {} `{name}`",
                    item_path.kind
                )
            })?;
            let module_path = item_path.path[..item_path.path.len() - 1].to_vec();
            let type_def = if item_path.kind == "struct" {
                extract_struct(item_id, item, krate, names, module_path)?
            } else {
                extract_enum(item_id, item, krate, names, module_path)?
            };
            collect_type_def_references(&type_def, &mut referenced);
            extracted.insert(name);
            types.push(type_def);
            grew = true;
        }
        if !grew {
            break;
        }
    }
    types.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(types)
}

/// Collect named types referenced from a local type's fields or variants.
fn collect_type_def_references(type_def: &TypeDef, out: &mut BTreeSet<String>) {
    match &type_def.kind {
        TypeDefKind::Alias(ty) => collect_named_types(ty, out),
        TypeDefKind::Struct(fields) => {
            for field in fields {
                collect_named_types(&field.type_ref, out);
            }
        }
        TypeDefKind::TupleStruct(types) => {
            for ty in types {
                collect_named_types(ty, out);
            }
        }
        TypeDefKind::Enum(variants) => {
            for variant in variants {
                match &variant.fields {
                    VariantFields::Unit => {}
                    VariantFields::Unnamed(types) => {
                        for ty in types {
                            collect_named_types(ty, out);
                        }
                    }
                    VariantFields::Named(fields) => {
                        for field in fields {
                            collect_named_types(&field.type_ref, out);
                        }
                    }
                }
            }
        }
    }
}

fn collect_named_types(ty: &TypeRef, out: &mut BTreeSet<String>) {
    match ty {
        TypeRef::Named { name, args } => {
            out.insert(name.clone());
            for arg in args {
                collect_named_types(arg, out);
            }
        }
        TypeRef::Vec(inner) | TypeRef::Option(inner) | TypeRef::Array(inner, _) => {
            collect_named_types(inner, out)
        }
        TypeRef::Tuple(items) => {
            for item in items {
                collect_named_types(item, out);
            }
        }
        TypeRef::Primitive(_) | TypeRef::Generic(_) | TypeRef::Unit => {}
    }
}

fn collect_local_trait_ids(krate: &Crate) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for (item_id, item_path) in &krate.paths {
        if item_path.crate_id != 0 || item_path.kind != "trait" {
            continue;
        }
        out.insert(item_id.clone());
    }
    out
}

fn is_super_trait(trait_inner: &serde_json::Value) -> bool {
    let no_methods = trait_inner
        .get("items")
        .and_then(|value| value.as_array())
        .map(|arr| arr.is_empty())
        .unwrap_or(true);

    let has_local_trait_bound = trait_inner
        .get("bounds")
        .and_then(|value| value.as_array())
        .map(|bounds| {
            bounds.iter().any(|bound| {
                bound
                    .get("trait_bound")
                    .and_then(|tb| tb.get("trait"))
                    .and_then(|t| t.get("path"))
                    .and_then(|p| p.as_str())
                    .map(|name| name != "Send" && name != "Sync")
                    .unwrap_or(false)
            })
        })
        .unwrap_or(false);

    no_methods && has_local_trait_bound
}

fn extract_super_trait(
    name: &str,
    item: &Item,
    trait_inner: &serde_json::Value,
) -> Result<PlatformSuperTrait> {
    let bounds = trait_inner
        .get("bounds")
        .and_then(|value| value.as_array())
        .with_context(|| format!("Super-trait `{name}` missing rustdoc bounds"))?;

    let mut composes = Vec::new();
    for bound in bounds {
        let Some(path) = bound
            .get("trait_bound")
            .and_then(|tb| tb.get("trait"))
            .and_then(|t| t.get("path"))
            .and_then(|p| p.as_str())
        else {
            continue;
        };
        if path == "Send" || path == "Sync" {
            continue;
        }
        composes.push(path.to_string());
    }

    Ok(PlatformSuperTrait {
        name: name.to_string(),
        docs: clean_docs(item.docs.as_deref()),
        composes,
    })
}

fn extract_capability_trait(
    name: &str,
    item: &Item,
    trait_inner: &serde_json::Value,
    krate: &Crate,
    names: &NameContext,
) -> Result<PlatformTrait> {
    let item_ids = trait_inner
        .get("items")
        .and_then(|value| value.as_array())
        .with_context(|| format!("Trait `{name}` missing rustdoc items array"))?;

    let mut methods = Vec::new();
    for method_id in item_ids {
        let method_id = value_to_id(method_id)
            .with_context(|| format!("Trait `{name}` contained a non-item method id"))?;
        let method_item = krate
            .index
            .get(&method_id)
            .with_context(|| format!("Trait `{name}` references missing item `{method_id}`"))?;
        if let Some(method) = extract_method(method_item, names)? {
            methods.push(method);
        }
    }

    Ok(PlatformTrait {
        name: name.to_string(),
        docs: clean_docs(item.docs.as_deref()),
        methods,
    })
}

fn extract_method(item: &Item, names: &NameContext) -> Result<Option<PlatformMethod>> {
    let Some(fn_inner) = item.inner.get("function") else {
        return Ok(None);
    };
    let name = item
        .name
        .as_ref()
        .cloned()
        .with_context(|| "Method item has no name".to_string())?;
    let sig = fn_inner
        .get("sig")
        .with_context(|| format!("Method `{name}` missing rustdoc signature"))?;

    let mut params = Vec::new();
    if let Some(inputs) = sig.get("inputs").and_then(|value| value.as_array()) {
        for input in inputs {
            let arr = input
                .as_array()
                .with_context(|| format!("Method `{name}` has an invalid input entry"))?;
            let param_name = arr
                .first()
                .and_then(|value| value.as_str())
                .with_context(|| format!("Method `{name}` has an unnamed input"))?
                .to_string();
            if param_name == "self" {
                continue;
            }
            let ty = arr.get(1).with_context(|| {
                format!("Method `{name}` input `{param_name}` is missing a type")
            })?;
            let type_ref = resolve_type(ty, names).with_context(|| {
                format!("Method `{name}` input `{param_name}` has an unsupported type")
            })?;
            params.push(PlatformParam {
                name: param_name,
                type_ref,
            });
        }
    }

    let return_shape = resolve_return(sig.get("output"), names)
        .with_context(|| format!("Method `{name}` has an unsupported return type"))?;
    let has_default = fn_inner
        .get("has_body")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);

    Ok(Some(PlatformMethod {
        name,
        docs: clean_docs(item.docs.as_deref()),
        params,
        return_shape,
        has_default,
    }))
}

fn resolve_return(
    output: Option<&serde_json::Value>,
    names: &NameContext,
) -> Result<PlatformReturn> {
    let Some(output) = output else {
        return Ok(PlatformReturn {
            is_async: false,
            inner: PlatformInner::Unit,
        });
    };
    if output.is_null() {
        return Ok(PlatformReturn {
            is_async: false,
            inner: PlatformInner::Unit,
        });
    }

    if let Some(future_output) = extract_async_trait_future_output(output) {
        let inner = resolve_inner_shape(&future_output, names)?;
        return Ok(PlatformReturn {
            is_async: true,
            inner,
        });
    }

    let inner = resolve_inner_shape(output, names)?;
    Ok(PlatformReturn {
        is_async: false,
        inner,
    })
}

fn extract_async_trait_future_output(output: &serde_json::Value) -> Option<serde_json::Value> {
    let pin = output.get("resolved_path")?;
    if resolved_leaf(pin) != Some("Pin") {
        return None;
    }
    let boxed = generic_arg(pin, 0)?;
    let boxed = boxed.get("resolved_path")?;
    if resolved_leaf(boxed) != Some("Box") {
        return None;
    }
    let dyn_trait = generic_arg(boxed, 0)?;
    let dyn_trait = dyn_trait.get("dyn_trait")?;
    let traits = dyn_trait.get("traits")?.as_array()?;
    for trait_entry in traits {
        let trait_obj = trait_entry.get("trait")?;
        if resolved_leaf(trait_obj) != Some("Future") {
            continue;
        }
        let constraints = trait_obj
            .get("args")?
            .get("angle_bracketed")?
            .get("constraints")?
            .as_array()?;
        for constraint in constraints {
            if constraint.get("name")?.as_str()? != "Output" {
                continue;
            }
            let ty = constraint.get("binding")?.get("equality")?.get("type")?;
            return Some(ty.clone());
        }
    }
    None
}

fn resolve_inner_shape(ty: &serde_json::Value, names: &NameContext) -> Result<PlatformInner> {
    // `()` tuple.
    if let Some(arr) = ty.get("tuple").and_then(|v| v.as_array())
        && arr.is_empty()
    {
        return Ok(PlatformInner::Unit);
    }

    if let Some(resolved) = ty.get("resolved_path") {
        let path = resolved
            .get("path")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let leaf = path.rsplit("::").next().unwrap_or(path);

        match leaf {
            "Result" => {
                let ok =
                    generic_arg(resolved, 0).context("Result<...> return type missing ok arg")?;
                let err =
                    generic_arg(resolved, 1).context("Result<...> return type missing err arg")?;
                let ok_ref = resolve_inner_type(&ok, names)?;
                let err_ref = resolve_inner_type(&err, names)?;
                return Ok(PlatformInner::Result {
                    ok: ok_ref,
                    err: err_ref,
                });
            }
            "BoxStream" => {
                // `BoxStream<'a, T>`: the lifetime arg is filtered out by
                // `generic_arg` (it has no `type` field), so the first
                // remaining positional arg is the item type.
                let item =
                    generic_arg(resolved, 0).context("BoxStream<'a, T> missing item type")?;
                return Ok(PlatformInner::Stream(resolve_type(&item, names)?));
            }
            "Box" => {
                // `Box<dyn TraitName>` or `Box<T>`.
                if let Some(arg) = generic_arg(resolved, 0)
                    && let Some(dyn_trait) = arg.get("dyn_trait")
                {
                    return Ok(PlatformInner::TraitObject(dyn_trait_leaf_name(dyn_trait)?));
                }
            }
            _ => {}
        }
    }

    let resolved_ref = resolve_type(ty, names)
        .with_context(|| format!("Unsupported return shape: {}", summarize_json(ty)))?;
    Ok(PlatformInner::Plain(resolved_ref))
}

/// Resolve a positional type. Recognises `Box<dyn TraitName>` and folds it
/// into a `TypeRef::Named { name: TraitName, args: [] }` so it survives
/// through to TS emission without `rustdoc.rs` having to model dyn traits.
fn resolve_inner_type(ty: &serde_json::Value, names: &NameContext) -> Result<TypeRef> {
    if let Some(resolved) = ty.get("resolved_path")
        && resolved
            .get("path")
            .and_then(|v| v.as_str())
            .map(|p| p.rsplit("::").next().unwrap_or(p) == "Box")
            .unwrap_or(false)
        && let Some(arg) = generic_arg(resolved, 0)
        && let Some(dyn_trait) = arg.get("dyn_trait")
    {
        return Ok(TypeRef::Named {
            name: dyn_trait_leaf_name(dyn_trait)?,
            args: Vec::new(),
        });
    }
    resolve_type(ty, names)
}

/// Extract the leaf trait name from a `Box<dyn Trait>` rustdoc `dyn_trait`
/// value (the last `::`-segment of the first listed trait path).
fn dyn_trait_leaf_name(dyn_trait: &serde_json::Value) -> Result<String> {
    Ok(dyn_trait
        .get("traits")
        .and_then(|t| t.as_array())
        .and_then(|arr| arr.first())
        .and_then(|first| first.get("trait"))
        .and_then(|trait_obj| trait_obj.get("path"))
        .and_then(|p| p.as_str())
        .context("Box<dyn Trait> missing trait path")?
        .rsplit("::")
        .next()
        .unwrap_or_default()
        .to_string())
}

fn resolved_leaf(resolved: &serde_json::Value) -> Option<&str> {
    let path = resolved.get("path")?.as_str()?;
    Some(path.rsplit("::").next().unwrap_or(path))
}

fn generic_arg(resolved: &serde_json::Value, index: usize) -> Option<serde_json::Value> {
    resolved
        .get("args")?
        .get("angle_bracketed")?
        .get("args")?
        .as_array()?
        .iter()
        .filter_map(|entry| entry.get("type").cloned())
        .nth(index)
}

fn value_to_id(value: &serde_json::Value) -> Result<String> {
    if let Some(id) = value.as_str() {
        return Ok(id.to_string());
    }
    if let Some(id) = value.as_u64() {
        return Ok(id.to_string());
    }
    bail!("Expected rustdoc item id, got non-id value")
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use serde_json::json;

    use crate::rustdoc::ItemPath;

    use super::*;

    #[test]
    fn extract_async_trait_future_output_from_pin_box_dyn_future() {
        let output = json!({
            "resolved_path": {
                "path": "::core::pin::Pin",
                "args": {
                    "angle_bracketed": {
                        "args": [
                            {
                                "type": {
                                    "resolved_path": {
                                        "path": "Box",
                                        "args": {
                                            "angle_bracketed": {
                                                "args": [
                                                    {
                                                        "type": {
                                                            "dyn_trait": {
                                                                "traits": [
                                                                    {
                                                                        "trait": {
                                                                            "path": "::core::future::Future",
                                                                            "args": {
                                                                                "angle_bracketed": {
                                                                                    "args": [],
                                                                                    "constraints": [
                                                                                        {
                                                                                            "name": "Output",
                                                                                            "binding": {
                                                                                                "equality": {
                                                                                                    "type": { "primitive": "u8" }
                                                                                                }
                                                                                            }
                                                                                        }
                                                                                    ]
                                                                                }
                                                                            }
                                                                        }
                                                                    },
                                                                    {
                                                                        "trait": {
                                                                            "path": "::core::marker::Send",
                                                                            "args": null
                                                                        }
                                                                    }
                                                                ],
                                                                "lifetime": "'async_trait"
                                                            }
                                                        }
                                                    }
                                                ],
                                                "constraints": []
                                            }
                                        }
                                    }
                                }
                            }
                        ],
                        "constraints": []
                    }
                }
            }
        });

        assert_eq!(
            extract_async_trait_future_output(&output),
            Some(json!({ "primitive": "u8" }))
        );
    }

    #[test]
    fn referenced_platform_type_names_must_be_unambiguous() {
        let krate = Crate {
            format_version: Some(57),
            index: HashMap::new(),
            paths: HashMap::from([
                (
                    "1".to_string(),
                    ItemPath {
                        crate_id: 0,
                        path: vec![
                            "truapi_platform".to_string(),
                            "one".to_string(),
                            "Shared".to_string(),
                        ],
                        kind: "struct".to_string(),
                    },
                ),
                (
                    "2".to_string(),
                    ItemPath {
                        crate_id: 0,
                        path: vec![
                            "truapi_platform".to_string(),
                            "two".to_string(),
                            "Shared".to_string(),
                        ],
                        kind: "enum".to_string(),
                    },
                ),
            ]),
        };
        let traits = [PlatformTrait {
            name: "Storage".to_string(),
            docs: None,
            methods: vec![PlatformMethod {
                name: "write".to_string(),
                docs: None,
                params: vec![PlatformParam {
                    name: "value".to_string(),
                    type_ref: TypeRef::Named {
                        name: "Shared".to_string(),
                        args: Vec::new(),
                    },
                }],
                return_shape: PlatformReturn {
                    is_async: false,
                    inner: PlatformInner::Unit,
                },
                has_default: false,
            }],
        }];

        let err =
            collect_referenced_local_types(&krate, &traits, &NameContext::default()).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("platform type name `Shared` is ambiguous"),
            "unexpected error: {msg}"
        );
        assert!(
            msg.contains("truapi_platform::one::Shared")
                && msg.contains("truapi_platform::two::Shared"),
            "unexpected error: {msg}"
        );
    }
}
