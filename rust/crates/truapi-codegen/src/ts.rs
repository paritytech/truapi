//! TypeScript code generation from extracted API definitions.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt::Write;
use std::fs;
use std::path::Path;

use anyhow::{Result, bail};
use convert_case::{Case, Casing};
use indoc::{formatdoc, writedoc};

use crate::rustdoc::*;

mod examples;
mod playground;

pub use examples::generate_client_examples;
pub use playground::generate_playground_services;

#[derive(Default)]
struct CodecContext {
    generic_codecs: HashMap<String, String>,
}

/// How a `TypeRef::Named` resolves its name when rendered to TS.
///
/// - `Public` strips the V0N prefix via `public_versioned_type_name` and
///   qualifies every named type with `T.*`. Used by the client/playground/
///   examples generators that emit version-aliased public names (e.g.
///   `T.HostAccountGetRequest`).
/// - `RawLocal` preserves the V0N prefix and emits V0N-prefixed types
///   **bare** (no namespace prefix) because they live in the same file as
///   the emission. Used for `types-by-version.ts`.
/// - `HostServer` preserves the V0N prefix and splits the namespace:
///   V0N-prefixed types resolve to `V.V0NXxx` (the `types-by-version.ts`
///   namespace), other names resolve to `T.Xxx` (the `@parity/truapi`
///   namespace). Used by `server.ts` so the host generator does not need
///   a trivial `types.ts` aggregator file.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum NameMode {
    #[default]
    Public,
    RawLocal,
    HostServer,
}

fn resolve_named(name: &str, mode: NameMode) -> String {
    match mode {
        NameMode::Public => public_versioned_type_name(name),
        NameMode::RawLocal | NameMode::HostServer => name.to_string(),
    }
}

/// Decide how to namespace a resolved type name for `qualified` rendering.
/// `Public` prefixes every name with `T.*`. `RawLocal` leaves V0N-prefixed
/// names bare because they are defined in the same file as the current
/// emission (`types-by-version.ts`). `HostServer` splits the namespace
/// between `V.*` (V0N-prefixed) and `T.*` (everything else).
fn qualify_named(resolved: &str, mode: NameMode) -> String {
    match mode {
        NameMode::Public => format!("T.{resolved}"),
        NameMode::RawLocal => {
            if version_prefixed_type(resolved).is_some() {
                resolved.to_string()
            } else {
                format!("T.{resolved}")
            }
        }
        NameMode::HostServer => {
            if version_prefixed_type(resolved).is_some() {
                format!("V.{resolved}")
            } else {
                format!("T.{resolved}")
            }
        }
    }
}

#[derive(Debug, Clone)]
struct PublicService<'a> {
    trait_def: &'a TraitDef,
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

fn versioned_wrapper_ts_name(name: &str) -> String {
    format!("Versioned{name}")
}

fn version_prefixed_type(name: &str) -> Option<(u32, &str)> {
    let rest = name.strip_prefix('V')?;
    if rest.len() < 3 {
        return None;
    }
    let (version, base) = rest.split_at(2);
    if base.is_empty() {
        return None;
    }
    Some((version.parse().ok()?, base))
}

fn public_versioned_type_name(name: &str) -> String {
    version_prefixed_type(name)
        .map(|(_, base)| base.to_string())
        .unwrap_or_else(|| name.to_string())
}

fn selected_public_aliases(
    api: &ApiDefinition,
    wrappers: &HashMap<String, VersionedWrapper>,
    emit_versions: &HashMap<String, BTreeSet<u32>>,
    target_version: u32,
) -> BTreeMap<String, String> {
    let mut selected_by_base: BTreeMap<String, (u32, String)> = BTreeMap::new();
    for (wrapper_name, versions) in emit_versions {
        let Some(wrapper) = wrappers.get(wrapper_name) else {
            continue;
        };
        for version in versions {
            let Some(variant) = wrapper.variants.get(version) else {
                continue;
            };
            let VersionedKind::Tuple(TypeRef::Named { name, args }) = &variant.kind else {
                continue;
            };
            if !args.is_empty() {
                continue;
            }
            let Some((inner_version, base)) = version_prefixed_type(name) else {
                continue;
            };
            selected_by_base.insert(base.to_string(), (inner_version, name.clone()));
        }
    }

    for ty in &api.types {
        let Some((version, base)) = version_prefixed_type(&ty.name) else {
            continue;
        };
        if version > target_version {
            continue;
        }
        if selected_by_base.contains_key(base) {
            continue;
        }
        let entry = selected_by_base
            .entry(base.to_string())
            .or_insert((version, ty.name.clone()));
        if version > entry.0 {
            *entry = (version, ty.name.clone());
        }
    }

    selected_by_base
        .into_iter()
        .map(|(base, (_, original))| (original, base))
        .collect()
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

/// Return the highest protocol version exposed by any versioned wrapper in
/// `api`, falling back to `1` if the API has none. Used as the default for
/// the client target version when the caller did not pass `--client-version`,
/// so an unconfigured codegen run produces a client that speaks the latest
/// wire format the Rust trait surface has shipped.
pub fn latest_wire_version(api: &ApiDefinition) -> u32 {
    collect_versioned_wrappers(api)
        .values()
        .flat_map(|wrapper| wrapper.variants.keys().copied())
        .max()
        .unwrap_or(1)
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
    if let TypeRef::Named { name, args } = ty
        && args.is_empty()
        && let Some(wrapper) = wrappers.get(name)
    {
        return Some((name.as_str(), wrapper));
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
    let text = strip_playground_doc_blocks(text);
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
        writeln!(out, "{indent}/** {line} */", line = lines[0]).unwrap();
        return;
    }
    writeln!(out, "{indent}/**").unwrap();
    for line in &lines {
        if line.is_empty() {
            writeln!(out, "{indent} *").unwrap();
        } else {
            writeln!(out, "{indent} * {line}").unwrap();
        }
    }
    writeln!(out, "{indent} */").unwrap();
}

fn strip_playground_doc_blocks(text: &str) -> String {
    let mut out = Vec::new();
    let mut in_typescript_doc_block = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if is_typescript_doc_block_start(trimmed) {
            in_typescript_doc_block = true;
            continue;
        }
        if in_typescript_doc_block && trimmed == "```" {
            in_typescript_doc_block = false;
            continue;
        }
        if !in_typescript_doc_block {
            out.push(line);
        }
    }
    trim_doc_lines(&out).unwrap_or_default()
}

fn is_typescript_doc_block_start(trimmed: &str) -> bool {
    trimmed == "```ts"
}

fn public_services(api: &ApiDefinition) -> Result<Vec<PublicService<'_>>> {
    let trait_defs = api
        .traits
        .iter()
        .map(|trait_def| (trait_def.name.as_str(), trait_def))
        .collect::<HashMap<_, _>>();

    let mut services = Vec::new();
    for name in &api.public_trait_order {
        let Some(trait_def) = trait_defs.get(name.as_str()).copied() else {
            bail!("trait `{name}` appears in `TrUApi` but was not extracted");
        };
        services.push(PublicService { trait_def });
    }

    Ok(services)
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

    let types_code = generate_types(api, target_version)?;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExpandedWireIds {
    Request {
        request_id: u8,
        response_id: u8,
    },
    Subscription {
        start_id: u8,
        stop_id: u8,
        interrupt_id: u8,
        receive_id: u8,
    },
}

impl ExpandedWireIds {
    fn sort_id(self) -> u8 {
        match self {
            ExpandedWireIds::Request { request_id, .. } => request_id,
            ExpandedWireIds::Subscription { start_id, .. } => start_id,
        }
    }

    fn entries(self, method_name: &str) -> Vec<(u8, String)> {
        match self {
            ExpandedWireIds::Request {
                request_id,
                response_id,
            } => vec![
                (request_id, format!("{method_name}_request")),
                (response_id, format!("{method_name}_response")),
            ],
            ExpandedWireIds::Subscription {
                start_id,
                stop_id,
                interrupt_id,
                receive_id,
            } => vec![
                (start_id, format!("{method_name}_start")),
                (stop_id, format!("{method_name}_stop")),
                (interrupt_id, format!("{method_name}_interrupt")),
                (receive_id, format!("{method_name}_receive")),
            ],
        }
    }
}

fn trim_doc_lines(lines: &[&str]) -> Option<String> {
    let mut start = 0;
    let mut end = lines.len();
    while start < end && lines[start].trim().is_empty() {
        start += 1;
    }
    while end > start && lines[end - 1].trim().is_empty() {
        end -= 1;
    }
    if start == end {
        return None;
    }
    Some(
        lines[start..end]
            .iter()
            .map(|line| line.strip_prefix(' ').unwrap_or(line).trim_end())
            .collect::<Vec<_>>()
            .join("\n"),
    )
}

fn ts_string_literal(value: &str) -> String {
    serde_json::to_string(value).expect("string serialization is infallible")
}

fn wire_const_name(trait_name: &str, method_name: &str) -> String {
    format!("{trait_name}_{method_name}").to_case(Case::UpperSnake)
}

/// Sort key for stable, wire-id-ordered method emission shared by the
/// playground and examples submodules.
fn method_wire_sort_id(method: &MethodDef) -> u8 {
    method
        .wire
        .request_id
        .or(method.wire.start_id)
        .unwrap_or(u8::MAX)
}

