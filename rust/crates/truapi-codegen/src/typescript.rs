//! TypeScript code generation from extracted API definitions.

use std::collections::{BTreeMap, HashMap};
use std::fmt::Write;
use std::fs;
use std::path::Path;

use anyhow::{bail, Result};

use crate::rustdoc::*;

#[derive(Default)]
struct CodecContext {
    generic_codecs: HashMap<String, String>,
}

/// A versioned enum wrapper like `enum HostSignPayloadRequest { V2(Inner) }`,
/// `enum HostCreateTransactionRequest { V2(CreateTransactionRequest) }`,
/// or a multi-version enum `enum HostDevicePermissionRequest { V1(_), V2(_) }`.
///
/// The client generator selects the latest wrapper variant up to its target
/// protocol version, so a V2 package emits V2 wire payloads when available and
/// falls back to V1 for wrappers whose shape did not change.
#[derive(Debug, Clone)]
struct VersionedWrapper {
    variants: BTreeMap<u32, VersionedWrapperVariant>,
}

#[derive(Debug, Clone)]
struct VersionedWrapperVariant {
    version: u32,
    kind: VersionedKind,
}

#[derive(Debug, Clone)]
enum VersionedKind {
    Unit,
    Tuple(TypeRef),
}

fn detect_versioned_wrapper(ty: &TypeDef) -> Option<VersionedWrapper> {
    if !ty.generic_params.is_empty() {
        return None;
    }
    let TypeDefKind::Enum(variants) = &ty.kind else {
        return None;
    };
    if variants.is_empty() || !variants.iter().all(|v| is_versioned_variant_name(&v.name)) {
        return None;
    }
    let mut version_variants = BTreeMap::new();
    for variant in variants {
        let version = version_number(&variant.name)?;
        let kind = match &variant.fields {
            VariantFields::Unit => VersionedKind::Unit,
            VariantFields::Unnamed(types) if types.len() == 1 => {
                VersionedKind::Tuple(types[0].clone())
            }
            _ => return None,
        };
        version_variants.insert(version, VersionedWrapperVariant { version, kind });
    }

    Some(VersionedWrapper {
        variants: version_variants,
    })
}

fn is_versioned_variant_name(name: &str) -> bool {
    version_number(name).is_some()
}

fn version_number(name: &str) -> Option<u32> {
    let rest = name.strip_prefix('V')?;
    if rest.is_empty() {
        return None;
    }
    rest.parse().ok()
}

fn collect_versioned_wrappers(api: &ApiDefinition) -> HashMap<String, VersionedWrapper> {
    api.types
        .iter()
        .filter_map(|ty| detect_versioned_wrapper(ty).map(|w| (ty.name.clone(), w)))
        .collect()
}

fn validate_versioned_wrapper_shapes(api: &ApiDefinition) -> Result<()> {
    for ty in &api.types {
        let TypeDefKind::Enum(variants) = &ty.kind else {
            continue;
        };
        if variants.is_empty() || !variants.iter().all(|v| is_versioned_variant_name(&v.name)) {
            continue;
        }
        for variant in variants {
            if matches!(variant.fields, VariantFields::Named(_)) {
                bail!(
                    "versioned wrapper `{}` variant `{}` uses named fields; define a request/response struct in the v0x module and wrap it as `{}`(v0x::MyStruct)",
                    ty.name,
                    variant.name,
                    variant.name
                );
            }
        }
    }
    Ok(())
}

fn versioned_wrapper_for<'a>(
    ty: &'a TypeRef,
    wrappers: &'a HashMap<String, VersionedWrapper>,
) -> Option<(&'a str, &'a VersionedWrapper)> {
    if let TypeRef::Named { name, args } = ty {
        if args.is_empty() {
            if let Some(wrapper) = wrappers.get(name) {
                return Some((name.as_str(), wrapper));
            }
        }
    }
    None
}

/// Emits a JSDoc block for `docs` at the given indent. No-op when `docs` is
/// `None` so callers can pipe rust doc strings through unconditionally.
///
/// Strips the conventional single space rustdoc preserves after `///` so the
/// emitted JSDoc reads `/** Foo */` rather than `/**  Foo */`. Deeper
/// indentation inside doc blocks is kept verbatim.
fn write_jsdoc(out: &mut String, indent: &str, docs: Option<&str>) {
    let Some(text) = docs else {
        return;
    };
    let safe = text.replace("*/", "*\\/");
    let lines: Vec<String> = safe
        .lines()
        .map(|line| {
            let trimmed = line.strip_prefix(' ').unwrap_or(line);
            trimmed.trim_end().to_string()
        })
        .collect();
    if lines.is_empty() {
        return;
    }
    if lines.len() == 1 {
        writeln!(out, "{}/** {} */", indent, lines[0]).unwrap();
        return;
    }
    writeln!(out, "{}/**", indent).unwrap();
    for line in &lines {
        if line.is_empty() {
            writeln!(out, "{} *", indent).unwrap();
        } else {
            writeln!(out, "{} * {}", indent, line).unwrap();
        }
    }
    writeln!(out, "{} */", indent).unwrap();
}

/// Generates the TypeScript client, types, and barrel files for an extracted
/// API definition into `output_dir`.
pub fn generate(
    api: &ApiDefinition,
    output_dir: &str,
    target_version: u32,
    codec_version: u8,
) -> Result<()> {
    fs::create_dir_all(output_dir)?;
    validate_versioned_wrapper_shapes(api)?;

    let types_code = generate_types(api)?;
    fs::write(Path::new(output_dir).join("types.ts"), types_code)?;

    let client_code = generate_client(api, target_version, codec_version)?;
    fs::write(Path::new(output_dir).join("client.ts"), client_code)?;

    let index_code = generate_index();
    fs::write(Path::new(output_dir).join("index.ts"), index_code)?;

    let wire_table_code = generate_wire_table(api, target_version)?;
    fs::write(Path::new(output_dir).join("wire-table.ts"), wire_table_code)?;

    Ok(())
}

fn generate_index() -> String {
    "export * from './types.js';\nexport * from './client.js';\n".to_string()
}

