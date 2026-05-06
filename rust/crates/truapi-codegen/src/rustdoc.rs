//! Parse rustdoc JSON output to extract API definitions.

use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap};

use anyhow::{bail, Context, Result};
use serde::Deserialize;

/// Parsed rustdoc crate. IDs are integers but serialized as string keys in JSON maps.
#[derive(Debug, Deserialize)]
pub struct Crate {
    pub index: HashMap<String, Item>,
    #[serde(default)]
    pub paths: HashMap<String, ItemPath>,
}

#[derive(Debug, Deserialize)]
pub struct Item {
    pub name: Option<String>,
    #[allow(dead_code)]
    pub docs: Option<String>,
    pub inner: serde_json::Value,
}

#[derive(Debug, Deserialize)]
pub struct ItemPath {
    pub crate_id: u32,
    pub path: Vec<String>,
    pub kind: String,
}

/// Extracted API definition ready for code generation.
#[derive(Debug, PartialEq, Eq)]
pub struct ApiDefinition {
    pub traits: Vec<TraitDef>,
    pub types: Vec<TypeDef>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct TraitDef {
    pub name: String,
    pub methods: Vec<MethodDef>,
    /// Rustdoc comment on the trait, with hidden codegen markers stripped.
    pub docs: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct MethodDef {
    pub name: String,
    pub kind: MethodKind,
    pub params: Vec<ParamDef>,
    pub return_type: ReturnType,
    /// Base wire-protocol discriminant id for this method (from `#[wire(id = N)]`).
    /// `None` for trait methods that have not been annotated yet (legacy / opt-in).
    pub wire_id: Option<u8>,
    /// Rustdoc comment on the method, with hidden codegen markers stripped.
    pub docs: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum MethodKind {
    Request,
    Subscription,
    ResultSubscription,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ParamDef {
    pub name: String,
    pub type_ref: TypeRef,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ReturnType {
    Result { ok: TypeRef, err: TypeRef },
    Subscription(TypeRef),
    ResultSubscription { item: TypeRef, err: TypeRef },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeRef {
    Primitive(String),
    Named { name: String, args: Vec<TypeRef> },
    Vec(Box<TypeRef>),
    Option(Box<TypeRef>),
    Tuple(Vec<TypeRef>),
    Array(Box<TypeRef>, usize),
    Generic(String),
    Unit,
}

#[derive(Debug, PartialEq, Eq)]
pub struct TypeDef {
    pub name: String,
    pub generic_params: Vec<String>,
    pub kind: TypeDefKind,
    /// Rustdoc comment on the type itself.
    pub docs: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum TypeDefKind {
    Alias(TypeRef),
    Struct(Vec<FieldDef>),
    Enum(Vec<VariantDef>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldDef {
    pub name: String,
    pub type_ref: TypeRef,
    /// Rustdoc comment on the field.
    pub docs: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct VariantDef {
    pub name: String,
    pub fields: VariantFields,
    /// Rustdoc comment on the variant.
    pub docs: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum VariantFields {
    Unit,
    Unnamed(Vec<TypeRef>),
    Named(Vec<FieldDef>),
}

#[derive(Debug, Clone)]
struct ItemCandidate {
    item_id: String,
    path: Vec<String>,
    kind: String,
}

/// Parses rustdoc JSON output into the minimal crate model used by the code
/// generator.
pub fn parse(json: &str) -> Result<Crate> {
    serde_json::from_str(json).context("Failed to parse rustdoc JSON")
}

/// Extracts the public traits and types that make up the generated API surface
/// from a parsed rustdoc crate.
pub fn extract_api(krate: &Crate) -> Result<ApiDefinition> {
    let trait_candidates = collect_public_candidates(krate, &["trait"]);
    let type_candidates = collect_public_candidates(krate, &["struct", "enum", "type_alias"]);

    let mut traits = Vec::new();
    for (name, candidates) in trait_candidates {
        // `Versioned` is a runtime-helper trait on the wrapper enums, not a
        // protocol-method trait. The codegen only cares about the protocol
        // surface (TrUAPI methods); skip anything declared outside
        // `truapi::api::*`.
        let candidate = select_candidate(&name, &candidates)?;
        if !candidate.path.iter().any(|s| s == "api") {
            continue;
        }
        let item = krate
            .index
            .get(&candidate.item_id)
            .with_context(|| format!("Missing rustdoc item `{}`", candidate.item_id))?;
        traits.push(extract_trait(&candidate.item_id, item, krate)?);
    }

    let mut types = Vec::new();
    for (name, candidates) in type_candidates {
        if matches!(
            name.as_str(),
            "Subscription" | "CallContext" | "RuntimeFailure" | "RuntimeFailureKind"
        ) {
            continue;
        }

        let candidate = select_candidate(&name, &candidates)?;
        let item = krate
            .index
            .get(&candidate.item_id)
            .with_context(|| format!("Missing rustdoc item `{}`", candidate.item_id))?;

        let type_def = if candidate.kind == "struct" {
            extract_struct(&candidate.item_id, item, krate)?
        } else if candidate.kind == "enum" {
            extract_enum(&candidate.item_id, item, krate)?
        } else if candidate.kind == "type_alias" {
            extract_type_alias(&candidate.item_id, item)?
        } else {
            bail!(
                "Unsupported rustdoc item kind `{}` for `{}`",
                candidate.kind,
                candidate.path.join("::")
            );
        };

        types.push(type_def);
    }

    traits.sort_by(|a, b| a.name.cmp(&b.name));
    types.sort_by(|a, b| a.name.cmp(&b.name));

    Ok(ApiDefinition { traits, types })
}

fn collect_public_candidates(
    krate: &Crate,
    allowed_kinds: &[&str],
) -> BTreeMap<String, Vec<ItemCandidate>> {
    let mut grouped: BTreeMap<String, Vec<ItemCandidate>> = BTreeMap::new();

    for (item_id, item_path) in &krate.paths {
        if item_path.crate_id != 0 || !allowed_kinds.contains(&item_path.kind.as_str()) {
            continue;
        }

        let Some(name) = item_path.path.last() else {
            continue;
        };

        grouped
            .entry(name.clone())
            .or_default()
            .push(ItemCandidate {
                item_id: item_id.clone(),
                path: item_path.path.clone(),
                kind: item_path.kind.clone(),
            });
    }

    for candidates in grouped.values_mut() {
        candidates.sort_by(compare_candidates);
    }

    grouped
}

fn compare_candidates(a: &ItemCandidate, b: &ItemCandidate) -> Ordering {
    version_rank(&a.path)
        .cmp(&version_rank(&b.path))
        .then_with(|| a.path.cmp(&b.path))
        .then_with(|| a.item_id.cmp(&b.item_id))
}

fn version_rank(path: &[String]) -> u32 {
    // The unified contract lives under `api::` for sub-traits and
    // `versioned::` for request/response wrappers; both must outrank the
    // version-numbered `v0N` legacy modules.
    if path
        .iter()
        .any(|segment| segment == "api" || segment == "versioned")
    {
        return u32::MAX;
    }
    path.iter()
        .find_map(|segment| {
            segment
                .strip_prefix('v')
                .and_then(|value| value.parse::<u32>().ok())
        })
        .unwrap_or(0)
}

fn select_candidate<'a>(name: &str, candidates: &'a [ItemCandidate]) -> Result<&'a ItemCandidate> {
    let Some(selected) = candidates.last() else {
        bail!("No rustdoc candidates found for `{}`", name);
    };

    let selected_rank = version_rank(&selected.path);
    let ambiguous = candidates
        .iter()
        .rev()
        .skip(1)
        .take_while(|candidate| version_rank(&candidate.path) == selected_rank)
        .collect::<Vec<_>>();

    if !ambiguous.is_empty() {
        let mut paths = ambiguous
            .iter()
            .map(|candidate| candidate.path.join("::"))
            .collect::<Vec<_>>();
        paths.push(selected.path.join("::"));
        paths.sort();
        bail!(
            "Ambiguous rustdoc candidates for `{}` at version rank {}: {}",
            name,
            selected_rank,
            paths.join(", ")
        );
    }

    Ok(selected)
}

fn extract_trait(item_id: &str, item: &Item, krate: &Crate) -> Result<TraitDef> {
    let name = item
        .name
        .as_ref()
        .cloned()
        .with_context(|| format!("Trait item `{}` has no name", item_id))?;
    let trait_inner = item
        .inner
        .get("trait")
        .with_context(|| format!("Trait `{}` missing rustdoc trait body", name))?;
    let item_ids = trait_inner
        .get("items")
        .and_then(|value| value.as_array())
        .with_context(|| format!("Trait `{}` missing rustdoc items array", name))?;

    let mut methods = Vec::new();
    for method_id in item_ids {
        let method_id = value_id(method_id)
            .with_context(|| format!("Trait `{}` contained a non-item method id", name))?;
        let method_item = krate
            .index
            .get(&method_id)
            .with_context(|| format!("Trait `{}` references missing item `{}`", name, method_id))?;
        if let Some(method_def) = extract_method(&method_id, method_item)? {
            methods.push(method_def);
        }
    }

    Ok(TraitDef {
        name,
        methods,
        docs: clean_docs(item.docs.as_deref()),
    })
}

fn extract_method(item_id: &str, item: &Item) -> Result<Option<MethodDef>> {
    let Some(fn_inner) = item.inner.get("function") else {
        return Ok(None);
    };

    let name = item
        .name
        .as_ref()
        .cloned()
        .with_context(|| format!("Method item `{}` has no name", item_id))?;
    let sig = fn_inner
        .get("sig")
        .with_context(|| format!("Method `{}` missing rustdoc signature", name))?;
    let raw_output = sig
        .get("output")
        .with_context(|| format!("Method `{}` missing rustdoc return type", name))?;
    let output = unwrap_async_trait_future(raw_output).unwrap_or(raw_output);

    let (kind, return_type) = if is_result_subscription_return(output) {
        (
            MethodKind::ResultSubscription,
            ReturnType::ResultSubscription {
                item: extract_result_subscription_inner(output).with_context(|| {
                    format!(
                        "Method `{}` has invalid Result<Subscription<..>, E> return type",
                        name
                    )
                })?,
                err: extract_generic_arg(output, 1).with_context(|| {
                    format!(
                        "Method `{}` is missing the error type in Result<Subscription<..>, E>",
                        name
                    )
                })?,
            },
        )
    } else if is_subscription_return(output) {
        (
            MethodKind::Subscription,
            ReturnType::Subscription(extract_generic_arg(output, 0).with_context(|| {
                format!("Method `{}` is missing Subscription<T> item type", name)
            })?),
        )
    } else if is_result_return(output) {
        (
            MethodKind::Request,
            ReturnType::Result {
                ok: extract_generic_arg(output, 0).with_context(|| {
                    format!("Method `{}` is missing Result<T, E> ok type", name)
                })?,
                err: extract_generic_arg(output, 1).with_context(|| {
                    format!("Method `{}` is missing Result<T, E> error type", name)
                })?,
            },
        )
    } else {
        bail!(
            "Unsupported method return type for `{}`: {}",
            name,
            summarize_json(output)
        );
    };

    let inputs = sig
        .get("inputs")
        .and_then(|value| value.as_array())
        .with_context(|| format!("Method `{}` missing rustdoc inputs array", name))?;
    let mut params = Vec::new();
    for input in inputs {
        let arr = input
            .as_array()
            .with_context(|| format!("Method `{}` has an invalid input entry", name))?;
        let param_name = arr
            .first()
            .and_then(|value| value.as_str())
            .with_context(|| format!("Method `{}` has an unnamed input", name))?
            .to_string();
        if param_name == "self" {
            continue;
        }

        let ty = arr.get(1).with_context(|| {
            format!("Method `{}` input `{}` is missing a type", name, param_name)
        })?;
        if is_call_context_ref(ty) {
            continue;
        }
        let type_ref = resolve_type(ty).with_context(|| {
            format!(
                "Method `{}` input `{}` has an unsupported type",
                name, param_name
            )
        })?;

        params.push(ParamDef {
            name: param_name,
            type_ref,
        });
    }

    let wire_id = item.docs.as_deref().and_then(extract_wire_id);

    Ok(Some(MethodDef {
        name,
        kind,
        params,
        return_type,
        wire_id,
        docs: clean_docs(item.docs.as_deref()),
    }))
}

/// Strips hidden codegen marker lines from a rustdoc comment so it can be
/// emitted as user-facing JSDoc. Returns `None` when the remaining text is
/// empty.
pub fn clean_docs(docs: Option<&str>) -> Option<String> {
    let raw = docs?;
    let cleaned = raw
        .lines()
        .filter(|line| !is_codegen_doc_marker(line))
        .collect::<Vec<_>>()
        .join("\n");
    let trimmed = cleaned.trim_end_matches('\n').to_string();
    if trimmed.trim().is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

fn is_codegen_doc_marker(line: &str) -> bool {
    let line = line.trim_start();
    line.starts_with("@wire_id=")
}

/// Extracts `@wire_id=N` from a doc comment block. Annotated methods carry
/// this marker via the `#[wire(id = N)]` proc-macro, which appends the marker
/// as a hidden doc string so it propagates through rustdoc JSON.
///
/// Only the first `@wire_id=` marker is honored. The proc-macro emits exactly
/// one, and duplicate ids are rejected by `generate_wire_table` regardless.
/// Trailing non-digit characters (whitespace, newlines) terminate the match.
fn extract_wire_id(docs: &str) -> Option<u8> {
    let needle = "@wire_id=";
    let start = docs.find(needle)? + needle.len();
    let end = docs[start..]
        .find(|c: char| !c.is_ascii_digit())
        .map_or(docs.len(), |offset| start + offset);
    docs[start..end].parse::<u8>().ok()
}

/// Unwrap the `async_trait` expansion `Pin<Box<dyn Future<Output = T> + Send>>`
/// back to `T`. Returns `None` when the output does not match that pattern.
fn unwrap_async_trait_future(output: &serde_json::Value) -> Option<&serde_json::Value> {
    let pin = output.get("resolved_path")?;
    if path_suffix(pin.get("path")?.as_str()?) != "Pin" {
        return None;
    }
    let boxed = pin
        .get("args")?
        .get("angle_bracketed")?
        .get("args")?
        .as_array()?
        .first()?
        .get("type")?;
    let box_path = boxed.get("resolved_path")?;
    if path_suffix(box_path.get("path")?.as_str()?) != "Box" {
        return None;
    }
    let dyn_trait = box_path
        .get("args")?
        .get("angle_bracketed")?
        .get("args")?
        .as_array()?
        .first()?
        .get("type")?
        .get("dyn_trait")?;
    for entry in dyn_trait.get("traits")?.as_array()? {
        let trait_ref = entry.get("trait")?;
        if path_suffix(trait_ref.get("path")?.as_str()?) != "Future" {
            continue;
        }
        for constraint in trait_ref
            .get("args")?
            .get("angle_bracketed")?
            .get("constraints")?
            .as_array()?
        {
            if constraint.get("name")?.as_str()? == "Output" {
                return constraint.get("binding")?.get("equality")?.get("type");
            }
        }
    }
    None
}

fn path_suffix(path: &str) -> &str {
    path.rsplit("::").next().unwrap_or(path)
}

/// Whether `ty` is a `&CallContext` rustdoc param. Used to filter the ambient
/// [`truapi::CallContext`] out of generated API signatures because it is a
/// framework-level dependency, not part of the public wire contract.
fn is_call_context_ref(ty: &serde_json::Value) -> bool {
    let Some(inner) = ty.get("borrowed_ref").and_then(|r| r.get("type")) else {
        return false;
    };
    inner
        .get("resolved_path")
        .and_then(|r| r.get("path"))
        .and_then(|p| p.as_str())
        .map(|p| path_suffix(p) == "CallContext")
        .unwrap_or(false)
}

fn is_subscription_return(output: &serde_json::Value) -> bool {
    get_resolved_name(output)
        .map(|name| name == "Subscription")
        .unwrap_or(false)
}

fn is_result_subscription_return(output: &serde_json::Value) -> bool {
    if !is_result_return(output) {
        return false;
    }

    get_generic_arg_value(output, 0)
        .and_then(|ok| get_resolved_name(&ok))
        .map(|name| name == "Subscription")
        .unwrap_or(false)
}

fn is_result_return(output: &serde_json::Value) -> bool {
    get_resolved_name(output)
        .map(|name| name == "Result")
        .unwrap_or(false)
}

fn extract_result_subscription_inner(output: &serde_json::Value) -> Result<TypeRef> {
    let ok_type = get_generic_arg_value(output, 0)
        .context("Result<Subscription<T>, E> return type is missing its ok type")?;

    if get_resolved_name(&ok_type).as_deref() != Some("Subscription") {
        bail!(
            "Expected Result<Subscription<T>, E> return type, got {}",
            summarize_json(&ok_type)
        );
    }

    extract_generic_arg(&ok_type, 0)
}

fn get_resolved_name(ty: &serde_json::Value) -> Option<String> {
    ty.get("resolved_path")?
        .get("path")?
        .as_str()
        .map(ToString::to_string)
}

fn get_generic_arg_value(ty: &serde_json::Value, index: usize) -> Option<serde_json::Value> {
    let args = ty
        .get("resolved_path")?
        .get("args")?
        .get("angle_bracketed")?
        .get("args")?
        .as_array()?;
    args.get(index)?.get("type").cloned()
}

fn extract_generic_arg(ty: &serde_json::Value, index: usize) -> Result<TypeRef> {
    let generic = get_generic_arg_value(ty, index).with_context(|| {
        format!(
            "Missing generic argument {} in {}",
            index,
            summarize_json(ty)
        )
    })?;
    resolve_type(&generic)
}

fn resolve_type(ty: &serde_json::Value) -> Result<TypeRef> {
    if let Some(name) = ty.get("generic").and_then(|value| value.as_str()) {
        return Ok(TypeRef::Generic(name.to_string()));
    }

    if let Some(primitive) = ty.get("primitive").and_then(|value| value.as_str()) {
        return Ok(TypeRef::Primitive(primitive.to_string()));
    }

    if let Some(resolved) = ty.get("resolved_path") {
        let raw_name = resolved
            .get("path")
            .and_then(|value| value.as_str())
            .with_context(|| format!("resolved_path missing path in {}", summarize_json(ty)))?;
        let name = raw_name.rsplit("::").next().unwrap_or(raw_name);
        let args = resolve_resolved_path_args(resolved)?;

        return match name {
            "Vec" => Ok(TypeRef::Vec(Box::new(expect_single_arg("Vec", args)?))),
            "Option" => Ok(TypeRef::Option(Box::new(expect_single_arg(
                "Option", args,
            )?))),
            "String" => {
                if !args.is_empty() {
                    bail!(
                        "String should not carry generic arguments in {}",
                        summarize_json(ty)
                    );
                }
                Ok(TypeRef::Primitive("str".to_string()))
            }
            "Box" => expect_single_arg("Box", args),
            _ => Ok(TypeRef::Named {
                name: name.to_string(),
                args,
            }),
        };
    }

    if let Some(tuple) = ty.get("tuple") {
        let items = tuple.as_array().with_context(|| {
            format!(
                "tuple rustdoc shape was not an array: {}",
                summarize_json(ty)
            )
        })?;
        if items.is_empty() {
            return Ok(TypeRef::Unit);
        }
        let types = items.iter().map(resolve_type).collect::<Result<Vec<_>>>()?;
        return Ok(TypeRef::Tuple(types));
    }

    if let Some(array) = ty.get("array") {
        let inner = array
            .get("type")
            .context("array rustdoc shape is missing its inner type")
            .and_then(resolve_type)?;
        let len = array
            .get("len")
            .and_then(|value| value.as_str())
            .with_context(|| {
                format!(
                    "array rustdoc shape is missing its length in {}",
                    summarize_json(ty)
                )
            })?
            .parse::<usize>()
            .with_context(|| format!("array length was not a usize in {}", summarize_json(ty)))?;
        return Ok(TypeRef::Array(Box::new(inner), len));
    }

    if let Some(borrowed_ref) = ty.get("borrowed_ref") {
        let inner = borrowed_ref
            .get("type")
            .context("borrowed_ref rustdoc shape is missing its inner type")?;
        return resolve_type(inner);
    }

    bail!("Unsupported rustdoc type shape: {}", summarize_json(ty))
}

fn resolve_resolved_path_args(resolved: &serde_json::Value) -> Result<Vec<TypeRef>> {
    let Some(args) = resolved.get("args") else {
        return Ok(Vec::new());
    };
    if args.is_null() {
        return Ok(Vec::new());
    }

    let values = args
        .get("angle_bracketed")
        .and_then(|value| value.get("args"))
        .and_then(|value| value.as_array())
        .with_context(|| {
            format!(
                "Unsupported resolved_path generic args shape: {}",
                summarize_json(resolved)
            )
        })?;

    values
        .iter()
        .map(|arg| {
            let ty = arg.get("type").with_context(|| {
                format!(
                    "Unsupported generic argument entry without `type`: {}",
                    summarize_json(arg)
                )
            })?;
            resolve_type(ty)
        })
        .collect()
}

fn expect_single_arg(type_name: &str, mut args: Vec<TypeRef>) -> Result<TypeRef> {
    if args.len() != 1 {
        bail!(
            "Expected exactly one generic argument for `{}`, got {}",
            type_name,
            args.len()
        );
    }
    Ok(args.remove(0))
}

fn extract_struct(item_id: &str, item: &Item, krate: &Crate) -> Result<TypeDef> {
    let name = item
        .name
        .as_ref()
        .cloned()
        .with_context(|| format!("Struct item `{}` has no name", item_id))?;
    let struct_inner = item
        .inner
        .get("struct")
        .with_context(|| format!("Struct `{}` missing rustdoc body", name))?;
    let generic_params = extract_generic_params(struct_inner.get("generics"))
        .with_context(|| format!("Struct `{}` has unsupported generic parameters", name))?;
    let kind = struct_inner
        .get("kind")
        .with_context(|| format!("Struct `{}` missing rustdoc kind", name))?;

    let field_ids = kind
        .get("plain")
        .and_then(|value| value.get("fields"))
        .and_then(|value| value.as_array())
        .with_context(|| {
            format!(
                "Unsupported struct shape for `{}`: {}",
                name,
                summarize_json(kind)
            )
        })?;

    let mut fields = Vec::new();
    for field_id in field_ids {
        let field_id = value_id(field_id)
            .with_context(|| format!("Struct `{}` had a non-item field id", name))?;
        let field_item = krate.index.get(&field_id).with_context(|| {
            format!("Struct `{}` references missing field `{}`", name, field_id)
        })?;
        let field_name = field_item
            .name
            .as_ref()
            .cloned()
            .with_context(|| format!("Struct `{}` field `{}` has no name", name, field_id))?;
        let field_type = field_item.inner.get("struct_field").with_context(|| {
            format!(
                "Struct `{}` field `{}` is missing rustdoc type info",
                name, field_name
            )
        })?;
        fields.push(FieldDef {
            name: field_name,
            type_ref: resolve_type(field_type).with_context(|| {
                format!(
                    "Struct `{}` field `{}` has an unsupported type",
                    name, field_id
                )
            })?,
            docs: clean_docs(field_item.docs.as_deref()),
        });
    }

    Ok(TypeDef {
        name,
        generic_params,
        kind: TypeDefKind::Struct(fields),
        docs: clean_docs(item.docs.as_deref()),
    })
}

fn extract_enum(item_id: &str, item: &Item, krate: &Crate) -> Result<TypeDef> {
    let name = item
        .name
        .as_ref()
        .cloned()
        .with_context(|| format!("Enum item `{}` has no name", item_id))?;
    let enum_inner = item
        .inner
        .get("enum")
        .with_context(|| format!("Enum `{}` missing rustdoc body", name))?;
    let generic_params = extract_generic_params(enum_inner.get("generics"))
        .with_context(|| format!("Enum `{}` has unsupported generic parameters", name))?;
    let variant_ids = enum_inner
        .get("variants")
        .and_then(|value| value.as_array())
        .with_context(|| format!("Enum `{}` missing rustdoc variants", name))?;

    let mut variants = Vec::new();
    for variant_id in variant_ids {
        let variant_id = value_id(variant_id)
            .with_context(|| format!("Enum `{}` had a non-item variant id", name))?;
        let variant_item = krate.index.get(&variant_id).with_context(|| {
            format!(
                "Enum `{}` references missing variant `{}`",
                name, variant_id
            )
        })?;
        let variant_name = variant_item
            .name
            .as_ref()
            .cloned()
            .with_context(|| format!("Enum `{}` variant `{}` has no name", name, variant_id))?;
        let fields = extract_variant_fields(variant_item.inner.get("variant"), krate)
            .with_context(|| {
                format!(
                    "Enum `{}` variant `{}` has an unsupported shape",
                    name, variant_name
                )
            })?;
        variants.push(VariantDef {
            name: variant_name,
            fields,
            docs: clean_docs(variant_item.docs.as_deref()),
        });
    }

    Ok(TypeDef {
        name,
        generic_params,
        kind: TypeDefKind::Enum(variants),
        docs: clean_docs(item.docs.as_deref()),
    })
}

fn extract_variant_fields(
    variant_inner: Option<&serde_json::Value>,
    krate: &Crate,
) -> Result<VariantFields> {
    let inner = variant_inner.context("variant rustdoc entry is missing its body")?;
    let kind = inner
        .get("kind")
        .context("variant rustdoc entry is missing its kind")?;

    if kind.as_str() == Some("plain") {
        return Ok(VariantFields::Unit);
    }

    if let Some(field_ids) = kind.get("tuple").and_then(|tuple| {
        tuple.as_array().cloned().or_else(|| {
            tuple
                .get("fields")
                .and_then(|fields| fields.as_array())
                .cloned()
        })
    }) {
        let mut types = Vec::new();
        for field_id in &field_ids {
            let field_id =
                value_id(field_id).context("tuple variant field id was not an item id")?;
            let item = krate
                .index
                .get(&field_id)
                .with_context(|| format!("Missing tuple variant field `{}`", field_id))?;
            let ty = item.inner.get("struct_field").with_context(|| {
                format!(
                    "Tuple variant field `{}` is missing rustdoc type info",
                    field_id
                )
            })?;
            types.push(resolve_type(ty)?);
        }
        return if types.is_empty() {
            Ok(VariantFields::Unit)
        } else {
            Ok(VariantFields::Unnamed(types))
        };
    }

    if let Some(struct_value) = kind.get("struct") {
        let field_ids = struct_value
            .get("fields")
            .and_then(|value| value.as_array())
            .context("struct variant is missing its field list")?;
        let mut fields = Vec::new();
        for field_id in field_ids {
            let field_id =
                value_id(field_id).context("struct variant field id was not an item id")?;
            let item = krate
                .index
                .get(&field_id)
                .with_context(|| format!("Missing struct variant field `{}`", field_id))?;
            let name = item
                .name
                .as_ref()
                .cloned()
                .with_context(|| format!("Struct variant field `{}` has no name", field_id))?;
            let ty = item.inner.get("struct_field").with_context(|| {
                format!(
                    "Struct variant field `{}` is missing rustdoc type info",
                    field_id
                )
            })?;
            fields.push(FieldDef {
                name,
                type_ref: resolve_type(ty)?,
                docs: clean_docs(item.docs.as_deref()),
            });
        }
        return Ok(VariantFields::Named(fields));
    }

    bail!("Unsupported enum variant kind: {}", summarize_json(kind))
}

fn extract_type_alias(item_id: &str, item: &Item) -> Result<TypeDef> {
    let name = item
        .name
        .as_ref()
        .cloned()
        .with_context(|| format!("Type alias item `{}` has no name", item_id))?;
    let type_alias = item
        .inner
        .get("type_alias")
        .with_context(|| format!("Type alias `{}` missing rustdoc body", name))?;
    let generic_params = extract_generic_params(type_alias.get("generics"))
        .with_context(|| format!("Type alias `{}` has unsupported generic parameters", name))?;
    let ty = type_alias
        .get("type")
        .with_context(|| format!("Type alias `{}` is missing its target type", name))?;
    let target = resolve_type(ty)
        .with_context(|| format!("Type alias `{}` has an unsupported target type", name))?;

    Ok(TypeDef {
        name,
        generic_params,
        kind: TypeDefKind::Alias(target),
        docs: clean_docs(item.docs.as_deref()),
    })
}

fn extract_generic_params(generics: Option<&serde_json::Value>) -> Result<Vec<String>> {
    let Some(generics) = generics else {
        return Ok(Vec::new());
    };

    let params = generics
        .get("params")
        .and_then(|value| value.as_array())
        .context("generic params rustdoc shape was not an array")?;

    params
        .iter()
        .map(|param| {
            param
                .get("name")
                .and_then(|value| value.as_str())
                .map(ToString::to_string)
                .with_context(|| {
                    format!(
                        "Generic parameter is missing its name: {}",
                        summarize_json(param)
                    )
                })
        })
        .collect()
}

fn value_id(value: &serde_json::Value) -> Result<String> {
    if let Some(id) = value.as_str() {
        return Ok(id.to_string());
    }
    if let Some(id) = value.as_u64() {
        return Ok(id.to_string());
    }
    bail!("Expected rustdoc item id, got {}", summarize_json(value))
}

fn summarize_json(value: &serde_json::Value) -> String {
    const LIMIT: usize = 200;

    let mut text =
        serde_json::to_string(value).unwrap_or_else(|_| "<unserializable json>".to_string());
    if text.len() > LIMIT {
        text.truncate(LIMIT);
        text.push_str("...");
    }
    text
}