fn generate_wire_table(api: &ApiDefinition, target_version: u32) -> Result<String> {
    let wrappers = collect_versioned_wrappers(api);
    let mut seen: BTreeMap<u8, String> = BTreeMap::new();
    let mut constants: Vec<(String, ExpandedWireIds)> = Vec::new();

    for trait_def in &api.traits {
        for method in &trait_def.methods {
            if !method_is_included(trait_def, method, &wrappers, target_version)? {
                continue;
            }
            let wire_ids = wire_ids_for_method(trait_def, method)?;
            for (id, tag) in wire_ids.entries(&method.name) {
                if let Some(existing) = seen.insert(id, tag.clone()) {
                    bail!("wire id {id} reused: `{existing}` and `{tag}` collide");
                }
            }
            constants.push((wire_const_name(&trait_def.name, &method.name), wire_ids));
        }
    }

    constants.sort_by_key(|(_, ids)| ids.sort_id());

    let mut out = String::new();
    writedoc!(
        out,
        r#"
        // Auto-generated by truapi-codegen. Do not edit.

        import type {{ RequestFrameIds, SubscriptionFrameIds }} from '../transport.js';

        // Wire-protocol discriminants. Method ordering is part of the
        // protocol; only ever append or explicitly reserve gaps.
        "#
    )
    .unwrap();
    for (name, ids) in constants {
        match ids {
            ExpandedWireIds::Request {
                request_id,
                response_id,
            } => {
                out.push('\n');
                out.push_str(&formatdoc! {"
                    export const {name} = {{
                      request: {request_id},
                      response: {response_id},
                    }} as const satisfies RequestFrameIds;
                "});
            }
            ExpandedWireIds::Subscription {
                start_id,
                stop_id,
                interrupt_id,
                receive_id,
            } => {
                out.push('\n');
                out.push_str(&formatdoc! {"
                    export const {name} = {{
                      start: {start_id},
                      stop: {stop_id},
                      interrupt: {interrupt_id},
                      receive: {receive_id},
                    }} as const satisfies SubscriptionFrameIds;
                "});
            }
        }
    }

    Ok(out)
}

fn method_is_included(
    trait_def: &TraitDef,
    method: &MethodDef,
    wrappers: &HashMap<String, VersionedWrapper>,
    target_version: u32,
) -> Result<bool> {
    wire_ids_for_method(trait_def, method)?;

    let wrapper_names = method_versioned_wrappers(method, wrappers);
    Ok(
        wrapper_names.is_empty()
            || method_wire_version(method, wrappers, target_version)?.is_some(),
    )
}

fn wire_ids_for_method(trait_def: &TraitDef, method: &MethodDef) -> Result<ExpandedWireIds> {
    let wire = &method.wire;
    match method.kind {
        MethodKind::Request => {
            if wire.start_id.is_some()
                || wire.stop_id.is_some()
                || wire.interrupt_id.is_some()
                || wire.receive_id.is_some()
            {
                bail!(
                    "method `{}::{}` is a request and must not use subscription wire ids",
                    trait_def.name,
                    method.name
                );
            }
            let request_id = wire.request_id.ok_or_else(|| {
                anyhow::anyhow!(
                    "method `{}::{}` is missing #[wire(request_id = N)] annotation",
                    trait_def.name,
                    method.name
                )
            })?;
            let response_id =
                infer_wire_id(wire.response_id, request_id, 1, &method.name, "response_id")?;
            Ok(ExpandedWireIds::Request {
                request_id,
                response_id,
            })
        }
        MethodKind::Subscription | MethodKind::ResultSubscription => {
            if wire.request_id.is_some() || wire.response_id.is_some() {
                bail!(
                    "method `{}::{}` is a subscription and must not use request wire ids",
                    trait_def.name,
                    method.name
                );
            }
            let start_id = wire.start_id.ok_or_else(|| {
                anyhow::anyhow!(
                    "method `{}::{}` is missing #[wire(start_id = N)] annotation",
                    trait_def.name,
                    method.name
                )
            })?;
            let stop_id = infer_wire_id(wire.stop_id, start_id, 1, &method.name, "stop_id")?;
            let interrupt_id =
                infer_wire_id(wire.interrupt_id, start_id, 2, &method.name, "interrupt_id")?;
            let receive_id =
                infer_wire_id(wire.receive_id, start_id, 3, &method.name, "receive_id")?;
            Ok(ExpandedWireIds::Subscription {
                start_id,
                stop_id,
                interrupt_id,
                receive_id,
            })
        }
    }
}

fn infer_wire_id(
    explicit: Option<u8>,
    anchor_id: u8,
    offset: u8,
    method_name: &str,
    field_name: &str,
) -> Result<u8> {
    explicit.map_or_else(
        || {
            anchor_id.checked_add(offset).ok_or_else(|| {
                anyhow::anyhow!(
                    "wire id overflow on `{method_name}` while inferring `{field_name}` from {anchor_id}"
                )
            })
        },
        Ok,
    )
}

/// Picks the wrapper variant the generated client emits on the wire for a
/// given method. Returns the highest variant supported by every wrapper the
/// method touches and that is ≤ `target_version`. Returns `None` when no
/// shared variant exists at or below the cap (the method is not exposed by
/// the client).
///
/// Picking the **highest** variant exposes the newest request/response shape
/// the host is known to support. Hosts that only implement an older codec
/// version still receive a wire envelope they understand because every
/// wrapper keeps each `Vn` variant at `#[codec(index = n - 1)]`.
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

    Ok(candidates.and_then(|versions| versions.into_iter().max()))
}

/// For each versioned wrapper, the set of wire versions the generated client
/// actually emits. Each method picks one wire version via [`method_wire_version`];
/// every wrapper it touches gets that version recorded here. Wrappers that no
/// included method references end up absent from the map and can be elided
/// from the emitted types altogether.
fn versioned_wrapper_emit_versions(
    api: &ApiDefinition,
    wrappers: &HashMap<String, VersionedWrapper>,
    target_version: u32,
) -> Result<HashMap<String, BTreeSet<u32>>> {
    let mut emit: HashMap<String, BTreeSet<u32>> = HashMap::new();
    for trait_def in &api.traits {
        for method in &trait_def.methods {
            if !method_is_included(trait_def, method, wrappers, target_version)? {
                continue;
            }
            let Some(wire_version) = method_wire_version(method, wrappers, target_version)? else {
                continue;
            };
            for wrapper_name in method_versioned_wrappers(method, wrappers) {
                emit.entry(wrapper_name).or_default().insert(wire_version);
            }
        }
    }
    Ok(emit)
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
        ReturnType::ResultSubscription { item, err } => {
            collect_type_versioned_wrappers(item, wrappers, &mut names);
            collect_type_versioned_wrappers(
                call_error_inner(err).unwrap_or(err),
                wrappers,
                &mut names,
            );
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

fn generate_types(api: &ApiDefinition, target_version: u32) -> Result<String> {
    let mut out = String::new();
    writedoc!(
        out,
        r#"
        // Auto-generated by truapi-codegen. Do not edit.

        import * as S from '../scale.js';
        import type {{ HexString }} from '../scale.js';

        "#
    )
    .unwrap();

    let wrappers = collect_versioned_wrappers(api);
    let emit_versions = versioned_wrapper_emit_versions(api, &wrappers, target_version)?;
    let aliases = selected_public_aliases(api, &wrappers, &emit_versions, target_version);

    for ty in &api.types {
        if version_prefixed_type(&ty.name).is_some() && !aliases.contains_key(&ty.name) {
            continue;
        }
        write_type_definition(&mut out, ty, &emit_versions, &aliases)?;
        writeln!(out).unwrap();
        write_codec_definition(&mut out, ty, &emit_versions, &aliases)?;
        writeln!(out).unwrap();
    }

    Ok(out)
}

fn generate_client(api: &ApiDefinition, target_version: u32, codec_version: u8) -> Result<String> {
    validate_versioned_wrapper_shapes(api)?;

    let mut out = String::new();
    writedoc!(
        out,
        r#"
        // Auto-generated by truapi-codegen. Do not edit.

        import {{ ResultAsync, type Result }} from 'neverthrow';
        import * as S from '../scale.js';
        import type {{ HexString }} from '../scale.js';
        import {{ SubscriptionError }} from '../transport.js';
        import type {{ ObservableLike, Observer, Subscription, SubscriptionFrameIds, TrUApiTransport }} from '../transport.js';
        import * as T from './types.js';
        import * as W from './wire-table.js';

        export {{ ResultAsync, SubscriptionError }};
        export type {{ ObservableLike, Observer, Result, Subscription, TrUApiTransport }};
        export const TRUAPI_VERSION = {target_version} as const;
        export const TRUAPI_CODEC_VERSION = {codec_version} as const;

        function toSubscriptionError<Reason = never>(error: unknown): SubscriptionError<Reason> {{
          if (error instanceof SubscriptionError) return error as SubscriptionError<Reason>;
          const cause = error instanceof Error ? error : new Error(String(error));
          return new SubscriptionError(cause.message, {{ cause }});
        }}

        "#
    )
    .unwrap();
    write_observable_helper(&mut out);

    let ctx = CodecContext::default();
    let wrappers = collect_versioned_wrappers(api);
    let services = public_services(api)?;

    for service in &services {
        let trait_def = service.trait_def;
        let methods = included_methods(trait_def, &wrappers, target_version)?;
        if methods.is_empty() {
            continue;
        }

        write_jsdoc(&mut out, "", trait_def.docs.as_deref());
        writedoc!(
            out,
            "
            export class {name}Client {{
              constructor(private readonly transport: TrUApiTransport) {{}}

            ",
            name = trait_def.name
        )
        .unwrap();

        for method in methods {
            emit_method(&mut out, trait_def, method, &wrappers, &ctx, target_version)?;
            writeln!(out).unwrap();
        }

        writeln!(out, "}}\n").unwrap();
    }

    writeln!(out, "export interface TrUApiClient {{").unwrap();
    for service in &services {
        let trait_def = service.trait_def;
        if included_methods(trait_def, &wrappers, target_version)?.is_empty() {
            continue;
        }
        let field = to_camel_case(&trait_def.name);
        writeln!(
            out,
            "  readonly {field}: {name}Client;",
            name = trait_def.name
        )
        .unwrap();
    }
    writedoc!(
        out,
        r#"
        }}

        export type Client = TrUApiClient;

        export type GeneratedClientTransport = Omit<TrUApiTransport, "truapiVersion" | "codecVersion"> &
          Partial<Pick<TrUApiTransport, "truapiVersion" | "codecVersion">>;

        function withGeneratedTransportVersions(transport: GeneratedClientTransport): TrUApiTransport {{
          return {{
            ...transport,
            truapiVersion: transport.truapiVersion ?? TRUAPI_VERSION,
            codecVersion: transport.codecVersion ?? TRUAPI_CODEC_VERSION,
          }};
        }}

        /** Creates the generated client facade by binding each service namespace to the
         * shared transport instance. */
        export function createClient(transport: GeneratedClientTransport): TrUApiClient {{
          const versionedTransport = withGeneratedTransportVersions(transport);
          return {{
        "#
    )
    .unwrap();
    for service in &services {
        let trait_def = service.trait_def;
        if included_methods(trait_def, &wrappers, target_version)?.is_empty() {
            continue;
        }
        let field = to_camel_case(&trait_def.name);
        writeln!(
            out,
            "    {}: new {}Client(versionedTransport),",
            field, trait_def.name
        )
        .unwrap();
    }
    writedoc!(
        out,
        r#"
          }};
        }}
        "#
    )
    .unwrap();

    Ok(out)
}

fn write_observable_helper(out: &mut String) {
    writedoc!(
        out,
        r#"
        // ES Observable interop key (rxjs reads Symbol.observable, falling
        // back to "@@observable" on platforms without the well-known symbol).
        const OBSERVABLE_INTEROP: symbol | string =
          (typeof Symbol === "function" && (Symbol as {{ observable?: symbol }}).observable) ||
          "@@observable";

        function createObservable<Item, Reason = never>({{
          transport,
          ids,
          payload,
          decodeItem,
          decodeInterrupt,
        }}: {{
          transport: TrUApiTransport;
          ids: SubscriptionFrameIds;
          payload: Uint8Array;
          decodeItem: (payload: Uint8Array) => Item;
          decodeInterrupt?: (payload: Uint8Array) => Reason;
        }}): ObservableLike<Item, Reason> {{
          const observable: ObservableLike<Item, Reason> = {{
            subscribe(observer: Partial<Observer<Item, Reason>> = {{}}): Subscription {{
              let closed = false;
              let raw: Subscription | undefined;

              const fail = (error: unknown, stop = true) => {{
                if (closed) return;
                closed = true;
                try {{
                  if (stop) raw?.unsubscribe();
                }} finally {{
                  observer.error?.(toSubscriptionError<Reason>(error));
                }}
              }};

              raw = transport.subscribeRaw({{
                ids,
                payload,
                onReceive: (payload) => {{
                  if (closed) return;
                  try {{
                    observer.next?.(decodeItem(payload));
                  }} catch (error) {{
                    fail(error);
                  }}
                }},
                onInterrupt: (payload) => {{
                  if (closed) return;
                  if (decodeInterrupt) {{
                    let reason: unknown;
                    try {{
                      reason = decodeInterrupt(payload);
                    }} catch (error) {{
                      fail(error, false);
                      return;
                    }}
                    fail(new SubscriptionError("Subscription interrupted", {{ reason }}), false);
                    return;
                  }}
                  closed = true;
                  observer.complete?.();
                }},
                onClose: fail,
              }});

              return {{
                get subscriptionId() {{
                  return raw?.subscriptionId ?? "";
                }},
                unsubscribe: () => {{
                  if (closed) return;
                  closed = true;
                  raw?.unsubscribe();
                }},
              }};
            }},
            [OBSERVABLE_INTEROP as typeof Symbol.observable]() {{
              return observable;
            }},
          }};
          return observable;
        }}

        "#
    )
    .unwrap();
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
    let arg = match wire_version {
        Some(version) => format!("{{ tag: \"V{version}\", value: {value_expr} }}"),
        None => value_expr.to_string(),
    };
    writeln!(out, "{indent}payload: {codec_expr}.enc({arg}),").unwrap();
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
    // handing bytes to the transport. The host-side dispatcher decodes the
    // full wrapper (variant byte included) from the wire payload.
    if params.len() == 1
        && let Some((wrapper_name, wrapper)) = versioned_wrapper_for(&params[0].type_ref, wrappers)
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
                wire_codec_expr: format!("T.{}", versioned_wrapper_ts_name(wrapper_name)),
                wire_version: Some(wrapper.version),
            }),
            VersionedKind::Tuple(inner) => {
                let inner_ts = ts_type_qualified(inner)?;
                Ok(PayloadEmission {
                    param_list: format!("request: {inner_ts}"),
                    param_names: vec!["request".to_string()],
                    inner_type_ts: inner_ts,
                    value_expr: "request".to_string(),
                    wire_codec_expr: format!("T.{}", versioned_wrapper_ts_name(wrapper_name)),
                    wire_version: Some(wrapper.version),
                })
            }
        };
    }

    if params.is_empty() {
        // No-param methods (subscribe-with-no-start-payload, etc.) still need
        // a versioned envelope on the wire so legacy hosts that decode an
        // `Enum({v1: _void})` payload receive at least the version byte.
        let version = wire_version.unwrap_or(1);
        let wire_codec_expr =
            indexed_versioned_codec_expr(std::iter::once((version, "S._void".to_string())))?;
        return Ok(PayloadEmission {
            param_list: String::new(),
            param_names: Vec::new(),
            inner_type_ts: "undefined".to_string(),
            value_expr: "undefined".to_string(),
            wire_codec_expr,
            wire_version: Some(version),
        });
    }

    let inner_type_ts = payload_type(params)?;
    Ok(PayloadEmission {
        param_list: format!("request: {inner_type_ts}"),
        param_names: vec!["request".to_string()],
        inner_type_ts: inner_type_ts.clone(),
        value_expr: "request".to_string(),
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
    format!("{{ tag: \"V{version}\"; value: {inner_type} }} & {wire_type}")
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
                wire_type_ts: format!("T.{}", versioned_wrapper_ts_name(wrapper_name)),
                wire_codec_expr: format!("T.{}", versioned_wrapper_ts_name(wrapper_name)),
                inner_codec_expr: "S._void".to_string(),
            }),
            VersionedKind::Tuple(inner) => Ok(ResponseEmission {
                inner_type_ts: ts_type_qualified(inner)?,
                wire_type_ts: format!("T.{}", versioned_wrapper_ts_name(wrapper_name)),
                wire_codec_expr: format!("T.{}", versioned_wrapper_ts_name(wrapper_name)),
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
    versioned_kind_codec_expr_mode(kind, qualified, ctx, NameMode::Public)
}

fn versioned_kind_codec_expr_mode(
    kind: &VersionedKind,
    qualified: bool,
    ctx: &CodecContext,
    mode: NameMode,
) -> Result<String> {
    match kind {
        VersionedKind::Unit => Ok("S._void".to_string()),
        VersionedKind::Tuple(inner) => codec_expr_mode(inner, qualified, ctx, mode),
    }
}

/// Builds a `S.indexedTaggedUnion({...})` expression for versioned wrapper
/// variants. Each `V<N>` arm uses wire discriminant `N - 1`, matching the
/// Rust `#[codec(index = N - 1)]` annotation.
fn indexed_versioned_codec_expr(
    variants: impl IntoIterator<Item = (u32, String)>,
) -> Result<String> {
    let mut entries = Vec::new();
    for (version, codec) in variants {
        let index = version
            .checked_sub(1)
            .ok_or_else(|| anyhow::anyhow!("versioned wrapper uses invalid V0 variant"))?;
        entries.push(format!("V{version}: [{index}, {codec}] as const"));
    }
    Ok(format!(
        "S.indexedTaggedUnion({{{entries}}})",
        entries = entries.join(", ")
    ))
}

fn versioned_result_codec_expr(version: u32, ok_codec: &str, err_codec: &str) -> Result<String> {
    indexed_versioned_codec_expr([(version, format!("S.Result({ok_codec}, {err_codec})"))])
}

fn emit_method(
    out: &mut String,
    trait_def: &TraitDef,
    method: &MethodDef,
    wrappers: &HashMap<String, VersionedWrapper>,
    ctx: &CodecContext,
    target_version: u32,
) -> Result<()> {
    let ts_method_name = to_camel_case(&strip_prefix(&method.name));
    let wire_const = wire_const_name(&trait_def.name, &method.name);
    let wire_version = method_wire_version(method, wrappers, target_version)?;
    let payload = emit_payload(&method.params, wrappers, ctx, wire_version)?;
    write_jsdoc(out, "  ", method.docs.as_deref());

    match (&method.kind, &method.return_type) {
        (MethodKind::Request, ReturnType::Result { ok, err }) => {
            let is_handshake = trait_def.name == "System" && method.name == "handshake";
            let response = emit_response(ok, wrappers, ctx, wire_version)?;
            let error = emit_error_response(err, wrappers, ctx, wire_version)?;
            let response_codec = match wire_version {
                Some(version) => versioned_result_codec_expr(
                    version,
                    &response.inner_codec_expr,
                    &error.inner_codec_expr,
                )?,
                None => format!(
                    "S.Result({}, {})",
                    response.wire_codec_expr, error.wire_codec_expr
                ),
            };

            let arg_decl = if is_handshake || payload.param_list.is_empty() {
                String::new()
            } else {
                format!("request: {}", payload.inner_type_ts)
            };
            let request_expr = if is_handshake {
                "{ codecVersion: this.transport.codecVersion }"
            } else {
                &payload.value_expr
            };

            writedoc!(
                out,
                "
                  {ts_method_name}({arg_decl}): ResultAsync<{ok_type}, {err_type}> {{
                    return this.transport.request<{ok_type}, {err_type}>({{
                      ids: W.{wire_const},
                ",
                ok_type = response.inner_type_ts,
                err_type = error.inner_type_ts
            )
            .unwrap();
            write_payload_field(
                out,
                "      ",
                &payload.wire_codec_expr,
                payload.wire_version,
                request_expr,
            );
            let value_suffix = if wire_version.is_some() { ".value" } else { "" };
            writeln!(
                out,
                "      decodeResponse: (payload) => {response_codec}.dec(payload){value_suffix},"
            )
            .unwrap();
            writedoc!(
                out,
                "
                    }});
                  }}
                "
            )
            .unwrap();
        }
        (MethodKind::Subscription, ReturnType::Subscription(ty)) => {
            let response = emit_response(ty, wrappers, ctx, wire_version)?;
            emit_subscribe_method(
                out,
                &ts_method_name,
                &wire_const,
                &payload,
                &response,
                response.inner_type_ts.clone(),
                None,
                wire_version,
            )?;
        }
        (MethodKind::ResultSubscription, ReturnType::ResultSubscription { item, err }) => {
            let response = emit_response(item, wrappers, ctx, wire_version)?;
            let error = emit_error_response(err, wrappers, ctx, wire_version)?;
            emit_subscribe_method(
                out,
                &ts_method_name,
                &wire_const,
                &payload,
                &response,
                response.inner_type_ts.clone(),
                Some(error),
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

/// Emits a subscribe method body that returns an Observable-compatible object.
/// Payloadless `_interrupt` maps to `complete`; typed interrupt payloads map
/// to `error`.
#[allow(clippy::too_many_arguments)]
fn emit_subscribe_method(
    out: &mut String,
    ts_method_name: &str,
    wire_const: &str,
    payload: &PayloadEmission,
    response: &ResponseEmission,
    item_type_ts: String,
    err: Option<ResponseEmission>,
    wire_version: Option<u32>,
) -> Result<()> {
    let observable_args = match err.as_ref() {
        Some(err) => format!("{item_type_ts}, {}", err.inner_type_ts),
        None => item_type_ts.clone(),
    };
    let signature = if payload.param_list.is_empty() {
        format!("  {ts_method_name}(): ObservableLike<{observable_args}> {{")
    } else {
        format!(
            "  {}({{ {} }}: {{ {} }}): ObservableLike<{}> {{",
            ts_method_name,
            payload.param_names.join(", "),
            payload.param_list,
            observable_args
        )
    };

    writedoc!(
        out,
        "
        {signature}
            return createObservable<{observable_args}>({{
              transport: this.transport,
              ids: W.{wire_const},
        "
    )
    .unwrap();
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
    writeln!(out, "      decodeItem: (payload) => {item_value},").unwrap();
    if let Some(err) = err {
        let err_value = if let Some(version) = wire_version {
            versioned_value_expr(
                &format!("{}.dec(payload)", err.wire_codec_expr),
                &err.wire_type_ts,
                &err.inner_type_ts,
                version,
            )
        } else {
            format!("{}.dec(payload)", err.wire_codec_expr)
        };
        writeln!(out, "      decodeInterrupt: (payload) => {err_value},").unwrap();
    }
    writedoc!(
        out,
        "
            }});
          }}
        "
    )
    .unwrap();

    Ok(())
}

fn write_type_definition(
    out: &mut String,
    ty: &TypeDef,
    emit_versions: &HashMap<String, BTreeSet<u32>>,
    aliases: &BTreeMap<String, String>,
) -> Result<()> {
    let generic_decl = generic_param_declaration(&ty.generic_params);
    let emitted_name = if should_rename_wire_wrapper(ty, emit_versions, aliases) {
        versioned_wrapper_ts_name(&ty.name)
    } else if let Some(alias) = aliases.get(&ty.name) {
        alias.clone()
    } else {
        ty.name.clone()
    };

    write_jsdoc(out, "", ty.docs.as_deref());
    match &ty.kind {
        TypeDefKind::Alias(type_ref) => {
            writeln!(
                out,
                "export type {emitted_name}{generic_decl} = {};",
                ts_type(type_ref)?
            )
            .unwrap();
        }
        TypeDefKind::Struct(fields) => {
            writeln!(out, "export interface {emitted_name}{generic_decl} {{").unwrap();
            for field in fields {
                let (ts_name, optional) = ts_field_name(&field.name, &field.type_ref);
                write_jsdoc(out, "  ", field.docs.as_deref());
                if optional {
                    writeln!(out, "  {ts_name}?: {};", ts_inner_option(&field.type_ref)?).unwrap();
                } else {
                    writeln!(out, "  {ts_name}: {};", ts_type(&field.type_ref)?).unwrap();
                }
            }
            writeln!(out, "}}").unwrap();
        }
        TypeDefKind::TupleStruct(fields) => {
            writeln!(
                out,
                "export type {emitted_name}{generic_decl} = {};",
                unnamed_fields_type(fields)?
            )
            .unwrap();
        }
        TypeDefKind::Enum(variants) => {
            if is_unit_only_enum(ty) {
                writeln!(
                    out,
                    "export type {emitted_name}{generic_decl} = {};",
                    unit_enum_union_type(variants)?
                )
                .unwrap();
            } else {
                // For versioned wrappers, only emit the variant(s) the client
                // actually wire-encodes. The wire byte index is preserved by the
                // codec definition (`indexed_versioned_codec_expr`).
                let selected = emit_versions.get(&ty.name);
                writeln!(out, "export type {emitted_name}{generic_decl} =").unwrap();
                for variant in variants {
                    if let Some(versions) = selected {
                        let Some(version) = version_number(&variant.name) else {
                            continue;
                        };
                        if !versions.contains(&version) {
                            continue;
                        }
                    }
                    write_jsdoc(out, "  ", variant.docs.as_deref());
                    writeln!(out, "  | {}", enum_variant_ts_type(variant)?).unwrap();
                }
                writeln!(out, ";").unwrap();
            }
        }
    }

    Ok(())
}

fn write_codec_definition(
    out: &mut String,
    ty: &TypeDef,
    emit_versions: &HashMap<String, BTreeSet<u32>>,
    aliases: &BTreeMap<String, String>,
) -> Result<()> {
    if ty.generic_params.is_empty() {
        let ctx = CodecContext::default();
        if let Some(wrapper) = detect_versioned_wrapper(ty) {
            let selected = emit_versions.get(&ty.name);
            let emitted_name = if should_rename_wire_wrapper(ty, emit_versions, aliases) {
                versioned_wrapper_ts_name(&ty.name)
            } else if let Some(alias) = aliases.get(&ty.name) {
                alias.clone()
            } else {
                ty.name.clone()
            };
            let codec_expr = indexed_versioned_codec_expr(
                wrapper
                    .variants
                    .values()
                    .filter(|variant| {
                        selected.is_none_or(|versions| versions.contains(&variant.version))
                    })
                    .map(|variant| {
                        Ok((
                            variant.version,
                            versioned_kind_codec_expr(&variant.kind, false, &ctx)?,
                        ))
                    })
                    .collect::<Result<Vec<_>>>()?,
            )?;
            let type_name = top_level_type_name(&emitted_name, &ty.generic_params);
            writeln!(
                out,
                "export const {emitted_name}: S.Codec<{type_name}> = S.lazy((): S.Codec<{type_name}> => {codec_expr});",
            )
            .unwrap();
            return Ok(());
        }
        let emitted_name = aliases
            .get(&ty.name)
            .map(String::as_str)
            .unwrap_or(&ty.name);
        let type_name = top_level_type_name(emitted_name, &ty.generic_params);
        let codec_expr = type_codec_expr(ty, &type_name, &ctx)?;
        writeln!(
            out,
            "export const {emitted_name}: S.Codec<{type_name}> = S.lazy((): S.Codec<{type_name}> => {codec_expr});",
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
    let type_name = top_level_type_name(
        aliases
            .get(&ty.name)
            .map(String::as_str)
            .unwrap_or(&ty.name),
        &ty.generic_params,
    );

    if ty.name == "Component" {
        writedoc!(
            out,
            "
            /** Builds a codec for renderer components parameterized by the codec of their
             * `props` payload. */
            "
        )
        .unwrap();
    }
    let function_name = aliases
        .get(&ty.name)
        .map(String::as_str)
        .unwrap_or(&ty.name);
    let codec_body = type_codec_expr(ty, &type_name, &ctx)?;
    writedoc!(
        out,
        "
        export function {function_name}{generic_decl}({codec_params}): S.Codec<{type_name}> {{
          return S.lazy((): S.Codec<{type_name}> => {codec_body});
        }}
        ",
    )
    .unwrap();

    Ok(())
}

fn should_rename_wire_wrapper(
    ty: &TypeDef,
    emit_versions: &HashMap<String, BTreeSet<u32>>,
    aliases: &BTreeMap<String, String>,
) -> bool {
    detect_versioned_wrapper(ty).is_some()
        && (emit_versions.contains_key(&ty.name) || aliases.values().any(|alias| alias == &ty.name))
}

fn type_codec_expr(ty: &TypeDef, type_name: &str, ctx: &CodecContext) -> Result<String> {
    type_codec_expr_mode_qualified(ty, type_name, ctx, NameMode::Public, false)
}

fn type_codec_expr_mode_qualified(
    ty: &TypeDef,
    type_name: &str,
    ctx: &CodecContext,
    mode: NameMode,
    qualified: bool,
) -> Result<String> {
    match &ty.kind {
        TypeDefKind::Alias(type_ref) => codec_expr_mode(type_ref, qualified, ctx, mode),
        TypeDefKind::Struct(fields) => {
            struct_codec_expr_mode(fields, type_name, qualified, ctx, mode)
        }
        TypeDefKind::TupleStruct(fields) => {
            unnamed_fields_codec_expr_mode(fields, qualified, ctx, mode)
        }
        TypeDefKind::Enum(variants) => {
            if is_unit_only_enum(ty) {
                unit_enum_codec_expr(variants)
            } else {
                let variants = variants
                    .iter()
                    .map(|variant| {
                        Ok(format!(
                            "{}: {}",
                            variant.name,
                            variant_codec_expr_mode(&variant.fields, qualified, ctx, mode)?
                        ))
                    })
                    .collect::<Result<Vec<_>>>()?
                    .join(", ");
                Ok(format!("S.TaggedUnion({{{variants}}})"))
            }
        }
    }
}

fn is_unit_only_enum(ty: &TypeDef) -> bool {
    detect_versioned_wrapper(ty).is_none()
        && matches!(
            &ty.kind,
            TypeDefKind::Enum(variants)
                if !variants.is_empty()
                    && variants
                        .iter()
                        .all(|variant| matches!(variant.fields, VariantFields::Unit))
        )
}

fn unit_enum_union_type(variants: &[VariantDef]) -> Result<String> {
    Ok(variants
        .iter()
        .map(|variant| ts_string_literal(&variant.name))
        .collect::<Vec<_>>()
        .join(" | "))
}

fn unit_enum_codec_expr(variants: &[VariantDef]) -> Result<String> {
    Ok(format!(
        "S.Status({})",
        variants
            .iter()
            .map(|variant| ts_string_literal(&variant.name))
            .collect::<Vec<_>>()
            .join(", ")
    ))
}

fn unit_enum_summary(variants: &[VariantDef]) -> String {
    format!(
        "Enum values: {}",
        variants
            .iter()
            .map(|variant| variant.name.as_str())
            .collect::<Vec<_>>()
            .join(" / ")
    )
}

fn variant_value_type_mode(fields: &VariantFields, mode: NameMode) -> Result<String> {
    let qualified = matches!(mode, NameMode::RawLocal | NameMode::HostServer);
    match fields {
        VariantFields::Unit => Ok("undefined".to_string()),
        VariantFields::Unnamed(types) => unnamed_fields_type_mode(types, qualified, mode),
        VariantFields::Named(fields) => inline_object_type_mode(fields, qualified, mode),
    }
}

/// Renders the public TS type for a single enum variant. Unit variants mark
/// `value` optional (`value?: undefined`) so consumers can write
/// `{ tag: "X" }` while the codec round-trip (`{ tag, value: undefined }`)
/// still type-checks.
fn enum_variant_ts_type(variant: &VariantDef) -> Result<String> {
    enum_variant_ts_type_mode(variant, NameMode::Public)
}

fn enum_variant_ts_type_mode(variant: &VariantDef, mode: NameMode) -> Result<String> {
    Ok(match &variant.fields {
        VariantFields::Unit => format!("{{ tag: \"{}\"; value?: undefined }}", variant.name),
        fields => format!(
            "{{ tag: \"{}\"; value: {} }}",
            variant.name,
            variant_value_type_mode(fields, mode)?
        ),
    })
}

fn variant_codec_expr_mode(
    fields: &VariantFields,
    qualified: bool,
    ctx: &CodecContext,
    mode: NameMode,
) -> Result<String> {
    match fields {
        VariantFields::Unit => Ok("S._void".to_string()),
        VariantFields::Unnamed(types) => {
            unnamed_fields_codec_expr_mode(types, qualified, ctx, mode)
        }
        VariantFields::Named(fields) => struct_codec_expr_mode(
            fields,
            &inline_object_type_mode(fields, qualified, mode)?,
            qualified,
            ctx,
            mode,
        ),
    }
}

fn unnamed_fields_type(types: &[TypeRef]) -> Result<String> {
    unnamed_fields_type_mode(types, false, NameMode::Public)
}

fn unnamed_fields_type_mode(types: &[TypeRef], qualified: bool, mode: NameMode) -> Result<String> {
    if types.is_empty() {
        Ok("undefined".to_string())
    } else if types.len() == 1 {
        ts_type_with_named(&types[0], qualified, mode)
    } else {
        Ok(format!(
            "[{}]",
            types
                .iter()
                .map(|ty| ts_type_with_named(ty, qualified, mode))
                .collect::<Result<Vec<_>>>()?
                .join(", ")
        ))
    }
}

fn unnamed_fields_codec_expr_mode(
    types: &[TypeRef],
    qualified: bool,
    ctx: &CodecContext,
    mode: NameMode,
) -> Result<String> {
    if types.is_empty() {
        Ok("S._void".to_string())
    } else if types.len() == 1 {
        codec_expr_mode(&types[0], qualified, ctx, mode)
    } else {
        let codecs = types
            .iter()
            .map(|ty| codec_expr_mode(ty, qualified, ctx, mode))
            .collect::<Result<Vec<_>>>()?
            .join(", ");
        Ok(format!("S.Tuple({codecs})"))
    }
}

fn struct_codec_expr_mode(
    fields: &[FieldDef],
    type_name: &str,
    qualified: bool,
    ctx: &CodecContext,
    mode: NameMode,
) -> Result<String> {
    let field_specs = fields
        .iter()
        .map(|field| {
            let (name, _optional) = ts_field_name(&field.name, &field.type_ref);
            Ok(format!(
                "{}: {}",
                name,
                codec_expr_mode(&field.type_ref, qualified, ctx, mode)?
            ))
        })
        .collect::<Result<Vec<_>>>()?
        .join(", ");
    Ok(format!(
        "S.Struct({{{field_specs}}}) as S.Codec<{type_name}>"
    ))
}

fn inline_object_type_mode(fields: &[FieldDef], qualified: bool, mode: NameMode) -> Result<String> {
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
                        ts_inner_option_with_named(&field.type_ref, qualified, mode)?
                    ))
                } else {
                    Ok(format!(
                        "{}: {}",
                        name,
                        ts_type_with_named(&field.type_ref, qualified, mode)?
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
    method_payload_codec_expr_mode(params, qualified, ctx, NameMode::Public)
}

fn method_payload_codec_expr_mode(
    params: &[ParamDef],
    qualified: bool,
    ctx: &CodecContext,
    mode: NameMode,
) -> Result<String> {
    match params.len() {
        0 => Ok("S._void".to_string()),
        1 => codec_expr_mode(&params[0].type_ref, qualified, ctx, mode),
        _ => {
            let codecs = params
                .iter()
                .map(|param| codec_expr_mode(&param.type_ref, qualified, ctx, mode))
                .collect::<Result<Vec<_>>>()?
                .join(", ");
            Ok(format!("S.Tuple({codecs})"))
        }
    }
}

fn codec_expr(ty: &TypeRef, qualified: bool, ctx: &CodecContext) -> Result<String> {
    codec_expr_mode(ty, qualified, ctx, NameMode::Public)
}

fn codec_expr_raw(ty: &TypeRef, qualified: bool, ctx: &CodecContext) -> Result<String> {
    codec_expr_mode(ty, qualified, ctx, NameMode::HostServer)
}

fn codec_expr_mode(
    ty: &TypeRef,
    qualified: bool,
    ctx: &CodecContext,
    mode: NameMode,
) -> Result<String> {
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
            "str" => Ok("S.str".to_string()),
            _ => bail!("Unsupported primitive type `{name}` in TypeScript codec generation"),
        },
        TypeRef::Named { name, args } => {
            let resolved = resolve_named(name, mode);
            let target = if qualified {
                qualify_named(&resolved, mode)
            } else {
                resolved
            };

            if args.is_empty() {
                Ok(target)
            } else {
                let codecs = args
                    .iter()
                    .map(|arg| codec_expr_mode(arg, qualified, ctx, mode))
                    .collect::<Result<Vec<_>>>()?
                    .join(", ");
                Ok(format!("{target}({codecs})"))
            }
        }
        TypeRef::Vec(inner) => match inner.as_ref() {
            TypeRef::Primitive(name) if name == "u8" => Ok("S.Hex()".to_string()),
            _ => Ok(format!(
                "S.Vector({})",
                codec_expr_mode(inner, qualified, ctx, mode)?
            )),
        },
        TypeRef::Option(inner) => Ok(format!(
            "S.Option({})",
            codec_expr_mode(inner, qualified, ctx, mode)?
        )),
        TypeRef::Tuple(items) => {
            if items.is_empty() {
                Ok("S._void".to_string())
            } else {
                let codecs = items
                    .iter()
                    .map(|item| codec_expr_mode(item, qualified, ctx, mode))
                    .collect::<Result<Vec<_>>>()?
                    .join(", ");
                Ok(format!("S.Tuple({codecs})"))
            }
        }
        TypeRef::Array(inner, len) => match inner.as_ref() {
            TypeRef::Primitive(name) if name == "u8" => Ok(format!("S.Hex({len})")),
            _ => Ok(format!(
                "S.Vector({}, {})",
                codec_expr_mode(inner, qualified, ctx, mode)?,
                len
            )),
        },
        TypeRef::Generic(name) => ctx
            .generic_codecs
            .get(name)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Missing codec for generic parameter `{name}`")),
        TypeRef::Unit => Ok("S._void".to_string()),
    }
}

fn ts_type(ty: &TypeRef) -> Result<String> {
    ts_type_with_named(ty, false, NameMode::Public)
}

fn ts_type_with_named(ty: &TypeRef, qualified: bool, mode: NameMode) -> Result<String> {
    match ty {
        TypeRef::Primitive(name) => match name.as_str() {
            "bool" => Ok("boolean".to_string()),
            "u8" | "u16" | "u32" | "i8" | "i16" | "i32" | "f32" | "f64" => Ok("number".to_string()),
            "u64" | "u128" | "i64" | "i128" => Ok("bigint".to_string()),
            "str" => Ok("string".to_string()),
            _ => bail!("Unsupported primitive type `{name}` in TypeScript type generation"),
        },
        TypeRef::Named { name, args } => {
            let resolved = resolve_named(name, mode);
            let target = if qualified {
                qualify_named(&resolved, mode)
            } else {
                resolved
            };

            if args.is_empty() {
                Ok(target)
            } else {
                let args = args
                    .iter()
                    .map(|arg| ts_type_with_named(arg, qualified, mode))
                    .collect::<Result<Vec<_>>>()?
                    .join(", ");
                Ok(format!("{target}<{args}>"))
            }
        }
        TypeRef::Vec(inner) => match inner.as_ref() {
            TypeRef::Primitive(name) if name == "u8" => Ok(hex_string_ts_name(qualified)),
            _ => Ok(format!(
                "Array<{}>",
                ts_type_with_named(inner, qualified, mode)?
            )),
        },
        TypeRef::Option(inner) => Ok(format!(
            "{} | undefined",
            ts_type_with_named(inner, qualified, mode)?
        )),
        TypeRef::Tuple(items) => {
            if items.is_empty() {
                Ok("undefined".to_string())
            } else {
                Ok(format!(
                    "[{}]",
                    items
                        .iter()
                        .map(|item| ts_type_with_named(item, qualified, mode))
                        .collect::<Result<Vec<_>>>()?
                        .join(", ")
                ))
            }
        }
        TypeRef::Array(inner, _len) => match inner.as_ref() {
            TypeRef::Primitive(name) if name == "u8" => Ok(hex_string_ts_name(qualified)),
            _ => Ok(format!(
                "Array<{}>",
                ts_type_with_named(inner, qualified, mode)?
            )),
        },
        TypeRef::Generic(name) => Ok(name.clone()),
        TypeRef::Unit => Ok("undefined".to_string()),
    }
}

/// Always emit the user-facing `HexString` name (no codec-namespace prefix).
/// Generated `types.ts` imports it directly from `scale.js`.
fn hex_string_ts_name(_qualified: bool) -> String {
    "HexString".to_string()
}

fn ts_inner_option(ty: &TypeRef) -> Result<String> {
    ts_inner_option_with_named(ty, false, NameMode::Public)
}

fn ts_inner_option_with_named(ty: &TypeRef, qualified: bool, mode: NameMode) -> Result<String> {
    match ty {
        TypeRef::Option(inner) => ts_type_with_named(inner, qualified, mode),
        other => ts_type_with_named(other, qualified, mode),
    }
}

fn ts_type_qualified(ty: &TypeRef) -> Result<String> {
    ts_type_with_named(ty, true, NameMode::Public)
}

fn ts_type_qualified_raw(ty: &TypeRef) -> Result<String> {
    ts_type_with_named(ty, true, NameMode::HostServer)
}

fn ts_field_name(name: &str, ty: &TypeRef) -> (String, bool) {
    let camel = to_camel_case(name);
    let optional = matches!(ty, TypeRef::Option(_));
    (camel, optional)
}

fn payload_type(params: &[ParamDef]) -> Result<String> {
    payload_type_mode(params, NameMode::Public)
}

fn payload_type_mode(params: &[ParamDef], mode: NameMode) -> Result<String> {
    match params.len() {
        0 => Ok("undefined".to_string()),
        1 => ts_type_with_named(&params[0].type_ref, true, mode),
        _ => Ok(format!(
            "[{}]",
            params
                .iter()
                .map(|param| ts_type_with_named(&param.type_ref, true, mode))
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

fn service_display_name(trait_def: &TraitDef) -> String {
    humanize_service_name(&trait_def.name)
}

fn humanize_service_name(name: &str) -> String {
    let display_name = name
        .to_case(Case::Title)
        .split_whitespace()
        .map(|word| match word {
            "Api" => "API".to_string(),
            "Id" => "ID".to_string(),
            "Json" => "JSON".to_string(),
            "Rpc" => "RPC".to_string(),
            "Url" => "URL".to_string(),
            _ => word.to_string(),
        })
        .collect::<Vec<_>>()
        .join(" ");

    if display_name == "JSON RPC" {
        "JSON-RPC".to_string()
    } else {
        display_name
    }
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
    s.to_case(Case::Camel)
}

/// Generates the host-side TypeScript dispatcher, typed handler interfaces,
/// and barrel files for an extracted API definition into `output_dir`.
///
/// Unlike the client generator, the host emission does not take a target
/// wire version. A host must be able to dispatch frames from any client
/// version it has shipped to, so the generator emits dispatch entries that
/// cover every variant of every wrapper a method touches.
///
/// Per-version inner types are emitted into `types-by-version.ts` under
/// their raw V0N-prefixed names (e.g. `V01HostAccountGetRequest`). This
/// decouples the host from the client's `--client-version` choice: when a
/// future `@parity/truapi` is built for V2, its unprefixed `HostAccountGetRequest`
/// alias points at the V2 inner, but the host's `V01HostAccountGetRequest`
/// keeps its V1 shape because it lives in this package.
///
/// Non-versioned helper types (`HexString`, `ProductAccountId` if it lives
/// outside `v0N::`, etc.) continue to come from `@parity/truapi`.
pub fn generate_host(api: &ApiDefinition, output_dir: &str) -> Result<()> {
    fs::create_dir_all(output_dir)?;
    validate_versioned_wrapper_shapes(api)?;

    let by_version = generate_host_versioned_types(api)?;
    fs::write(
        Path::new(output_dir).join("types-by-version.ts"),
        by_version,
    )?;

    let server_code = generate_host_server(api)?;
    fs::write(Path::new(output_dir).join("server.ts"), server_code)?;

    Ok(())
}

/// Emit type and codec definitions for every V0N-prefixed type in the API,
/// using raw V0N names for all references (including transitive ones). The
/// host's `server.ts` references types via these raw names so that the
/// shapes the dispatcher decodes don't shift when `@parity/truapi`'s
/// `--client-version` changes.
fn generate_host_versioned_types(api: &ApiDefinition) -> Result<String> {
    let mut out = String::new();
    writedoc!(
        out,
        r#"
        // Auto-generated by truapi-codegen. Do not edit.
        //
        // Per-version inner type and codec definitions for the host
        // dispatcher. Names are V0N-prefixed so they remain stable across
        // any `--client-version` choice for `@parity/truapi`.

        import * as S from '@parity/truapi/scale';
        import type {{ HexString }} from '@parity/truapi/scale';
        import * as T from '@parity/truapi';

        "#
    )
    .unwrap();

    let ctx = CodecContext::default();
    for ty in &api.types {
        // Only V0N-prefixed types are emitted here; everything else stays
        // in `@parity/truapi`.
        if version_prefixed_type(&ty.name).is_none() {
            continue;
        }
        // Versioned wrapper enums themselves (`HostAccountGetRequest`) are
        // never referenced by the host (the host emits inline tagged-union
        // literals against the inner types directly), so skip them.
        if detect_versioned_wrapper(ty).is_some() {
            continue;
        }
        if !ty.generic_params.is_empty() {
            bail!(
                "generic V0N-prefixed type `{}` is not supported by the host generator",
                ty.name
            );
        }
        write_host_type_definition(&mut out, ty)?;
        writeln!(out).unwrap();
        write_host_codec_definition(&mut out, ty, &ctx)?;
        writeln!(out).unwrap();
    }
    Ok(out)
}

fn write_host_type_definition(out: &mut String, ty: &TypeDef) -> Result<()> {
    let emitted_name = ty.name.clone();
    let mode = NameMode::RawLocal;
    write_jsdoc(out, "", ty.docs.as_deref());
    match &ty.kind {
        TypeDefKind::Alias(type_ref) => {
            writeln!(
                out,
                "export type {emitted_name} = {};",
                ts_type_with_named(type_ref, true, mode)?
            )
            .unwrap();
        }
        TypeDefKind::Struct(fields) => {
            writeln!(out, "export interface {emitted_name} {{").unwrap();
            for field in fields {
                let (ts_name, optional) = ts_field_name(&field.name, &field.type_ref);
                write_jsdoc(out, "  ", field.docs.as_deref());
                if optional {
                    writeln!(
                        out,
                        "  {ts_name}?: {};",
                        ts_inner_option_with_named(&field.type_ref, true, mode)?
                    )
                    .unwrap();
                } else {
                    writeln!(
                        out,
                        "  {ts_name}: {};",
                        ts_type_with_named(&field.type_ref, true, mode)?
                    )
                    .unwrap();
                }
            }
            writeln!(out, "}}").unwrap();
        }
        TypeDefKind::TupleStruct(fields) => {
            writeln!(
                out,
                "export type {emitted_name} = {};",
                unnamed_fields_type_mode(fields, true, mode)?
            )
            .unwrap();
        }
        TypeDefKind::Enum(variants) => {
            if is_unit_only_enum(ty) {
                writeln!(
                    out,
                    "export type {emitted_name} = {};",
                    unit_enum_union_type(variants)?
                )
                .unwrap();
            } else {
                writeln!(out, "export type {emitted_name} =").unwrap();
                for variant in variants {
                    write_jsdoc(out, "  ", variant.docs.as_deref());
                    writeln!(out, "  | {}", enum_variant_ts_type_mode(variant, mode)?).unwrap();
                }
                writeln!(out, ";").unwrap();
            }
        }
    }
    Ok(())
}

fn write_host_codec_definition(out: &mut String, ty: &TypeDef, ctx: &CodecContext) -> Result<()> {
    let mode = NameMode::RawLocal;
    let emitted_name = ty.name.clone();
    let codec_body = type_codec_expr_mode_qualified(ty, &emitted_name, ctx, mode, true)?;
    writeln!(
        out,
        "export const {emitted_name}: S.Codec<{emitted_name}> = S.lazy((): S.Codec<{emitted_name}> => {codec_body});",
    )
    .unwrap();
    Ok(())
}

/// Set of wire versions a host dispatch entry must handle for a given
/// method: the intersection of variant sets across every versioned wrapper
/// the method touches. Empty when the method has no versioned wrapper at
/// all (unversioned input/output struct).
fn method_wire_versions(
    method: &MethodDef,
    wrappers: &HashMap<String, VersionedWrapper>,
) -> Result<BTreeSet<u32>> {
    let wrapper_names = method_versioned_wrappers(method, wrappers);
    if wrapper_names.is_empty() {
        return Ok(BTreeSet::new());
    }
    let mut candidates: Option<BTreeSet<u32>> = None;
    for wrapper_name in wrapper_names {
        let wrapper = wrappers
            .get(&wrapper_name)
            .expect("method_versioned_wrappers only returns known wrappers");
        let versions: BTreeSet<u32> = wrapper.variants.keys().copied().collect();
        candidates = Some(match candidates {
            Some(current) => current.intersection(&versions).copied().collect(),
            None => versions,
        });
    }
    let result = candidates.unwrap_or_default();
    if result.is_empty() {
        bail!(
            "method `{}` references versioned wrappers with disjoint variant sets; cannot dispatch on the host",
            method.name
        );
    }
    Ok(result)
}

/// TS type literal and inner SCALE codec for a single version of a value
/// type. Used as the per-arm payload of `host_union_type` /
/// `host_indexed_codec` to build versioned union types / `indexedTaggedUnion`
/// codecs.
struct HostVersionedShape {
    version: u32,
    inner_type_ts: String,
    inner_codec_expr: String,
}

/// Resolve the per-version inner type and codec for the method's request
/// payload. Returns `None` for the unversioned-struct case so callers can
/// fall back to a single-shape payload.
fn host_request_shapes_for_versions(
    params: &[ParamDef],
    wrappers: &HashMap<String, VersionedWrapper>,
    ctx: &CodecContext,
    versions: &BTreeSet<u32>,
) -> Result<Option<Vec<HostVersionedShape>>> {
    if params.len() == 1
        && let Some((wrapper_name, wrapper)) = versioned_wrapper_for(&params[0].type_ref, wrappers)
    {
        let mut shapes = Vec::new();
        for version in versions {
            let variant = wrapper.variants.get(version).ok_or_else(|| {
                anyhow::anyhow!("versioned wrapper `{wrapper_name}` has no V{version} variant")
            })?;
            shapes.push(match &variant.kind {
                VersionedKind::Unit => HostVersionedShape {
                    version: *version,
                    inner_type_ts: "undefined".to_string(),
                    inner_codec_expr: "S._void".to_string(),
                },
                VersionedKind::Tuple(inner) => HostVersionedShape {
                    version: *version,
                    inner_type_ts: ts_type_qualified_raw(inner)?,
                    inner_codec_expr: codec_expr_raw(inner, true, ctx)?,
                },
            });
        }
        return Ok(Some(shapes));
    }
    if params.is_empty() {
        // No-param method: still emit a per-version `S._void` envelope so
        // the dispatcher can validate the inbound version byte.
        let shapes = versions
            .iter()
            .map(|v| HostVersionedShape {
                version: *v,
                inner_type_ts: "undefined".to_string(),
                inner_codec_expr: "S._void".to_string(),
            })
            .collect();
        return Ok(Some(shapes));
    }
    Ok(None)
}

/// Resolve the per-version inner shape for a response/item/error/reason
/// position. Versioned wrappers fan out across `versions`; unversioned
/// types (including `()`) repeat the same shape per version. Returns `None`
/// only for unversioned types when `versions` is empty (no versioned
/// wrapper anywhere on the method), letting callers fall back to a
/// non-`indexedTaggedUnion` codec for fully-unversioned methods.
fn host_response_shapes_for_versions(
    ty: &TypeRef,
    wrappers: &HashMap<String, VersionedWrapper>,
    ctx: &CodecContext,
    versions: &BTreeSet<u32>,
) -> Result<Option<Vec<HostVersionedShape>>> {
    if let Some((wrapper_name, wrapper)) = versioned_wrapper_for(ty, wrappers) {
        let mut shapes = Vec::new();
        for version in versions {
            let variant = wrapper.variants.get(version).ok_or_else(|| {
                anyhow::anyhow!("versioned wrapper `{wrapper_name}` has no V{version} variant")
            })?;
            shapes.push(match &variant.kind {
                VersionedKind::Unit => HostVersionedShape {
                    version: *version,
                    inner_type_ts: "undefined".to_string(),
                    inner_codec_expr: "S._void".to_string(),
                },
                VersionedKind::Tuple(inner) => HostVersionedShape {
                    version: *version,
                    inner_type_ts: ts_type_qualified_raw(inner)?,
                    inner_codec_expr: codec_expr_raw(inner, true, ctx)?,
                },
            });
        }
        return Ok(Some(shapes));
    }
    if versions.is_empty() {
        return Ok(None);
    }
    // Non-versioned type referenced from a versioned method position
    // (e.g. `()` in `Result<(), VersionedErr>`). The shape is the same for
    // every version, but we render it with raw-mode names anyway so a
    // future V0N-prefixed transitive reference stays stable.
    let inner_type_ts = ts_type_qualified_raw(ty)?;
    let inner_codec_expr = codec_expr_raw(ty, true, ctx)?;
    let shapes = versions
        .iter()
        .map(|v| HostVersionedShape {
            version: *v,
            inner_type_ts: inner_type_ts.clone(),
            inner_codec_expr: inner_codec_expr.clone(),
        })
        .collect();
    Ok(Some(shapes))
}

/// Render a TS union type `{ tag: 'V1'; value: T1 } | { tag: 'V2'; value: T2 } | ...`.
///
/// A variant whose inner TS type renders as `undefined` emits the optional
/// form `{ tag: 'V<n>'; value?: undefined }` to match how the SCALE codec
/// shapes unit variants on decode.
fn host_union_type<F>(shapes: &[HostVersionedShape], inner_type: F) -> String
where
    F: Fn(&HostVersionedShape) -> String,
{
    shapes
        .iter()
        .map(|s| {
            let inner = inner_type(s);
            if inner == "undefined" {
                format!("{{ tag: 'V{}'; value?: undefined }}", s.version)
            } else {
                format!("{{ tag: 'V{}'; value: {} }}", s.version, inner)
            }
        })
        .collect::<Vec<_>>()
        .join(" | ")
}

fn generate_host_server(api: &ApiDefinition) -> Result<String> {
    let ctx = CodecContext::default();
    let wrappers = collect_versioned_wrappers(api);
    let services = public_services(api)?;

    let mut services_with_methods = Vec::new();
    for service in &services {
        let trait_def = service.trait_def;
        let methods = host_included_methods(trait_def, &wrappers)?;
        if methods.is_empty() {
            continue;
        }
        services_with_methods.push((trait_def, methods));
    }

    // Only import the response-payload helpers actually referenced by this
    // generation. `MethodKind::Request` methods need `toResponsePayload`
    // (versioned) or `toFlatResponsePayload` (unversioned).
    let mut needs_versioned_helper = false;
    let mut needs_flat_helper = false;
    for (_, methods) in &services_with_methods {
        for method in methods {
            if !matches!(method.kind, MethodKind::Request) {
                continue;
            }
            if method_wire_versions(method, &wrappers)?.is_empty() {
                needs_flat_helper = true;
            } else {
                needs_versioned_helper = true;
            }
        }
    }
    let helper_imports = match (needs_versioned_helper, needs_flat_helper) {
        (true, true) => "  toFlatResponsePayload,\n  toResponsePayload,\n",
        (true, false) => "  toResponsePayload,\n",
        (false, true) => "  toFlatResponsePayload,\n",
        (false, false) => "",
    };

    let mut out = String::new();
    writedoc!(
        out,
        r#"
        // Auto-generated by truapi-codegen. Do not edit.

        import type {{ ResultAsync }} from 'neverthrow';
        import * as S from '@parity/truapi/scale';
        import * as T from '@parity/truapi';
        import * as V from './types-by-version.js';
        import * as W from '@parity/truapi/wire-table';
        import type {{
          HexString,
          ObservableLike,
          Observer,
          Provider,
          Subscription,
        }} from '@parity/truapi';
        import {{
          createHostServer,
        {helper_imports}  type CallContext,
          type HostDispatchEntry,
          type HostServerHooks,
          type TrUApiHostServer,
        }} from '../index.js';

        export type {{
          CallContext,
          HostServerHooks,
          ObservableLike,
          Observer,
          ResultAsync,
          Subscription,
          TrUApiHostServer,
        }};

        "#
    )
    .unwrap();

    for (trait_def, methods) in &services_with_methods {
        for method in methods {
            emit_host_method_aliases(&mut out, trait_def, method, &wrappers, &ctx)?;
        }
        writeln!(out).unwrap();

        write_jsdoc(&mut out, "", trait_def.docs.as_deref());
        writeln!(out, "export interface {}HostHandlers {{", trait_def.name).unwrap();
        for method in methods {
            emit_host_handler_signature(&mut out, trait_def, method, &wrappers)?;
        }
        writedoc!(out, "}}\n\n").unwrap();

        for method in methods {
            emit_host_method_codecs(&mut out, trait_def, method, &wrappers, &ctx)?;
        }
        writeln!(out).unwrap();

        writeln!(
            out,
            "function build{name}Entries(handlers: {name}HostHandlers): HostDispatchEntry[] {{",
            name = trait_def.name
        )
        .unwrap();
        writeln!(out, "  return [").unwrap();
        for method in methods {
            emit_host_entry(&mut out, trait_def, method, &wrappers)?;
        }
        writedoc!(out, "  ];\n}}\n\n").unwrap();
    }

    writeln!(out, "export interface TrUApiHostHandlers {{").unwrap();
    for (trait_def, _) in &services_with_methods {
        let field = to_camel_case(&trait_def.name);
        writeln!(
            out,
            "  readonly {field}: {name}HostHandlers;",
            name = trait_def.name
        )
        .unwrap();
    }
    writedoc!(
        out,
        r#"
        }}

        /** Attach a host server to a `Provider`. Inbound request and
         * subscription frames are routed to the supplied typed handlers.
         */
        export function createTrUApiServer(
          provider: Provider,
          handlers: TrUApiHostHandlers,
          hooks?: HostServerHooks,
        ): TrUApiHostServer {{
          const entries: HostDispatchEntry[] = [
        "#
    )
    .unwrap();
    for (trait_def, _) in &services_with_methods {
        let field = to_camel_case(&trait_def.name);
        writeln!(
            out,
            "    ...build{name}Entries(handlers.{field}),",
            name = trait_def.name,
            field = field
        )
        .unwrap();
    }
    writedoc!(
        out,
        r#"
          ];
          return createHostServer(provider, entries, hooks);
        }}
        "#
    )
    .unwrap();

    Ok(out)
}

/// Like `included_methods` but for the host generator: include any method
/// with at least one shared wire version across its wrappers, or no
/// versioned wrappers at all.
fn host_included_methods<'a>(
    trait_def: &'a TraitDef,
    wrappers: &HashMap<String, VersionedWrapper>,
) -> Result<Vec<&'a MethodDef>> {
    trait_def
        .methods
        .iter()
        .filter_map(
            |method| match host_method_is_included(trait_def, method, wrappers) {
                Ok(true) => Some(Ok(method)),
                Ok(false) => None,
                Err(err) => Some(Err(err)),
            },
        )
        .collect()
}

fn host_method_is_included(
    trait_def: &TraitDef,
    method: &MethodDef,
    wrappers: &HashMap<String, VersionedWrapper>,
) -> Result<bool> {
    wire_ids_for_method(trait_def, method)?;
    let wrapper_names = method_versioned_wrappers(method, wrappers);
    if wrapper_names.is_empty() {
        return Ok(true);
    }
    Ok(!method_wire_versions(method, wrappers)?.is_empty())
}

/// TS type for the handler's `request` parameter. A versioned method emits
/// the union over every shared wire version. An unversioned request emits
/// the plain payload struct type (works whether or not the method's other
/// positions are versioned).
fn host_request_type(
    params: &[ParamDef],
    wrappers: &HashMap<String, VersionedWrapper>,
    ctx: &CodecContext,
    versions: &BTreeSet<u32>,
) -> Result<String> {
    match host_request_shapes_for_versions(params, wrappers, ctx, versions)? {
        Some(shapes) => Ok(host_union_type(&shapes, |s| s.inner_type_ts.clone())),
        None => {
            if params.is_empty() {
                Ok("undefined".to_string())
            } else {
                payload_type(params)
            }
        }
    }
}

/// Codec for decoding the request bytes of a method. A versioned request
/// gets an `indexedTaggedUnion` over inner per-version codecs; an
/// unversioned request gets the inline struct codec.
fn host_request_codec(
    params: &[ParamDef],
    wrappers: &HashMap<String, VersionedWrapper>,
    ctx: &CodecContext,
    versions: &BTreeSet<u32>,
) -> Result<String> {
    match host_request_shapes_for_versions(params, wrappers, ctx, versions)? {
        Some(shapes) => indexed_versioned_codec_expr(
            shapes.into_iter().map(|s| (s.version, s.inner_codec_expr)),
        ),
        None => {
            if params.is_empty() {
                Ok("S._void".to_string())
            } else {
                method_payload_codec_expr_mode(params, true, ctx, NameMode::HostServer)
            }
        }
    }
}

/// Codec for encoding the response of a request method. Per-position: if
/// both Ok and Err are versioned, build `indexedTaggedUnion({V<n>: [n-1,
/// S.Result(ok_n, err_n)], ...})`. If neither is versioned, fall back to
/// `S.Result(ok, err)`.
fn host_response_codec(
    ok: &TypeRef,
    err: &TypeRef,
    wrappers: &HashMap<String, VersionedWrapper>,
    ctx: &CodecContext,
    versions: &BTreeSet<u32>,
) -> Result<String> {
    let err_inner = call_error_inner(err).unwrap_or(err);
    let ok_shapes = host_response_shapes_for_versions(ok, wrappers, ctx, versions)?;
    let err_shapes = host_response_shapes_for_versions(err_inner, wrappers, ctx, versions)?;
    match (ok_shapes, err_shapes) {
        (None, None) => {
            let ok_codec = codec_expr_raw(ok, true, ctx)?;
            let err_codec = codec_expr_raw(err_inner, true, ctx)?;
            Ok(format!("S.Result({ok_codec}, {err_codec})"))
        }
        (Some(o), Some(e)) => {
            indexed_versioned_codec_expr(o.iter().zip(e.iter()).map(|(oo, ee)| {
                (
                    oo.version,
                    format!(
                        "S.Result({ok}, {err})",
                        ok = oo.inner_codec_expr,
                        err = ee.inner_codec_expr,
                    ),
                )
            }))
        }
        _ => bail!("internal: mismatched per-version shapes for `{ok:?}` and `{err_inner:?}`"),
    }
}

/// Codec for encoding a subscription item or interrupt reason. Versioned
/// positions build an `indexedTaggedUnion`; unversioned positions get the
/// plain inline codec.
fn host_value_codec(
    ty: &TypeRef,
    wrappers: &HashMap<String, VersionedWrapper>,
    ctx: &CodecContext,
    versions: &BTreeSet<u32>,
) -> Result<String> {
    match host_response_shapes_for_versions(ty, wrappers, ctx, versions)? {
        Some(shapes) => indexed_versioned_codec_expr(
            shapes.into_iter().map(|s| (s.version, s.inner_codec_expr)),
        ),
        None => codec_expr_raw(ty, true, ctx),
    }
}

/// Codec + literal value for a void interrupt frame in a plain (non-result)
/// subscription. The client ignores these bytes; we encode the lowest known
/// version so the codec stays in shape.
fn host_void_interrupt(versions: &BTreeSet<u32>) -> Result<(String, String)> {
    if versions.is_empty() {
        return Ok(("S._void".to_string(), "undefined".to_string()));
    }
    let entries: Vec<(u32, String)> = versions
        .iter()
        .map(|v| (*v, "S._void".to_string()))
        .collect();
    let codec = indexed_versioned_codec_expr(entries)?;
    let first = versions.iter().next().expect("non-empty checked above");
    let value = format!("{{ tag: 'V{first}' as const, value: undefined }}");
    Ok((codec, value))
}

/// Stable PascalCase prefix used for every alias emitted per host method,
/// e.g. `AccountGetAccount` for `Account::get_account`. Suffixes (`Request`,
/// `Result`, `Item`, `Reason`) disambiguate the role each alias plays.
fn host_alias_base(trait_name: &str, method: &MethodDef) -> String {
    let pascal_method = to_camel_case(&strip_prefix(&method.name)).to_case(Case::Pascal);
    format!("{trait_name}{pascal_method}")
}

/// Emit the per-version type aliases used by one method's handler interface.
/// Each version of each role (request/ok/err/item/reason) gets its own alias
/// so consumers can import stable names when implementing handlers.
fn emit_host_method_aliases(
    out: &mut String,
    trait_def: &TraitDef,
    method: &MethodDef,
    wrappers: &HashMap<String, VersionedWrapper>,
    ctx: &CodecContext,
) -> Result<()> {
    let versions = method_wire_versions(method, wrappers)?;
    let has_request_param =
        !method.params.is_empty() || (trait_def.name == "System" && method.name == "handshake");
    let base = host_alias_base(&trait_def.name, method);

    if versions.is_empty() {
        if has_request_param {
            let request_type = host_request_type(&method.params, wrappers, ctx, &versions)?;
            writeln!(out, "export type {base}Request = {request_type};").unwrap();
        }
        match (&method.kind, &method.return_type) {
            (MethodKind::Request, ReturnType::Result { ok, err }) => {
                let err_inner = call_error_inner(err).unwrap_or(err);
                let ok_ts = ts_type_qualified_raw(ok)?;
                let err_ts = ts_type_qualified_raw(err_inner)?;
                writeln!(out, "export type {base}Ok = {ok_ts};").unwrap();
                writeln!(out, "export type {base}Err = {err_ts};").unwrap();
            }
            (MethodKind::Subscription, ReturnType::Subscription(ty)) => {
                let item_ts = ts_type_qualified_raw(ty)?;
                writeln!(out, "export type {base}Item = {item_ts};").unwrap();
            }
            (MethodKind::ResultSubscription, ReturnType::ResultSubscription { item, err }) => {
                let err_inner = call_error_inner(err).unwrap_or(err);
                let item_ts = ts_type_qualified_raw(item)?;
                let reason_ts = ts_type_qualified_raw(err_inner)?;
                writeln!(out, "export type {base}Item = {item_ts};").unwrap();
                writeln!(out, "export type {base}Reason = {reason_ts};").unwrap();
            }
            (kind, return_type) => {
                bail!(
                    "Host generator mismatch for method `{}`: kind {:?} does not match return type {:?}",
                    method.name,
                    kind,
                    return_type
                );
            }
        }
        return Ok(());
    }

    // Always emit Request as a versioned union, including no-param methods
    // (those carry `value?: undefined` per variant).
    let request_shapes =
        match host_request_shapes_for_versions(&method.params, wrappers, ctx, &versions)? {
            Some(shapes) => shapes,
            None => versions
                .iter()
                .map(|v| HostVersionedShape {
                    version: *v,
                    inner_type_ts: "undefined".to_string(),
                    inner_codec_expr: "S._void".to_string(),
                })
                .collect(),
        };
    writeln!(
        out,
        "export type {base}Request = {ts};",
        ts = host_union_type(&request_shapes, |s| s.inner_type_ts.clone()),
    )
    .unwrap();

    match (&method.kind, &method.return_type) {
        (MethodKind::Request, ReturnType::Result { ok, err }) => {
            let err_inner = call_error_inner(err).unwrap_or(err);
            let ok_shapes = host_response_shapes_for_versions(ok, wrappers, ctx, &versions)?
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "internal: ok shapes unexpectedly unversioned for `{}`",
                        method.name
                    )
                })?;
            let err_shapes =
                host_response_shapes_for_versions(err_inner, wrappers, ctx, &versions)?
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "internal: err shapes unexpectedly unversioned for `{}`",
                            method.name
                        )
                    })?;
            writeln!(
                out,
                "export type {base}Ok = {ts};",
                ts = host_union_type(&ok_shapes, |s| s.inner_type_ts.clone()),
            )
            .unwrap();
            writeln!(
                out,
                "export type {base}Err = {ts};",
                ts = host_union_type(&err_shapes, |s| s.inner_type_ts.clone()),
            )
            .unwrap();
        }
        (MethodKind::Subscription, ReturnType::Subscription(ty)) => {
            let item_shapes = host_response_shapes_for_versions(ty, wrappers, ctx, &versions)?
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "internal: item shapes unexpectedly unversioned for `{}`",
                        method.name
                    )
                })?;
            writeln!(
                out,
                "export type {base}Item = {ts};",
                ts = host_union_type(&item_shapes, |s| s.inner_type_ts.clone()),
            )
            .unwrap();
        }
        (MethodKind::ResultSubscription, ReturnType::ResultSubscription { item, err }) => {
            let err_inner = call_error_inner(err).unwrap_or(err);
            let item_shapes = host_response_shapes_for_versions(item, wrappers, ctx, &versions)?
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "internal: item shapes unexpectedly unversioned for `{}`",
                        method.name
                    )
                })?;
            let reason_shapes =
                host_response_shapes_for_versions(err_inner, wrappers, ctx, &versions)?
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "internal: reason shapes unexpectedly unversioned for `{}`",
                            method.name
                        )
                    })?;
            writeln!(
                out,
                "export type {base}Item = {ts};",
                ts = host_union_type(&item_shapes, |s| s.inner_type_ts.clone()),
            )
            .unwrap();
            writeln!(
                out,
                "export type {base}Reason = {ts};",
                ts = host_union_type(&reason_shapes, |s| s.inner_type_ts.clone()),
            )
            .unwrap();
        }
        (kind, return_type) => {
            bail!(
                "Host generator mismatch for method `{}`: kind {:?} does not match return type {:?}",
                method.name,
                kind,
                return_type
            );
        }
    }
    Ok(())
}