fn generate_wire_table(api: &ApiDefinition, target_version: u32) -> Result<String> {
    let wrappers = collect_versioned_wrappers(api);
    let mut entries: Vec<(u8, String)> = Vec::new();
    let mut seen: BTreeMap<u8, String> = BTreeMap::new();

    for trait_def in &api.traits {
        for method in &trait_def.methods {
            if !method_is_included(trait_def, method, &wrappers, target_version)? {
                continue;
            }
            let base = method.wire_id.expect("validated by method_is_included");
            let suffixes: &[&str] = match method.kind {
                MethodKind::Request => &["request", "response"],
                MethodKind::Subscription | MethodKind::ResultSubscription => {
                    &["start", "stop", "interrupt", "receive"]
                }
            };
            for (offset, suffix) in suffixes.iter().enumerate() {
                let id = base.checked_add(offset as u8).ok_or_else(|| {
                    anyhow::anyhow!("wire id overflow on `{}` (base {})", method.name, base)
                })?;
                let tag = format!("{}_{}", method.name, suffix);
                if let Some(existing) = seen.insert(id, tag.clone()) {
                    bail!(
                        "wire id {} reused: `{}` and `{}` collide",
                        id,
                        existing,
                        tag
                    );
                }
                entries.push((id, tag));
            }
        }
    }

    entries.sort_by_key(|(id, _)| *id);

    let mut out = String::new();
    writeln!(out, "// Auto-generated by truapi-codegen. Do not edit.").unwrap();
    writeln!(out, "//").unwrap();
    writeln!(
        out,
        "// Wire-protocol discriminant table. Pairs (id, tag) where tag is `<method>_<suffix>`,"
    )
    .unwrap();
    writeln!(
        out,
        "// suffix in {{request, response, start, stop, interrupt, receive}}."
    )
    .unwrap();
    writeln!(
        out,
        "// Method ordering is part of the wire protocol; only ever append."
    )
    .unwrap();
    writeln!(out).unwrap();
    writeln!(
        out,
        "/** Sorted (id, tag) pairs. Mirrors Rust `WIRE_TABLE` exactly. */"
    )
    .unwrap();
    writeln!(
        out,
        "export const WIRE_TABLE: ReadonlyArray<readonly [number, string]> = ["
    )
    .unwrap();
    for (id, tag) in &entries {
        writeln!(out, "  [{}, '{}'],", id, tag).unwrap();
    }
    writeln!(out, "];").unwrap();
    writeln!(out).unwrap();
    writeln!(out, "const ID_BY_TAG: Map<string, number> = new Map(").unwrap();
    writeln!(out, "  WIRE_TABLE.map(([id, tag]) => [tag, id]),").unwrap();
    writeln!(out, ");").unwrap();
    writeln!(out, "const TAG_BY_ID: Map<number, string> = new Map(").unwrap();
    writeln!(out, "  WIRE_TABLE.map(([id, tag]) => [id, tag]),").unwrap();
    writeln!(out, ");").unwrap();
    writeln!(out).unwrap();
    writeln!(out, "/** Lookup discriminant for a tag string. */").unwrap();
    writeln!(
        out,
        "export function idForTag(tag: string): number | undefined {{"
    )
    .unwrap();
    writeln!(out, "  return ID_BY_TAG.get(tag);").unwrap();
    writeln!(out, "}}").unwrap();
    writeln!(out).unwrap();
    writeln!(out, "/** Lookup tag for a discriminant. */").unwrap();
    writeln!(
        out,
        "export function tagForId(id: number): string | undefined {{"
    )
    .unwrap();
    writeln!(out, "  return TAG_BY_ID.get(id);").unwrap();
    writeln!(out, "}}").unwrap();

    Ok(out)
}

fn method_is_included(
    trait_def: &TraitDef,
    method: &MethodDef,
    wrappers: &HashMap<String, VersionedWrapper>,
    target_version: u32,
) -> Result<bool> {
    if method.wire_id.is_none() {
        bail!(
            "method `{}::{}` is missing #[wire(id = N)] annotation",
            trait_def.name,
            method.name
        );
    };

    let wrapper_names = method_versioned_wrappers(method, wrappers);
    Ok(
        wrapper_names.is_empty()
            || method_wire_version(method, wrappers, target_version)?.is_some(),
    )
}

/// Picks the wrapper variant the generated client emits on the wire for a
/// given method. Returns the lowest variant supported by every wrapper the
/// method touches and that is ≤ `target_version`. Returns `None` when no
/// shared variant exists at or below the cap (the method is not exposed by
/// the client).
///
/// Picking the **lowest** variant keeps outbound frames decodable by legacy
/// hosts (e.g. dotli's vendored `@novasamatech/host-api`, which only
/// registers each method's `v1` codec). Newer hosts that know multiple
/// variants still accept V1 because every wrapper keeps `V1` at
/// `#[codec(index = 0)]`.
fn method_wire_version(
    method: &MethodDef,
    wrappers: &HashMap<String, VersionedWrapper>,
    target_version: u32,
) -> Result<Option<u32>> {
    let wrapper_names = method_versioned_wrappers(method, wrappers);
    if wrapper_names.is_empty() {
        return Ok(None);
    }

    let mut candidates: Option<Vec<u32>> = None;
    for wrapper_name in wrapper_names {
        let wrapper = wrappers
            .get(&wrapper_name)
            .expect("method_versioned_wrappers only returns known wrappers");
        let versions = wrapper
            .variants
            .keys()
            .filter(|version| **version <= target_version)
            .copied()
            .collect::<Vec<_>>();
        candidates = Some(match candidates {
            Some(current) => current
                .into_iter()
                .filter(|version| versions.contains(version))
                .collect(),
            None => versions,
        });
    }

    Ok(candidates.and_then(|versions| versions.into_iter().min()))
}

fn method_versioned_wrappers(
    method: &MethodDef,
    wrappers: &HashMap<String, VersionedWrapper>,
) -> Vec<String> {
    let mut names = Vec::new();
    for param in &method.params {
        collect_type_versioned_wrappers(&param.type_ref, wrappers, &mut names);
    }
    match &method.return_type {
        ReturnType::Result { ok, err } => {
            collect_type_versioned_wrappers(ok, wrappers, &mut names);
            collect_type_versioned_wrappers(
                call_error_inner(err).unwrap_or(err),
                wrappers,
                &mut names,
            );
        }
        ReturnType::Subscription(item) => {
            collect_type_versioned_wrappers(item, wrappers, &mut names);
        }
        ReturnType::ResultSubscription { item, err: _ } => {
            collect_type_versioned_wrappers(item, wrappers, &mut names);
        }
    }
    names.sort();
    names.dedup();
    names
}

fn call_error_inner(ty: &TypeRef) -> Option<&TypeRef> {
    match ty {
        TypeRef::Named { name, args } if name == "CallError" && args.len() == 1 => Some(&args[0]),
        _ => None,
    }
}

fn collect_type_versioned_wrappers(
    ty: &TypeRef,
    wrappers: &HashMap<String, VersionedWrapper>,
    names: &mut Vec<String>,
) {
    match ty {
        TypeRef::Named { name, args } => {
            if args.is_empty() && wrappers.contains_key(name) {
                names.push(name.clone());
            }
            for arg in args {
                collect_type_versioned_wrappers(arg, wrappers, names);
            }
        }
        TypeRef::Vec(inner) | TypeRef::Option(inner) | TypeRef::Array(inner, _) => {
            collect_type_versioned_wrappers(inner, wrappers, names);
        }
        TypeRef::Tuple(items) => {
            for item in items {
                collect_type_versioned_wrappers(item, wrappers, names);
            }
        }
        TypeRef::Primitive(_) | TypeRef::Generic(_) | TypeRef::Unit => {}
    }
}

fn generate_types(api: &ApiDefinition) -> Result<String> {
    let mut out = String::new();
    writeln!(out, "// Auto-generated by truapi-codegen. Do not edit.").unwrap();
    writeln!(out).unwrap();
    writeln!(out, "import * as S from '../scale.js';").unwrap();
    writeln!(out).unwrap();

    for ty in &api.types {
        write_type_definition(&mut out, ty)?;
        writeln!(out).unwrap();
        write_codec_definition(&mut out, ty)?;
        writeln!(out).unwrap();
    }

    Ok(out)
}

