//! Parse `truapi-platform`-style "plain capability traits" from rustdoc JSON.
//!
//! Unlike the `truapi` API crate, the platform crate has no `#[wire(id = N)]`
//! annotations: it is a set of host-facing capability traits whose methods
//! are typed `async fn`-style (`impl Future<Output = T> + Send`) or plain
//! synchronous functions returning trait objects / `BoxStream`. This module
//! walks the rustdoc index for every public trait in the platform crate and
//! produces a [`PlatformDefinition`] the TS emitter can render directly.

use std::collections::BTreeMap;

use anyhow::{Context, Result, bail};

use crate::rustdoc::{
    Crate, Item, ItemPath, NameContext, TypeRef, clean_docs, resolve_type, summarize_json,
};

/// Top-level extracted shape of a `truapi-platform`-style crate.
#[derive(Debug, PartialEq, Eq)]
pub struct PlatformDefinition {
    /// Capability traits in source declaration order.
    pub traits: Vec<PlatformTrait>,
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
}

/// Method parameter (name + type).
#[derive(Debug, PartialEq, Eq)]
pub struct PlatformParam {
    /// Parameter name as written in the trait method signature.
    pub name: String,
    /// Parameter type expressed as a [`TypeRef`].
    pub type_ref: TypeRef,
}

/// Return shape after stripping `impl Future + Send` / `Box<dyn _>` wrappers.
#[derive(Debug, PartialEq, Eq)]
pub struct PlatformReturn {
    /// Whether the method returns `impl Future<...> + Send` (i.e. is async).
    pub is_async: bool,
    /// Unwrapped inner shape.
    pub inner: PlatformInner,
}

/// Classification of the unwrapped return type.
#[derive(Debug, PartialEq, Eq)]
pub enum PlatformInner {
    /// `()` (or no return).
    Unit,
    /// `Result<Ok, Err>` — the TS surface returns `Promise<Ok>` and rejects with `Err`.
    Result { ok: TypeRef, err: TypeRef },
    /// `BoxStream<'static, T>` — a stream of `T` items.
    Stream(TypeRef),
    /// `Box<dyn TraitName>` — a trait object handle to a named trait.
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
    let trait_paths = collect_local_trait_paths(krate);
    let names = NameContext::default();

    let mut traits = Vec::new();
    let mut super_trait = None;
    for (item_id, item_path) in &trait_paths {
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

        // Touch item_path so a future use can rely on the same iteration order.
        let _ = item_path;
    }

    traits.sort_by(|a, b| a.name.cmp(&b.name));

    Ok(PlatformDefinition {
        traits,
        super_trait,
    })
}

fn collect_local_trait_paths(krate: &Crate) -> BTreeMap<String, &ItemPath> {
    let mut out = BTreeMap::new();
    for (item_id, item_path) in &krate.paths {
        if item_path.crate_id != 0 || item_path.kind != "trait" {
            continue;
        }
        out.insert(item_id.clone(), item_path);
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

    Ok(Some(PlatformMethod {
        name,
        docs: clean_docs(item.docs.as_deref()),
        params,
        return_shape,
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

    if let Some(future_output) = extract_impl_future_output(output) {
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

fn extract_impl_future_output(output: &serde_json::Value) -> Option<serde_json::Value> {
    let bounds = output.get("impl_trait")?.as_array()?;
    for bound in bounds {
        let trait_bound = bound.get("trait_bound")?;
        let trait_obj = trait_bound.get("trait")?;
        let path = trait_obj.get("path")?.as_str()?;
        if path != "Future" {
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
                    let trait_name = dyn_trait
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
                        .to_string();
                    return Ok(PlatformInner::TraitObject(trait_name));
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
        let trait_name = dyn_trait
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
            .to_string();
        return Ok(TypeRef::Named {
            name: trait_name,
            args: Vec::new(),
        });
    }
    resolve_type(ty, names)
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