/// Module-scope constant name for one of a method's codecs. Builds on the
/// shared `wire_const_name` so the codec sits next to its wire ids in the
/// reader's mental model (`W.ACCOUNT_GET_ACCOUNT` for ids,
/// `ACCOUNT_GET_ACCOUNT_REQUEST_CODEC` for the request codec).
fn codec_const_name(trait_name: &str, method: &MethodDef, suffix: &str) -> String {
    format!("{}_{}", wire_const_name(trait_name, &method.name), suffix)
}

/// Emit module-scope codec constants for one method so the per-call
/// dispatch entry stays tiny (decode → call → encode) and the codecs are
/// only constructed once instead of on every inbound frame.
fn emit_host_method_codecs(
    out: &mut String,
    trait_def: &TraitDef,
    method: &MethodDef,
    wrappers: &HashMap<String, VersionedWrapper>,
    ctx: &CodecContext,
) -> Result<()> {
    let versions = method_wire_versions(method, wrappers)?;
    let request_codec = host_request_codec(&method.params, wrappers, ctx, &versions)?;
    let request_name = codec_const_name(&trait_def.name, method, "REQUEST_CODEC");
    writeln!(out, "const {request_name} = {request_codec};").unwrap();

    match (&method.kind, &method.return_type) {
        (MethodKind::Request, ReturnType::Result { ok, err }) => {
            let response_codec = host_response_codec(ok, err, wrappers, ctx, &versions)?;
            let response_name = codec_const_name(&trait_def.name, method, "RESPONSE_CODEC");
            writeln!(out, "const {response_name} = {response_codec};").unwrap();
        }
        (MethodKind::Subscription, ReturnType::Subscription(ty)) => {
            let item_codec = host_value_codec(ty, wrappers, ctx, &versions)?;
            let item_name = codec_const_name(&trait_def.name, method, "ITEM_CODEC");
            writeln!(out, "const {item_name} = {item_codec};").unwrap();
            let (interrupt_codec, _) = host_void_interrupt(&versions)?;
            let interrupt_name = codec_const_name(&trait_def.name, method, "INTERRUPT_CODEC");
            writeln!(out, "const {interrupt_name} = {interrupt_codec};").unwrap();
        }
        (MethodKind::ResultSubscription, ReturnType::ResultSubscription { item, err }) => {
            let item_codec = host_value_codec(item, wrappers, ctx, &versions)?;
            let item_name = codec_const_name(&trait_def.name, method, "ITEM_CODEC");
            writeln!(out, "const {item_name} = {item_codec};").unwrap();
            let interrupt_codec = host_value_codec(
                call_error_inner(err).unwrap_or(err),
                wrappers,
                ctx,
                &versions,
            )?;
            let interrupt_name = codec_const_name(&trait_def.name, method, "INTERRUPT_CODEC");
            writeln!(out, "const {interrupt_name} = {interrupt_codec};").unwrap();
        }
        (kind, return_type) => {
            bail!(
                "Host generator mismatch for method `{}`: kind {:?} does not match return type {:?}",
                method.name,
                kind,
                return_type
            );
        }
    }
    Ok(())
}