fn generate_client(api: &ApiDefinition, target_version: u32, codec_version: u8) -> Result<String> {
    validate_versioned_wrapper_shapes(api)?;

    let mut out = String::new();
    writeln!(out, "// Auto-generated by truapi-codegen. Do not edit.").unwrap();
    writeln!(out).unwrap();
    writeln!(out, "import {{ err, ok, type Result }} from 'neverthrow';").unwrap();
    writeln!(out, "import * as S from '../scale.js';").unwrap();
    writeln!(
        out,
        "import type {{ SubscribeCallbacks, Subscription, TrUApiTransport }} from '../transport.js';"
    )
    .unwrap();
    writeln!(out, "import * as T from './types.js';").unwrap();
    writeln!(out).unwrap();
    writeln!(out, "export {{ Result }};").unwrap();
    writeln!(out, "export type {{ Subscription, TrUApiTransport }};").unwrap();
    writeln!(
        out,
        "export const TRUAPI_VERSION = {} as const;",
        target_version
    )
    .unwrap();
    writeln!(
        out,
        "export const TRUAPI_CODEC_VERSION = {} as const;",
        codec_version
    )
    .unwrap();
    writeln!(out).unwrap();

    let ctx = CodecContext::default();
    let wrappers = collect_versioned_wrappers(api);

    for trait_def in &api.traits {
        if trait_def.name == "TrUApi" {
            continue;
        }
        let methods = included_methods(trait_def, &wrappers, target_version)?;
        if methods.is_empty() {
            continue;
        }

        write_jsdoc(&mut out, "", trait_def.docs.as_deref());
        writeln!(out, "export class {}Client {{", trait_def.name).unwrap();
        writeln!(
            out,
            "  constructor(private readonly transport: TrUApiTransport) {{}}"
        )
        .unwrap();
        writeln!(out).unwrap();

        for method in methods {
            emit_method(&mut out, method, &wrappers, &ctx, target_version)?;
            writeln!(out).unwrap();
        }

        writeln!(out, "}}").unwrap();
        writeln!(out).unwrap();
    }

    writeln!(out, "export interface TrUApiClient {{").unwrap();
    for trait_def in &api.traits {
        if trait_def.name == "TrUApi" {
            continue;
        }
        if included_methods(trait_def, &wrappers, target_version)?.is_empty() {
            continue;
        }
        let field = to_camel_case(&trait_def.name);
        writeln!(out, "  readonly {}: {}Client;", field, trait_def.name).unwrap();
    }
    writeln!(out, "}}").unwrap();
    writeln!(out).unwrap();
    writeln!(
        out,
        "/** Creates the generated client facade by binding each service namespace to the"
    )
    .unwrap();
    writeln!(out, " * shared transport instance. */").unwrap();

    writeln!(
        out,
        "export function createClient(transport: TrUApiTransport): TrUApiClient {{"
    )
    .unwrap();
    writeln!(out, "  return {{").unwrap();
    for trait_def in &api.traits {
        if trait_def.name == "TrUApi" {
            continue;
        }
        if included_methods(trait_def, &wrappers, target_version)?.is_empty() {
            continue;
        }
        let field = to_camel_case(&trait_def.name);
        writeln!(
            out,
            "    {}: new {}Client(transport),",
            field, trait_def.name
        )
        .unwrap();
    }
    writeln!(out, "  }};").unwrap();
    writeln!(out, "}}").unwrap();

    Ok(out)
}

fn included_methods<'a>(
    trait_def: &'a TraitDef,
    wrappers: &HashMap<String, VersionedWrapper>,
    target_version: u32,
) -> Result<Vec<&'a MethodDef>> {
    trait_def
        .methods
        .iter()
        .filter_map(|method| {
            match method_is_included(trait_def, method, wrappers, target_version) {
                Ok(true) => Some(Ok(method)),
                Ok(false) => None,
                Err(err) => Some(Err(err)),
            }
        })
        .collect()
}

fn write_payload_field(
    out: &mut String,
    indent: &str,
    codec_expr: &str,
    wire_version: Option<u32>,
    value_expr: &str,
) {
    if let Some(version) = wire_version {
        writeln!(
            out,
            "{}payload: {}.enc({{ tag: \"V{}\", value: {} }}),",
            indent, codec_expr, version, value_expr
        )
        .unwrap();
    } else {
        writeln!(
            out,
            "{}payload: {}.enc({}),",
            indent, codec_expr, value_expr
        )
        .unwrap();
    }
}

/// Lowered method payload: the TS param list, the inner value expression, and
/// the wire codec/version used by the generated client to produce payload
/// bytes. The public method signature stays ergonomic (inner version types),
/// while the generated client owns versioned wrapper encoding.
struct PayloadEmission {
    /// Comma-separated `name: Type` entries used as the body of the user-facing
    /// object argument type. Empty when the method takes no input.
    param_list: String,
    /// Local names destructured in the method signature and referenced by
    /// `value_expr`.
    param_names: Vec<String>,
    inner_type_ts: String,
    value_expr: String,
    wire_codec_expr: String,
    wire_version: Option<u32>,
}

fn emit_payload(
    params: &[ParamDef],
    wrappers: &HashMap<String, VersionedWrapper>,
    ctx: &CodecContext,
    wire_version: Option<u32>,
) -> Result<PayloadEmission> {
    // The unified contract always takes a single versioned-wrapper arg. On the
    // TS side callers pass the inner value directly, but we re-wrap before
    // handing bytes to the transport. The Rust dispatcher decodes the full
    // wrapper (variant byte included) from the wire payload; see
    // `rust_dispatcher::MethodEmission`.
    if params.len() == 1 {
        if let Some((wrapper_name, wrapper)) = versioned_wrapper_for(&params[0].type_ref, wrappers)
        {
            let version = wire_version.ok_or_else(|| {
                anyhow::anyhow!("versioned wrapper `{wrapper_name}` has no selected wire version")
            })?;
            let wrapper = wrapper.variants.get(&version).ok_or_else(|| {
                anyhow::anyhow!("versioned wrapper `{wrapper_name}` has no V{version} variant")
            })?;
            return match &wrapper.kind {
                VersionedKind::Unit => Ok(PayloadEmission {
                    param_list: String::new(),
                    param_names: Vec::new(),
                    inner_type_ts: "undefined".to_string(),
                    value_expr: "undefined".to_string(),
                    wire_codec_expr: format!("T.{}", wrapper_name),
                    wire_version: Some(wrapper.version),
                }),
                VersionedKind::Tuple(inner) => {
                    let inner_ts = ts_type_qualified(inner)?;
                    Ok(PayloadEmission {
                        param_list: format!("request: {}", inner_ts),
                        param_names: vec!["request".to_string()],
                        inner_type_ts: inner_ts,
                        value_expr: "request".to_string(),
                        wire_codec_expr: format!("T.{}", wrapper_name),
                        wire_version: Some(wrapper.version),
                    })
                }
            };
        }
    }

    let inner_type_ts = payload_type(params)?;
    let has_request = !params.is_empty();
    Ok(PayloadEmission {
        param_list: if has_request {
            format!("request: {}", inner_type_ts)
        } else {
            String::new()
        },
        param_names: if has_request {
            vec!["request".to_string()]
        } else {
            Vec::new()
        },
        inner_type_ts: inner_type_ts.clone(),
        value_expr: if has_request {
            "request".to_string()
        } else {
            "undefined".to_string()
        },
        wire_codec_expr: method_payload_codec_expr(params, true, ctx)?,
        wire_version: None,
    })
}

/// Response shape after the versioned wrapper is stripped. TS callers see the
/// inner type; request responses decode `Versioned<Result<Ok, Err>>`, while
/// subscription items still decode the full versioned item wrapper.
#[derive(Clone)]
struct ResponseEmission {
    inner_type_ts: String,
    wire_type_ts: String,
    wire_codec_expr: String,
    inner_codec_expr: String,
}

fn versioned_value_cast(wire_type: &str, inner_type: &str, version: u32) -> String {
    format!(
        "{{ tag: \"V{}\"; value: {} }} & {}",
        version, inner_type, wire_type
    )
}

fn versioned_value_expr(
    value_expr: &str,
    wire_type: &str,
    inner_type: &str,
    version: u32,
) -> String {
    format!(
        "({} as {}).value",
        value_expr,
        versioned_value_cast(wire_type, inner_type, version)
    )
}