fn emit_host_handler_signature(
    out: &mut String,
    trait_def: &TraitDef,
    method: &MethodDef,
    wrappers: &HashMap<String, VersionedWrapper>,
) -> Result<()> {
    let ts_method_name = to_camel_case(&strip_prefix(&method.name));
    let versions = method_wire_versions(method, wrappers)?;
    // Unversioned methods preserve their original `(ctx)` vs `(ctx, request)`
    // shape. Versioned methods always carry `request` so the handler can read
    // and reply with the matching version tag.
    let has_request_param = !versions.is_empty()
        || !method.params.is_empty()
        || (trait_def.name == "System" && method.name == "handshake");
    let base = host_alias_base(&trait_def.name, method);
    let request_param = if has_request_param {
        format!(", request: {base}Request")
    } else {
        String::new()
    };
    write_jsdoc(out, "  ", method.docs.as_deref());

    match (&method.kind, &method.return_type) {
        (MethodKind::Request, ReturnType::Result { .. }) => {
            writeln!(
                out,
                "  {ts_method_name}(ctx: CallContext{request_param}): ResultAsync<{base}Ok, {base}Err>;",
            )
            .unwrap();
        }
        (MethodKind::Subscription, ReturnType::Subscription(_)) => {
            writeln!(
                out,
                "  {ts_method_name}(ctx: CallContext{request_param}): ObservableLike<{base}Item>;",
            )
            .unwrap();
        }
        (MethodKind::ResultSubscription, ReturnType::ResultSubscription { .. }) => {
            writeln!(
                out,
                "  {ts_method_name}(ctx: CallContext{request_param}): ObservableLike<{base}Item, {base}Reason>;",
            )
            .unwrap();
        }
        (kind, return_type) => {
            bail!(
                "Host generator mismatch for method `{}`: kind {:?} does not match return type {:?}",
                method.name,
                kind,
                return_type
            );
        }
    }
    Ok(())
}