fn emit_response(
    ty: &TypeRef,
    wrappers: &HashMap<String, VersionedWrapper>,
    ctx: &CodecContext,
    wire_version: Option<u32>,
) -> Result<ResponseEmission> {
    if let Some((wrapper_name, wrapper)) = versioned_wrapper_for(ty, wrappers) {
        let version = wire_version.ok_or_else(|| {
            anyhow::anyhow!("versioned wrapper `{wrapper_name}` has no selected wire version")
        })?;
        let wrapper = wrapper.variants.get(&version).ok_or_else(|| {
            anyhow::anyhow!("versioned wrapper `{wrapper_name}` has no V{version} variant")
        })?;
        return match &wrapper.kind {
            VersionedKind::Unit => Ok(ResponseEmission {
                inner_type_ts: "undefined".to_string(),
                wire_type_ts: format!("T.{}", wrapper_name),
                wire_codec_expr: format!("T.{}", wrapper_name),
                inner_codec_expr: "S.unit".to_string(),
            }),
            VersionedKind::Tuple(inner) => Ok(ResponseEmission {
                inner_type_ts: ts_type_qualified(inner)?,
                wire_type_ts: format!("T.{}", wrapper_name),
                wire_codec_expr: format!("T.{}", wrapper_name),
                inner_codec_expr: codec_expr(inner, true, ctx)?,
            }),
        };
    }

    Ok(ResponseEmission {
        inner_type_ts: ts_type_qualified(ty)?,
        wire_type_ts: ts_type_qualified(ty)?,
        wire_codec_expr: codec_expr(ty, true, ctx)?,
        inner_codec_expr: codec_expr(ty, true, ctx)?,
    })
}

fn emit_error_response(
    ty: &TypeRef,
    wrappers: &HashMap<String, VersionedWrapper>,
    ctx: &CodecContext,
    wire_version: Option<u32>,
) -> Result<ResponseEmission> {
    emit_response(
        call_error_inner(ty).unwrap_or(ty),
        wrappers,
        ctx,
        wire_version,
    )
}

fn versioned_kind_codec_expr(
    kind: &VersionedKind,
    qualified: bool,
    ctx: &CodecContext,
) -> Result<String> {
    match kind {
        VersionedKind::Unit => Ok("S.unit".to_string()),
        VersionedKind::Tuple(inner) => codec_expr(inner, qualified, ctx),
    }
}

fn indexed_versioned_codec_expr(
    variants: impl IntoIterator<Item = (u32, String)>,
) -> Result<String> {
    let mut entries = Vec::new();
    for (version, codec) in variants {
        let index = version
            .checked_sub(1)
            .ok_or_else(|| anyhow::anyhow!("versioned wrapper uses invalid V0 variant"))?;
        entries.push(format!("V{}: [{}, {}] as const", version, index, codec));
    }
    Ok(format!("S.indexedTaggedUnion({{{}}})", entries.join(", ")))
}

fn versioned_result_codec_expr(version: u32, ok_codec: &str, err_codec: &str) -> Result<String> {
    indexed_versioned_codec_expr([(version, format!("S.result({}, {})", ok_codec, err_codec))])
}

fn emit_method(
    out: &mut String,
    method: &MethodDef,
    wrappers: &HashMap<String, VersionedWrapper>,
    ctx: &CodecContext,
    target_version: u32,
) -> Result<()> {
    let ts_method_name = to_camel_case(&strip_prefix(&method.name));
    let wire_version = method_wire_version(method, wrappers, target_version)?;
    let payload = emit_payload(&method.params, wrappers, ctx, wire_version)?;
    write_jsdoc(out, "  ", method.docs.as_deref());

    match (&method.kind, &method.return_type) {
        (MethodKind::Request, ReturnType::Result { ok, err }) => {
            let is_handshake = method.name == "host_handshake";
            let response = emit_response(ok, wrappers, ctx, wire_version)?;
            let error = emit_error_response(err, wrappers, ctx, wire_version)?;
            let response_codec = match wire_version {
                Some(version) => versioned_result_codec_expr(
                    version,
                    &response.inner_codec_expr,
                    &error.inner_codec_expr,
                )?,
                None => format!(
                    "S.result({}, {})",
                    response.wire_codec_expr, error.wire_codec_expr
                ),
            };

            let arg_decl = if is_handshake || payload.param_list.is_empty() {
                String::new()
            } else {
                format!("request: {}", payload.inner_type_ts)
            };
            let request_expr = if is_handshake {
                "this.transport.codecVersion"
            } else {
                &payload.value_expr
            };

            writeln!(
                out,
                "  async {}({}): Promise<Result<{}, {}>> {{",
                ts_method_name, arg_decl, response.inner_type_ts, error.inner_type_ts
            )
            .unwrap();
            writeln!(out, "    const result = await this.transport.request<").unwrap();
            writeln!(
                out,
                "      S.ResultPayload<{}, {}>",
                response.inner_type_ts, error.inner_type_ts
            )
            .unwrap();
            writeln!(out, "    >({{").unwrap();
            writeln!(out, "      method: \"{}\",", method.name).unwrap();
            write_payload_field(
                out,
                "      ",
                &payload.wire_codec_expr,
                payload.wire_version,
                request_expr,
            );
            if wire_version.is_some() {
                writeln!(
                    out,
                    "      decodeResponse: (payload) => {}.dec(payload).value,",
                    response_codec
                )
                .unwrap();
            } else {
                writeln!(
                    out,
                    "      decodeResponse: (payload) => {}.dec(payload),",
                    response_codec
                )
                .unwrap();
            }
            writeln!(out, "    }});").unwrap();
            writeln!(
                out,
                "    return result.success ? ok(result.value) : err(result.value);"
            )
            .unwrap();
            writeln!(out, "  }}").unwrap();
        }
        (MethodKind::Subscription, ReturnType::Subscription(ty)) => {
            let response = emit_response(ty, wrappers, ctx, wire_version)?;
            emit_subscribe_method(
                out,
                &ts_method_name,
                &method.name,
                &payload,
                &response,
                response.inner_type_ts.clone(),
                None,
                wire_version,
            )?;
        }
        (MethodKind::ResultSubscription, ReturnType::ResultSubscription { item, err: _ }) => {
            let response = emit_response(item, wrappers, ctx, wire_version)?;
            emit_subscribe_method(
                out,
                &ts_method_name,
                &method.name,
                &payload,
                &response,
                response.inner_type_ts.clone(),
                None,
                wire_version,
            )?;
        }
        (kind, return_type) => {
            bail!(
                "Generator internal mismatch for method `{}`: kind {:?} does not match return type {:?}",
                method.name,
                kind,
                return_type
            );
        }
    }

    Ok(())
}

/// Emits a subscribe method body that takes a single object combining the
/// method-specific input fields with the universal `onData`/`onInterrupt`
/// callbacks. `_interrupt` is payloadless for compatibility, so generated
/// methods decode only `_receive` payloads.
#[allow(clippy::too_many_arguments)]
fn emit_subscribe_method(
    out: &mut String,
    ts_method_name: &str,
    wire_method_name: &str,
    payload: &PayloadEmission,
    response: &ResponseEmission,
    item_type_ts: String,
    err: Option<ResponseEmission>,
    wire_version: Option<u32>,
) -> Result<()> {
    let _ = err;
    let pick = format!(
        "Pick<SubscribeCallbacks<{}>, 'onData' | 'onInterrupt'>",
        item_type_ts
    );
    let args_type = if payload.param_list.is_empty() {
        pick
    } else {
        format!("{{ {} }} & {}", payload.param_list, pick)
    };

    let mut destructure = vec!["onData".to_string(), "onInterrupt".to_string()];
    destructure.extend(payload.param_names.iter().cloned());

    writeln!(
        out,
        "  {}({{ {} }}: {}): Subscription {{",
        ts_method_name,
        destructure.join(", "),
        args_type
    )
    .unwrap();

    writeln!(
        out,
        "    return this.transport.subscribe<{}>({{",
        item_type_ts
    )
    .unwrap();
    writeln!(out, "      method: \"{}\",", wire_method_name).unwrap();
    write_payload_field(
        out,
        "      ",
        &payload.wire_codec_expr,
        payload.wire_version,
        &payload.value_expr,
    );
    let item_value = if let Some(version) = wire_version {
        versioned_value_expr(
            &format!("{}.dec(payload)", response.wire_codec_expr),
            &response.wire_type_ts,
            &item_type_ts,
            version,
        )
    } else {
        format!("{}.dec(payload)", response.wire_codec_expr)
    };
    writeln!(out, "      onData: (payload) => onData({}),", item_value).unwrap();
    writeln!(out, "      onInterrupt,").unwrap();
    writeln!(out, "      onClose: onInterrupt,").unwrap();
    writeln!(out, "    }});").unwrap();
    writeln!(out, "  }}").unwrap();

    Ok(())
}