fn emit_host_entry(
    out: &mut String,
    trait_def: &TraitDef,
    method: &MethodDef,
    wrappers: &HashMap<String, VersionedWrapper>,
) -> Result<()> {
    let ts_method_name = to_camel_case(&strip_prefix(&method.name));
    let wire_const = wire_const_name(&trait_def.name, &method.name);
    let versions = method_wire_versions(method, wrappers)?;
    let has_request_param =
        !method.params.is_empty() || (trait_def.name == "System" && method.name == "handshake");

    if versions.is_empty() {
        emit_host_entry_unversioned(
            out,
            trait_def,
            method,
            &ts_method_name,
            &wire_const,
            has_request_param,
        )?;
        return Ok(());
    }

    // Versioned dispatch is straight-through: the request codec decodes a
    // versioned wrapper, the handler returns a versioned wrapper (or items),
    // and the codec routes by tag on encode. No per-version branching. The
    // codec constants come from `emit_host_method_codecs`.
    let request_codec = codec_const_name(&trait_def.name, method, "REQUEST_CODEC");
    match (&method.kind, &method.return_type) {
        (MethodKind::Request, ReturnType::Result { .. }) => {
            let response_codec = codec_const_name(&trait_def.name, method, "RESPONSE_CODEC");
            writedoc!(
                out,
                "
                    {{
                      kind: 'request',
                      ids: W.{wire_const},
                      async handle(ctx, bytes) {{
                        const request = {request_codec}.dec(bytes);
                        const response = toResponsePayload(
                          await handlers.{ts_method_name}(ctx, request),
                        );
                        return {response_codec}.enc(response);
                      }},
                    }},
                "
            )
            .unwrap();
        }
        (MethodKind::Subscription, ReturnType::Subscription(_)) => {
            let item_codec = codec_const_name(&trait_def.name, method, "ITEM_CODEC");
            let interrupt_codec = codec_const_name(&trait_def.name, method, "INTERRUPT_CODEC");
            writedoc!(
                out,
                "
                    {{
                      kind: 'subscription',
                      ids: W.{wire_const},
                      start(ctx, bytes, port) {{
                        const request = {request_codec}.dec(bytes);
                        const sub = handlers.{ts_method_name}(ctx, request).subscribe({{
                          next(item) {{ if (!port.isClosed) port.sendReceive({item_codec}.enc(item)); }},
                          complete() {{ if (!port.isClosed) port.sendInterrupt({interrupt_codec}.enc({{ tag: request.tag, value: undefined }})); }},
                        }});
                        return () => sub.unsubscribe();
                      }},
                    }},
                "
            )
            .unwrap();
        }
        (MethodKind::ResultSubscription, ReturnType::ResultSubscription { .. }) => {
            let item_codec = codec_const_name(&trait_def.name, method, "ITEM_CODEC");
            let interrupt_codec = codec_const_name(&trait_def.name, method, "INTERRUPT_CODEC");
            writedoc!(
                out,
                "
                    {{
                      kind: 'subscription',
                      ids: W.{wire_const},
                      start(ctx, bytes, port) {{
                        const request = {request_codec}.dec(bytes);
                        const sub = handlers.{ts_method_name}(ctx, request).subscribe({{
                          next(item) {{ if (!port.isClosed) port.sendReceive({item_codec}.enc(item)); }},
                          error(err) {{ if (err.reason !== undefined && !port.isClosed) port.sendInterrupt({interrupt_codec}.enc(err.reason)); }},
                        }});
                        return () => sub.unsubscribe();
                      }},
                    }},
                "
            )
            .unwrap();
        }
        (kind, return_type) => {
            bail!(
                "Host generator mismatch for method `{}`: kind {:?} does not match return type {:?}",
                method.name,
                kind,
                return_type
            );
        }
    }
    Ok(())
}