fn write_type_definition(out: &mut String, ty: &TypeDef) -> Result<()> {
    let generic_decl = generic_param_declaration(&ty.generic_params);

    write_jsdoc(out, "", ty.docs.as_deref());
    match &ty.kind {
        TypeDefKind::Alias(type_ref) => {
            writeln!(
                out,
                "export type {}{} = {};",
                ty.name,
                generic_decl,
                ts_type(type_ref)?
            )
            .unwrap();
        }
        TypeDefKind::Struct(fields) => {
            writeln!(out, "export interface {}{} {{", ty.name, generic_decl).unwrap();
            for field in fields {
                let (ts_name, optional) = ts_field_name(&field.name, &field.type_ref);
                write_jsdoc(out, "  ", field.docs.as_deref());
                if optional {
                    writeln!(
                        out,
                        "  {}?: {};",
                        ts_name,
                        ts_inner_option(&field.type_ref)?
                    )
                    .unwrap();
                } else {
                    writeln!(out, "  {}: {};", ts_name, ts_type(&field.type_ref)?).unwrap();
                }
            }
            writeln!(out, "}}").unwrap();
        }
        TypeDefKind::Enum(variants) => {
            writeln!(out, "export type {}{} =", ty.name, generic_decl).unwrap();
            for variant in variants {
                write_jsdoc(out, "  ", variant.docs.as_deref());
                writeln!(
                    out,
                    "  | {{ tag: \"{}\"; value: {} }}",
                    variant.name,
                    variant_value_type(&variant.fields)?
                )
                .unwrap();
            }
            writeln!(out, ";").unwrap();
        }
    }

    Ok(())
}

fn write_codec_definition(out: &mut String, ty: &TypeDef) -> Result<()> {
    if ty.generic_params.is_empty() {
        let ctx = CodecContext::default();
        if let Some(wrapper) = detect_versioned_wrapper(ty) {
            let codec_expr = indexed_versioned_codec_expr(
                wrapper
                    .variants
                    .values()
                    .map(|variant| {
                        Ok((
                            variant.version,
                            versioned_kind_codec_expr(&variant.kind, false, &ctx)?,
                        ))
                    })
                    .collect::<Result<Vec<_>>>()?,
            )?;
            writeln!(
                out,
                "export const {}: S.Codec<{}> = S.lazy((): S.Codec<{}> => {});",
                ty.name,
                top_level_type_name(&ty.name, &ty.generic_params),
                top_level_type_name(&ty.name, &ty.generic_params),
                codec_expr
            )
            .unwrap();
            return Ok(());
        }
        writeln!(
            out,
            "export const {}: S.Codec<{}> = S.lazy((): S.Codec<{}> => {});",
            ty.name,
            top_level_type_name(&ty.name, &ty.generic_params),
            top_level_type_name(&ty.name, &ty.generic_params),
            type_codec_expr(ty, &ctx)?
        )
        .unwrap();
        return Ok(());
    }

    let generic_decl = generic_param_declaration(&ty.generic_params);
    let codec_params = ty
        .generic_params
        .iter()
        .map(|param| format!("{}: S.Codec<{}>", codec_param_name(param), param))
        .collect::<Vec<_>>()
        .join(", ");
    let ctx = codec_context(&ty.generic_params);
    let type_name = top_level_type_name(&ty.name, &ty.generic_params);

    if ty.name == "Component" {
        writeln!(
            out,
            "/** Builds a codec for renderer components parameterized by the codec of their"
        )
        .unwrap();
        writeln!(out, " * `props` payload. */").unwrap();
    }
    writeln!(
        out,
        "export function {}{}({}): S.Codec<{}> {{",
        ty.name, generic_decl, codec_params, type_name
    )
    .unwrap();
    writeln!(
        out,
        "  return S.lazy((): S.Codec<{}> => {});",
        type_name,
        type_codec_expr(ty, &ctx)?
    )
    .unwrap();
    writeln!(out, "}}").unwrap();

    Ok(())
}

fn type_codec_expr(ty: &TypeDef, ctx: &CodecContext) -> Result<String> {
    match &ty.kind {
        TypeDefKind::Alias(type_ref) => codec_expr(type_ref, false, ctx),
        TypeDefKind::Struct(fields) => struct_codec_expr(
            fields,
            &top_level_type_name(&ty.name, &ty.generic_params),
            false,
            ctx,
        ),
        TypeDefKind::Enum(variants) => {
            let variants = variants
                .iter()
                .map(|variant| {
                    Ok(format!(
                        "{}: {}",
                        variant.name,
                        variant_codec_expr(&variant.fields, false, ctx)?
                    ))
                })
                .collect::<Result<Vec<_>>>()?
                .join(", ");
            Ok(format!("S.taggedUnion({{{}}})", variants))
        }
    }
}

fn variant_value_type(fields: &VariantFields) -> Result<String> {
    match fields {
        VariantFields::Unit => Ok("undefined".to_string()),
        VariantFields::Unnamed(types) => {
            if types.len() == 1 {
                ts_type(&types[0])
            } else {
                Ok(format!(
                    "[{}]",
                    types
                        .iter()
                        .map(ts_type)
                        .collect::<Result<Vec<_>>>()?
                        .join(", ")
                ))
            }
        }
        VariantFields::Named(fields) => inline_object_type(fields, false),
    }
}

fn variant_codec_expr(
    fields: &VariantFields,
    qualified: bool,
    ctx: &CodecContext,
) -> Result<String> {
    match fields {
        VariantFields::Unit => Ok("S.unit".to_string()),
        VariantFields::Unnamed(types) => {
            if types.len() == 1 {
                codec_expr(&types[0], qualified, ctx)
            } else {
                let codecs = types
                    .iter()
                    .map(|ty| codec_expr(ty, qualified, ctx))
                    .collect::<Result<Vec<_>>>()?
                    .join(", ");
                Ok(format!("S.tuple({})", codecs))
            }
        }
        VariantFields::Named(fields) => struct_codec_expr(
            fields,
            &inline_object_type(fields, qualified)?,
            qualified,
            ctx,
        ),
    }
}

fn struct_codec_expr(
    fields: &[FieldDef],
    type_name: &str,
    qualified: bool,
    ctx: &CodecContext,
) -> Result<String> {
    let field_specs = fields
        .iter()
        .map(|field| {
            let (name, _optional) = ts_field_name(&field.name, &field.type_ref);
            Ok(format!(
                "{}: {}",
                name,
                codec_expr(&field.type_ref, qualified, ctx)?
            ))
        })
        .collect::<Result<Vec<_>>>()?
        .join(", ");
    Ok(format!(
        "S.struct({{{}}}) as S.Codec<{}>",
        field_specs, type_name
    ))
}

fn inline_object_type(fields: &[FieldDef], qualified: bool) -> Result<String> {
    Ok(format!(
        "{{ {} }}",
        fields
            .iter()
            .map(|field| {
                let (name, optional) = ts_field_name(&field.name, &field.type_ref);
                if optional {
                    Ok(format!(
                        "{}?: {}",
                        name,
                        ts_inner_option_with_named(&field.type_ref, qualified)?
                    ))
                } else {
                    Ok(format!(
                        "{}: {}",
                        name,
                        ts_type_with_named(&field.type_ref, qualified)?
                    ))
                }
            })
            .collect::<Result<Vec<_>>>()?
            .join("; ")
    ))
}

fn method_payload_codec_expr(
    params: &[ParamDef],
    qualified: bool,
    ctx: &CodecContext,
) -> Result<String> {
    match params.len() {
        0 => Ok("S.unit".to_string()),
        1 => codec_expr(&params[0].type_ref, qualified, ctx),
        _ => {
            let codecs = params
                .iter()
                .map(|param| codec_expr(&param.type_ref, qualified, ctx))
                .collect::<Result<Vec<_>>>()?
                .join(", ");
            Ok(format!("S.tuple({})", codecs))
        }
    }
}

fn codec_expr(ty: &TypeRef, qualified: bool, ctx: &CodecContext) -> Result<String> {
    match ty {
        TypeRef::Primitive(name) => match name.as_str() {
            "bool" => Ok("S.bool".to_string()),
            "u8" => Ok("S.u8".to_string()),
            "u16" => Ok("S.u16".to_string()),
            "u32" => Ok("S.u32".to_string()),
            "u64" => Ok("S.u64".to_string()),
            "u128" => Ok("S.u128".to_string()),
            "i8" => Ok("S.i8".to_string()),
            "i16" => Ok("S.i16".to_string()),
            "i32" => Ok("S.i32".to_string()),
            "i64" => Ok("S.i64".to_string()),
            "i128" => Ok("S.i128".to_string()),
            "f32" => Ok("S.f32".to_string()),
            "f64" => Ok("S.f64".to_string()),
            "str" => Ok("S.str".to_string()),
            _ => bail!(
                "Unsupported primitive type `{}` in TypeScript codec generation",
                name
            ),
        },
        TypeRef::Named { name, args } => {
            let target = if qualified {
                format!("T.{}", name)
            } else {
                name.clone()
            };

            if args.is_empty() {
                Ok(target)
            } else {
                let codecs = args
                    .iter()
                    .map(|arg| codec_expr(arg, qualified, ctx))
                    .collect::<Result<Vec<_>>>()?
                    .join(", ");
                Ok(format!("{}({})", target, codecs))
            }
        }
        TypeRef::Vec(inner) => match inner.as_ref() {
            TypeRef::Primitive(name) if name == "u8" => Ok("S.bytes".to_string()),
            _ => Ok(format!("S.vec({})", codec_expr(inner, qualified, ctx)?)),
        },
        TypeRef::Option(inner) => Ok(format!("S.option({})", codec_expr(inner, qualified, ctx)?)),
        TypeRef::Tuple(items) => {
            if items.is_empty() {
                Ok("S.unit".to_string())
            } else {
                let codecs = items
                    .iter()
                    .map(|item| codec_expr(item, qualified, ctx))
                    .collect::<Result<Vec<_>>>()?
                    .join(", ");
                Ok(format!("S.tuple({})", codecs))
            }
        }
        TypeRef::Array(inner, len) => match inner.as_ref() {
            TypeRef::Primitive(name) if name == "u8" => Ok(format!("S.byteArray({})", len)),
            _ => Ok(format!(
                "S.array({}, {})",
                codec_expr(inner, qualified, ctx)?,
                len
            )),
        },
        TypeRef::Generic(name) => ctx
            .generic_codecs
            .get(name)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Missing codec for generic parameter `{}`", name)),
        TypeRef::Unit => Ok("S.unit".to_string()),
    }
}

fn ts_type(ty: &TypeRef) -> Result<String> {
    ts_type_with_named(ty, false)
}

fn ts_type_with_named(ty: &TypeRef, qualified: bool) -> Result<String> {
    match ty {
        TypeRef::Primitive(name) => match name.as_str() {
            "bool" => Ok("boolean".to_string()),
            "u8" | "u16" | "u32" | "i8" | "i16" | "i32" | "f32" | "f64" => Ok("number".to_string()),
            "u64" | "u128" | "i64" | "i128" => Ok("bigint".to_string()),
            "str" => Ok("string".to_string()),
            _ => bail!(
                "Unsupported primitive type `{}` in TypeScript type generation",
                name
            ),
        },
        TypeRef::Named { name, args } => {
            let target = if qualified {
                format!("T.{}", name)
            } else {
                name.clone()
            };

            if args.is_empty() {
                Ok(target)
            } else {
                let args = args
                    .iter()
                    .map(|arg| ts_type_with_named(arg, qualified))
                    .collect::<Result<Vec<_>>>()?
                    .join(", ");
                Ok(format!("{}<{}>", target, args))
            }
        }
        TypeRef::Vec(inner) => match inner.as_ref() {
            TypeRef::Primitive(name) if name == "u8" => Ok("Uint8Array".to_string()),
            _ => Ok(format!("Array<{}>", ts_type_with_named(inner, qualified)?)),
        },
        TypeRef::Option(inner) => Ok(format!(
            "{} | undefined",
            ts_type_with_named(inner, qualified)?
        )),
        TypeRef::Tuple(items) => {
            if items.is_empty() {
                Ok("undefined".to_string())
            } else {
                Ok(format!(
                    "[{}]",
                    items
                        .iter()
                        .map(|item| ts_type_with_named(item, qualified))
                        .collect::<Result<Vec<_>>>()?
                        .join(", ")
                ))
            }
        }
        TypeRef::Array(inner, _len) => match inner.as_ref() {
            TypeRef::Primitive(name) if name == "u8" => Ok("Uint8Array".to_string()),
            _ => Ok(format!("Array<{}>", ts_type_with_named(inner, qualified)?)),
        },
        TypeRef::Generic(name) => Ok(name.clone()),
        TypeRef::Unit => Ok("undefined".to_string()),
    }
}

fn ts_inner_option(ty: &TypeRef) -> Result<String> {
    ts_inner_option_with_named(ty, false)
}

fn ts_inner_option_with_named(ty: &TypeRef, qualified: bool) -> Result<String> {
    match ty {
        TypeRef::Option(inner) => ts_type_with_named(inner, qualified),
        other => ts_type_with_named(other, qualified),
    }
}

fn ts_type_qualified(ty: &TypeRef) -> Result<String> {
    ts_type_with_named(ty, true)
}

fn ts_field_name(name: &str, ty: &TypeRef) -> (String, bool) {
    let camel = to_camel_case(name);
    let optional = matches!(ty, TypeRef::Option(_));
    (camel, optional)
}

fn payload_type(params: &[ParamDef]) -> Result<String> {
    match params.len() {
        0 => Ok("undefined".to_string()),
        1 => ts_type_qualified(&params[0].type_ref),
        _ => Ok(format!(
            "[{}]",
            params
                .iter()
                .map(|param| ts_type_qualified(&param.type_ref))
                .collect::<Result<Vec<_>>>()?
                .join(", ")
        )),
    }
}

fn generic_param_declaration(params: &[String]) -> String {
    if params.is_empty() {
        String::new()
    } else {
        format!("<{}>", params.join(", "))
    }
}

fn top_level_type_name(name: &str, generic_params: &[String]) -> String {
    if generic_params.is_empty() {
        name.to_string()
    } else {
        format!("{}<{}>", name, generic_params.join(", "))
    }
}

fn codec_context(generic_params: &[String]) -> CodecContext {
    let generic_codecs = generic_params
        .iter()
        .map(|param| (param.clone(), codec_param_name(param)))
        .collect();
    CodecContext { generic_codecs }
}