/// Emit a dispatch entry for an unversioned method (no versioned wrapper at any
/// position). Handler is flat: `methodName(ctx, request)` returning
/// `ResultAsync<Ok, Err>` for requests or `ObservableLike<...>` for subscriptions.
fn emit_host_entry_unversioned(
    out: &mut String,
    trait_def: &TraitDef,
    method: &MethodDef,
    ts_method_name: &str,
    wire_const: &str,
    has_request_param: bool,
) -> Result<()> {
    let request_codec = codec_const_name(&trait_def.name, method, "REQUEST_CODEC");
    let request_decode_stmt = if has_request_param {
        format!("const request = {request_codec}.dec(bytes);")
    } else {
        format!("{request_codec}.dec(bytes);")
    };
    let call_args = if has_request_param {
        "ctx, request"
    } else {
        "ctx"
    };
    // `host_void_interrupt` for the unversioned (empty-versions) case returns
    // a literal `undefined` for the interrupt value; capture that here so the
    // plain-subscription complete() frame stays correct.
    let versions = BTreeSet::new();
    let (_, interrupt_value) = host_void_interrupt(&versions)?;

    match (&method.kind, &method.return_type) {
        (MethodKind::Request, ReturnType::Result { .. }) => {
            let response_codec = codec_const_name(&trait_def.name, method, "RESPONSE_CODEC");
            writedoc!(
                out,
                "
                    {{
                      kind: 'request',
                      ids: W.{wire_const},
                      async handle(ctx, bytes) {{
                        {request_decode_stmt}
                        const response = toFlatResponsePayload(
                          await handlers.{ts_method_name}({call_args}),
                        );
                        return {response_codec}.enc(response);
                      }},
                    }},
                "
            )
            .unwrap();
        }
        (MethodKind::Subscription, ReturnType::Subscription(_)) => {
            let item_codec = codec_const_name(&trait_def.name, method, "ITEM_CODEC");
            let interrupt_codec = codec_const_name(&trait_def.name, method, "INTERRUPT_CODEC");
            writedoc!(
                out,
                "
                    {{
                      kind: 'subscription',
                      ids: W.{wire_const},
                      start(ctx, bytes, port) {{
                        {request_decode_stmt}
                        const sub = handlers.{ts_method_name}({call_args}).subscribe({{
                          next(item) {{ if (!port.isClosed) port.sendReceive({item_codec}.enc(item)); }},
                          complete() {{ if (!port.isClosed) port.sendInterrupt({interrupt_codec}.enc({interrupt_value})); }},
                        }});
                        return () => sub.unsubscribe();
                      }},
                    }},
                "
            )
            .unwrap();
        }
        (MethodKind::ResultSubscription, ReturnType::ResultSubscription { .. }) => {
            let item_codec = codec_const_name(&trait_def.name, method, "ITEM_CODEC");
            let interrupt_codec = codec_const_name(&trait_def.name, method, "INTERRUPT_CODEC");
            writedoc!(
                out,
                "
                    {{
                      kind: 'subscription',
                      ids: W.{wire_const},
                      start(ctx, bytes, port) {{
                        {request_decode_stmt}
                        const sub = handlers.{ts_method_name}({call_args}).subscribe({{
                          next(item) {{ if (!port.isClosed) port.sendReceive({item_codec}.enc(item)); }},
                          error(err) {{ if (err.reason !== undefined && !port.isClosed) port.sendInterrupt({interrupt_codec}.enc(err.reason)); }},
                        }});
                        return () => sub.unsubscribe();
                      }},
                    }},
                "
            )
            .unwrap();
        }
        (kind, return_type) => {
            bail!(
                "Host generator mismatch for method `{}`: kind {:?} does not match return type {:?}",
                method.name,
                kind,
                return_type
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request_wire(request_id: Option<u8>) -> WireAttrs {
        WireAttrs {
            request_id,
            ..WireAttrs::default()
        }
    }

    fn subscription_wire(start_id: Option<u8>) -> WireAttrs {
        WireAttrs {
            start_id,
            ..WireAttrs::default()
        }
    }

    #[test]
    fn service_display_name_formats_known_acronyms() {
        let json_rpc = TraitDef {
            name: "JsonRpc".to_string(),
            module_path: Vec::new(),
            methods: Vec::new(),
            docs: None,
        };
        let system = TraitDef {
            name: "System".to_string(),
            module_path: Vec::new(),
            methods: Vec::new(),
            docs: None,
        };

        assert_eq!(service_display_name(&json_rpc), "JSON-RPC");
        assert_eq!(service_display_name(&system), "System");
    }

    fn request_method(name: &str, wire_id: Option<u8>) -> MethodDef {
        MethodDef {
            name: name.to_string(),
            kind: MethodKind::Request,
            params: Vec::new(),
            return_type: ReturnType::Result {
                ok: TypeRef::Unit,
                err: TypeRef::Unit,
            },
            wire: request_wire(wire_id),
            docs: None,
        }
    }

    fn subscription_method(name: &str, wire_id: Option<u8>) -> MethodDef {
        MethodDef {
            name: name.to_string(),
            kind: MethodKind::Subscription,
            params: Vec::new(),
            return_type: ReturnType::Subscription(TypeRef::Unit),
            wire: subscription_wire(wire_id),
            docs: None,
        }
    }

    fn api(methods: Vec<MethodDef>) -> ApiDefinition {
        ApiDefinition {
            traits: vec![TraitDef {
                name: "Example".to_string(),
                module_path: Vec::new(),
                methods,
                docs: None,
            }],
            public_trait_order: Vec::new(),
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
            wire: request_wire(wire_id),
            docs: None,
        }
    }

    fn subscription_method_with_wrappers(name: &str, wire_id: Option<u8>, item: &str) -> MethodDef {
        MethodDef {
            name: name.to_string(),
            kind: MethodKind::Subscription,
            params: Vec::new(),
            return_type: ReturnType::Subscription(named_type(item)),
            wire: subscription_wire(wire_id),
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
    fn latest_wire_version_falls_back_to_one_without_wrappers() {
        let api = ApiDefinition {
            traits: Vec::new(),
            public_trait_order: Vec::new(),
            types: Vec::new(),
        };
        assert_eq!(latest_wire_version(&api), 1);
    }

    #[test]
    fn latest_wire_version_picks_highest_variant_across_wrappers() {
        let api = ApiDefinition {
            traits: Vec::new(),
            public_trait_order: Vec::new(),
            types: vec![
                versioned_tuple_wrapper_variants("OneWrapper", &[(1, "Legacy")]),
                versioned_tuple_wrapper_variants("TwoWrapper", &[(1, "Legacy"), (3, "Latest")]),
                versioned_tuple_wrapper_variants("ThreeWrapper", &[(2, "Middle")]),
            ],
        };
        assert_eq!(latest_wire_version(&api), 3);
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

        assert!(source.contains("export const EXAMPLE_STREAM = {"));
        assert!(source.contains("  start: 2,"));
        assert!(source.contains("  receive: 5,"));
        assert!(source.contains("export const EXAMPLE_LATER = {"));
        assert!(source.contains("  request: 10,"));
        assert!(
            source
                .find("export const EXAMPLE_STREAM")
                .expect("stream entry")
                < source
                    .find("export const EXAMPLE_LATER")
                    .expect("later entry")
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

        assert!(err.to_string().contains("missing #[wire(request_id = N)]"));
    }

    #[test]
    fn generate_wire_table_uses_explicit_overrides() {
        let mut request = request_method("custom_request", Some(2));
        request.wire.response_id = Some(9);
        let mut subscription = subscription_method("custom_stream", Some(20));
        subscription.wire.stop_id = Some(30);
        subscription.wire.interrupt_id = Some(31);
        subscription.wire.receive_id = Some(32);

        let source = generate_wire_table(&api(vec![request, subscription]), 2).expect("wire table");

        assert!(source.contains("export const EXAMPLE_CUSTOM_REQUEST = {"));
        assert!(source.contains("  request: 2,"));
        assert!(source.contains("  response: 9,"));
        assert!(source.contains("export const EXAMPLE_CUSTOM_STREAM = {"));
        assert!(source.contains("  start: 20,"));
        assert!(source.contains("  stop: 30,"));
        assert!(source.contains("  interrupt: 31,"));
        assert!(source.contains("  receive: 32,"));
    }

    #[test]
    fn generate_wire_table_rejects_invalid_attrs_by_method_kind() {
        let mut request = request_method("bad_request", Some(2));
        request.wire.start_id = Some(4);
        let err = generate_wire_table(&api(vec![request]), 2)
            .expect_err("request with start id must error");
        assert!(
            err.to_string()
                .contains("must not use subscription wire ids")
        );

        let mut subscription = subscription_method("bad_stream", Some(10));
        subscription.wire.request_id = Some(12);
        let err = generate_wire_table(&api(vec![subscription]), 2)
            .expect_err("subscription with request id must error");
        assert!(err.to_string().contains("must not use request wire ids"));
    }

    #[test]
    fn generate_wire_table_rejects_inferred_overflow() {
        let err = generate_wire_table(&api(vec![subscription_method("overflow", Some(253))]), 2)
            .expect_err("overflow must error");

        assert!(err.to_string().contains("wire id overflow"));
    }

    #[test]
    fn generate_wire_table_filters_methods_by_target_version() {
        let api = ApiDefinition {
            traits: vec![TraitDef {
                name: "Example".to_string(),
                module_path: Vec::new(),
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
            public_trait_order: Vec::new(),
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

        assert!(source.contains("export const EXAMPLE_LEGACY = {"));
        assert!(source.contains("  request: 2,"));
        assert!(!source.contains("FUTURE"));
        assert!(!source.contains("FUTURE_STREAM"));
    }

    #[test]
    fn generate_client_filters_empty_services_by_target_version() {
        let api = ApiDefinition {
            traits: vec![
                TraitDef {
                    name: "Legacy".to_string(),
                    module_path: Vec::new(),
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
                    module_path: Vec::new(),
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
            public_trait_order: vec!["Legacy".to_string(), "FutureOnly".to_string()],
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
    fn generate_client_selects_highest_shared_wrapper_variant() {
        let api = ApiDefinition {
            traits: vec![TraitDef {
                name: "Example".to_string(),
                module_path: Vec::new(),
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
                    wire: request_wire(Some(2)),
                    docs: None,
                }],
                docs: None,
            }],
            public_trait_order: vec!["Example".to_string()],
            types: vec![
                versioned_tuple_wrapper("ExampleRequest", "LegacyRequest", "LatestRequest"),
                versioned_tuple_wrapper("ExampleResponse", "LegacyResponse", "LatestResponse"),
            ],
        };

        let client_source = generate_client(&api, 2, 1).expect("generate client");

        // V2 is the highest variant supported by every wrapper at or below the
        // target version. The codegen prefers the newest shared variant so
        // callers see the latest request/response shape the host advertises.
        assert!(client_source.contains("request: T.LatestRequest"));
        assert!(
            client_source.contains(
                "payload: T.VersionedExampleRequest.enc({ tag: \"V2\", value: request }),"
            )
        );
        assert!(client_source.contains("ResultAsync<T.LatestResponse, undefined>"));
    }

    #[test]
    fn generate_client_uses_only_existing_wrapper_variant() {
        let api = ApiDefinition {
            traits: vec![TraitDef {
                name: "Example".to_string(),
                module_path: Vec::new(),
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
                    wire: request_wire(Some(2)),
                    docs: None,
                }],
                docs: None,
            }],
            public_trait_order: vec!["Example".to_string()],
            types: vec![
                versioned_tuple_wrapper_variants("ExampleRequest", &[(1, "LegacyRequest")]),
                versioned_tuple_wrapper("ExampleResponse", "LegacyResponse", "LatestResponse"),
            ],
        };

        let client_source = generate_client(&api, 2, 1).expect("generate client");

        assert!(client_source.contains("request: T.LegacyRequest"));
        assert!(
            client_source.contains(
                "payload: T.VersionedExampleRequest.enc({ tag: \"V1\", value: request }),"
            )
        );
        assert!(client_source.contains("ResultAsync<T.LegacyResponse, undefined>"));
    }

    #[test]
    fn generate_client_rejects_named_field_versioned_wrapper() {
        let api = ApiDefinition {
            traits: vec![TraitDef {
                name: "Example".to_string(),
                module_path: Vec::new(),
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
                    wire: request_wire(Some(2)),
                    docs: None,
                }],
                docs: None,
            }],
            public_trait_order: Vec::new(),
            types: vec![
                named_field_versioned_wrapper("ExampleRequest"),
                versioned_tuple_wrapper("ExampleResponse", "LegacyResponse", "LatestResponse"),
            ],
        };

        let err = generate_client(&api, 2, 1).expect_err("named field wrapper rejected");

        assert!(err.to_string().contains("uses named fields"));
    }
}