fn codec_param_name(name: &str) -> String {
    format!("{}Codec", to_camel_case(name))
}

fn strip_prefix(name: &str) -> String {
    for prefix in ["host_", "remote_", "product_"] {
        if let Some(rest) = name.strip_prefix(prefix) {
            return rest.to_string();
        }
    }
    name.to_string()
}

fn to_camel_case(s: &str) -> String {
    let mut result = String::new();
    let mut capitalize_next = false;
    for (index, ch) in s.chars().enumerate() {
        if ch == '_' {
            capitalize_next = true;
        } else if capitalize_next {
            result.push(ch.to_uppercase().next().unwrap_or(ch));
            capitalize_next = false;
        } else if index == 0 {
            result.push(ch.to_lowercase().next().unwrap_or(ch));
        } else {
            result.push(ch);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request_method(name: &str, wire_id: Option<u8>) -> MethodDef {
        MethodDef {
            name: name.to_string(),
            kind: MethodKind::Request,
            params: Vec::new(),
            return_type: ReturnType::Result {
                ok: TypeRef::Unit,
                err: TypeRef::Unit,
            },
            wire_id,
            docs: None,
        }
    }

    fn subscription_method(name: &str, wire_id: Option<u8>) -> MethodDef {
        MethodDef {
            name: name.to_string(),
            kind: MethodKind::Subscription,
            params: Vec::new(),
            return_type: ReturnType::Subscription(TypeRef::Unit),
            wire_id,
            docs: None,
        }
    }

    fn api(methods: Vec<MethodDef>) -> ApiDefinition {
        ApiDefinition {
            traits: vec![TraitDef {
                name: "Example".to_string(),
                methods,
                docs: None,
            }],
            types: Vec::new(),
        }
    }

    fn named_type(name: &str) -> TypeRef {
        TypeRef::Named {
            name: name.to_string(),
            args: Vec::new(),
        }
    }

    fn request_method_with_wrappers(
        name: &str,
        wire_id: Option<u8>,
        request: &str,
        response: &str,
        error: &str,
    ) -> MethodDef {
        MethodDef {
            name: name.to_string(),
            kind: MethodKind::Request,
            params: vec![ParamDef {
                name: "request".to_string(),
                type_ref: named_type(request),
            }],
            return_type: ReturnType::Result {
                ok: named_type(response),
                err: named_type(error),
            },
            wire_id,
            docs: None,
        }
    }

    fn subscription_method_with_wrappers(name: &str, wire_id: Option<u8>, item: &str) -> MethodDef {
        MethodDef {
            name: name.to_string(),
            kind: MethodKind::Subscription,
            params: Vec::new(),
            return_type: ReturnType::Subscription(named_type(item)),
            wire_id,
            docs: None,
        }
    }

    fn versioned_tuple_wrapper_variants(name: &str, variants: &[(u32, &str)]) -> TypeDef {
        TypeDef {
            name: name.to_string(),
            generic_params: Vec::new(),
            kind: TypeDefKind::Enum(
                variants
                    .iter()
                    .map(|(version, inner)| VariantDef {
                        name: format!("V{version}"),
                        fields: VariantFields::Unnamed(vec![named_type(inner)]),
                        docs: None,
                    })
                    .collect(),
            ),
            docs: None,
        }
    }

    fn versioned_tuple_wrapper(name: &str, legacy: &str, latest: &str) -> TypeDef {
        versioned_tuple_wrapper_variants(name, &[(1, legacy), (2, latest)])
    }

    fn named_field_versioned_wrapper(name: &str) -> TypeDef {
        let fields = vec![
            FieldDef {
                name: "product_account_id".to_string(),
                type_ref: TypeRef::Named {
                    name: "ProductAccountId".to_string(),
                    args: Vec::new(),
                },
                docs: None,
            },
            FieldDef {
                name: "context".to_string(),
                type_ref: TypeRef::Named {
                    name: "Bytes".to_string(),
                    args: Vec::new(),
                },
                docs: None,
            },
        ];
        TypeDef {
            name: name.to_string(),
            generic_params: Vec::new(),
            kind: TypeDefKind::Enum(vec![
                VariantDef {
                    name: "V1".to_string(),
                    fields: VariantFields::Named(fields.clone()),
                    docs: None,
                },
                VariantDef {
                    name: "V2".to_string(),
                    fields: VariantFields::Named(fields),
                    docs: None,
                },
            ]),
            docs: None,
        }
    }

    #[test]
    fn detect_versioned_wrapper_keeps_each_versioned_variant() {
        let ty = TypeDef {
            name: "ExampleRequest".to_string(),
            generic_params: Vec::new(),
            kind: TypeDefKind::Enum(vec![
                VariantDef {
                    name: "V1".to_string(),
                    fields: VariantFields::Unnamed(vec![TypeRef::Named {
                        name: "LegacyRequest".to_string(),
                        args: Vec::new(),
                    }]),
                    docs: None,
                },
                VariantDef {
                    name: "V10".to_string(),
                    fields: VariantFields::Unnamed(vec![TypeRef::Named {
                        name: "LatestRequest".to_string(),
                        args: Vec::new(),
                    }]),
                    docs: None,
                },
                VariantDef {
                    name: "V2".to_string(),
                    fields: VariantFields::Unnamed(vec![TypeRef::Named {
                        name: "IntermediateRequest".to_string(),
                        args: Vec::new(),
                    }]),
                    docs: None,
                },
            ]),
            docs: None,
        };

        let wrapper = detect_versioned_wrapper(&ty).expect("versioned wrapper");
        let legacy = wrapper.variants.get(&1).expect("V1 variant");
        let fallback = wrapper
            .variants
            .range(..=9)
            .next_back()
            .map(|(_, variant)| variant)
            .expect("V2 fallback");
        let latest = wrapper.variants.get(&10).expect("V10 variant");

        match &legacy.kind {
            VersionedKind::Tuple(TypeRef::Named { name, .. }) => {
                assert_eq!(name, "LegacyRequest");
            }
            other => panic!("unexpected wrapper kind: {other:?}"),
        }

        match &latest.kind {
            VersionedKind::Tuple(TypeRef::Named { name, .. }) => {
                assert_eq!(name, "LatestRequest");
            }
            other => panic!("unexpected wrapper kind: {other:?}"),
        }

        match &fallback.kind {
            VersionedKind::Tuple(TypeRef::Named { name, .. }) => {
                assert_eq!(name, "IntermediateRequest");
            }
            other => panic!("unexpected wrapper kind: {other:?}"),
        }
    }

    #[test]
    fn generate_wire_table_emits_sorted_typescript_entries() {
        let source = generate_wire_table(
            &api(vec![
                request_method("later", Some(10)),
                subscription_method("stream", Some(2)),
            ]),
            2,
        )
        .expect("generate wire table");

        assert!(source.contains("export const WIRE_TABLE"));
        assert!(source.contains("  [2, 'stream_start'],"));
        assert!(source.contains("  [5, 'stream_receive'],"));
        assert!(source.contains("  [10, 'later_request'],"));
        assert!(
            source.find("[2, 'stream_start']").expect("stream entry")
                < source.find("[10, 'later_request']").expect("later entry")
        );
    }

    #[test]
    fn generate_wire_table_rejects_duplicate_ids() {
        let err = generate_wire_table(
            &api(vec![
                request_method("first", Some(2)),
                subscription_method("second", Some(3)),
            ]),
            2,
        )
        .expect_err("duplicate ids must error");

        assert!(err.to_string().contains("wire id 3 reused"));
    }

    #[test]
    fn generate_wire_table_rejects_missing_annotation() {
        let err = generate_wire_table(&api(vec![request_method("missing", None)]), 2)
            .expect_err("missing wire id must error");

        assert!(err.to_string().contains("missing #[wire(id = N)]"));
    }

    #[test]
    fn generate_wire_table_filters_methods_by_target_version() {
        let api = ApiDefinition {
            traits: vec![TraitDef {
                name: "Example".to_string(),
                methods: vec![
                    request_method_with_wrappers(
                        "legacy",
                        Some(2),
                        "LegacyRequest",
                        "LegacyResponse",
                        "LegacyError",
                    ),
                    request_method_with_wrappers(
                        "future",
                        Some(10),
                        "FutureRequest",
                        "FutureResponse",
                        "FutureError",
                    ),
                    subscription_method_with_wrappers("future_stream", Some(20), "FutureItem"),
                ],
                docs: None,
            }],
            types: vec![
                versioned_tuple_wrapper_variants("LegacyRequest", &[(1, "LegacyRequestV1")]),
                versioned_tuple_wrapper_variants("LegacyResponse", &[(1, "LegacyResponseV1")]),
                versioned_tuple_wrapper_variants("LegacyError", &[(1, "LegacyErrorV1")]),
                versioned_tuple_wrapper_variants("FutureRequest", &[(2, "FutureRequestV2")]),
                versioned_tuple_wrapper_variants("FutureResponse", &[(2, "FutureResponseV2")]),
                versioned_tuple_wrapper_variants("FutureError", &[(2, "FutureErrorV2")]),
                versioned_tuple_wrapper_variants("FutureItem", &[(2, "FutureItemV2")]),
            ],
        };

        let source = generate_wire_table(&api, 1).expect("generate wire table");

        assert!(source.contains("  [2, 'legacy_request'],"));
        assert!(!source.contains("future_request"));
        assert!(!source.contains("future_stream_start"));
    }

    #[test]
    fn generate_client_filters_empty_services_by_target_version() {
        let api = ApiDefinition {
            traits: vec![
                TraitDef {
                    name: "Legacy".to_string(),
                    methods: vec![request_method_with_wrappers(
                        "legacy_call",
                        Some(2),
                        "LegacyRequest",
                        "LegacyResponse",
                        "LegacyError",
                    )],
                    docs: None,
                },
                TraitDef {
                    name: "FutureOnly".to_string(),
                    methods: vec![request_method_with_wrappers(
                        "future_call",
                        Some(4),
                        "FutureRequest",
                        "FutureResponse",
                        "FutureError",
                    )],
                    docs: None,
                },
            ],
            types: vec![
                versioned_tuple_wrapper_variants("LegacyRequest", &[(1, "LegacyRequestV1")]),
                versioned_tuple_wrapper_variants("LegacyResponse", &[(1, "LegacyResponseV1")]),
                versioned_tuple_wrapper_variants("LegacyError", &[(1, "LegacyErrorV1")]),
                versioned_tuple_wrapper_variants("FutureRequest", &[(2, "FutureRequestV2")]),
                versioned_tuple_wrapper_variants("FutureResponse", &[(2, "FutureResponseV2")]),
                versioned_tuple_wrapper_variants("FutureError", &[(2, "FutureErrorV2")]),
            ],
        };

        let source = generate_client(&api, 1, 1).expect("generate client");

        assert!(source.contains("export const TRUAPI_VERSION = 1 as const;"));
        assert!(source.contains("export const TRUAPI_CODEC_VERSION = 1 as const;"));
        assert!(source.contains("export class LegacyClient"));
        assert!(source.contains("legacyCall("));
        assert!(!source.contains("FutureOnlyClient"));
        assert!(!source.contains("futureCall("));
    }

    #[test]
    fn generate_client_selects_lowest_shared_wrapper_variant() {
        let api = ApiDefinition {
            traits: vec![TraitDef {
                name: "Example".to_string(),
                methods: vec![MethodDef {
                    name: "example_call".to_string(),
                    kind: MethodKind::Request,
                    params: vec![ParamDef {
                        name: "request".to_string(),
                        type_ref: TypeRef::Named {
                            name: "ExampleRequest".to_string(),
                            args: Vec::new(),
                        },
                    }],
                    return_type: ReturnType::Result {
                        ok: TypeRef::Named {
                            name: "ExampleResponse".to_string(),
                            args: Vec::new(),
                        },
                        err: TypeRef::Unit,
                    },
                    wire_id: Some(2),
                    docs: None,
                }],
                docs: None,
            }],
            types: vec![
                versioned_tuple_wrapper("ExampleRequest", "LegacyRequest", "LatestRequest"),
                versioned_tuple_wrapper("ExampleResponse", "LegacyResponse", "LatestResponse"),
            ],
        };

        let client_source = generate_client(&api, 2, 1).expect("generate client");

        // V1 is the lowest variant supported by every wrapper at or below the
        // target version. The codegen prefers V1 so the wire payload is
        // decodable by legacy hosts that only register `v1`.
        assert!(client_source.contains("request: T.LegacyRequest"));
        assert!(client_source
            .contains("payload: T.ExampleRequest.enc({ tag: \"V1\", value: request }),"));
        assert!(client_source.contains("Promise<Result<T.LegacyResponse, undefined>>"));
    }

    #[test]
    fn generate_client_uses_only_existing_wrapper_variant() {
        let api = ApiDefinition {
            traits: vec![TraitDef {
                name: "Example".to_string(),
                methods: vec![MethodDef {
                    name: "example_call".to_string(),
                    kind: MethodKind::Request,
                    params: vec![ParamDef {
                        name: "request".to_string(),
                        type_ref: TypeRef::Named {
                            name: "ExampleRequest".to_string(),
                            args: Vec::new(),
                        },
                    }],
                    return_type: ReturnType::Result {
                        ok: TypeRef::Named {
                            name: "ExampleResponse".to_string(),
                            args: Vec::new(),
                        },
                        err: TypeRef::Unit,
                    },
                    wire_id: Some(2),
                    docs: None,
                }],
                docs: None,
            }],
            types: vec![
                versioned_tuple_wrapper_variants("ExampleRequest", &[(1, "LegacyRequest")]),
                versioned_tuple_wrapper("ExampleResponse", "LegacyResponse", "LatestResponse"),
            ],
        };

        let client_source = generate_client(&api, 2, 1).expect("generate client");

        assert!(client_source.contains("request: T.LegacyRequest"));
        assert!(client_source
            .contains("payload: T.ExampleRequest.enc({ tag: \"V1\", value: request }),"));
        assert!(client_source.contains("Promise<Result<T.LegacyResponse, undefined>>"));
    }

    #[test]
    fn generate_client_rejects_named_field_versioned_wrapper() {
        let api = ApiDefinition {
            traits: vec![TraitDef {
                name: "Example".to_string(),
                methods: vec![MethodDef {
                    name: "example_call".to_string(),
                    kind: MethodKind::Request,
                    params: vec![ParamDef {
                        name: "request".to_string(),
                        type_ref: TypeRef::Named {
                            name: "ExampleRequest".to_string(),
                            args: Vec::new(),
                        },
                    }],
                    return_type: ReturnType::Result {
                        ok: TypeRef::Named {
                            name: "ExampleResponse".to_string(),
                            args: Vec::new(),
                        },
                        err: TypeRef::Unit,
                    },
                    wire_id: Some(2),
                    docs: None,
                }],
                docs: None,
            }],
            types: vec![
                named_field_versioned_wrapper("ExampleRequest"),
                versioned_tuple_wrapper("ExampleResponse", "LegacyResponse", "LatestResponse"),
            ],
        };

        let err = generate_client(&api, 2, 1).expect_err("named field wrapper rejected");

        assert!(err.to_string().contains("uses named fields"));
    }
}
