//! TypeScript code generation from extracted API definitions.

use std::collections::{BTreeMap, BTreeSet, HashMap};
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

fn strip_playground_doc_blocks(text: &str) -> String {
    let mut out = Vec::new();
    let mut in_truapi_doc_block = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if is_truapi_doc_block_start(trimmed) {
            in_truapi_doc_block = true;
            continue;
        }
        if in_truapi_doc_block && trimmed == "```" {
            in_truapi_doc_block = false;
            continue;
        }
        if !in_truapi_doc_block {
            out.push(line);
        }
    }
    trim_doc_lines(&out).unwrap_or_default()
}

fn is_truapi_doc_block_start(trimmed: &str) -> bool {
    trimmed == "```truapi-playground-request" || trimmed == "```truapi-client-example"
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

/// Generates playground-only service metadata from the same Rustdoc API input
/// used by the client generator.
pub fn generate_playground_services(
    api: &ApiDefinition,
    output_dir: &str,
    target_version: u32,
) -> Result<()> {
    fs::create_dir_all(output_dir)?;
    validate_versioned_wrapper_shapes(api)?;

    let code = generate_playground_services_code(api, target_version)?;
    fs::write(Path::new(output_dir).join("services.ts"), code)?;

    Ok(())
}

/// Generates static registry data consumed by the GitHub Pages explorer.
pub fn generate_explorer_registry(
    api: &ApiDefinition,
    output_dir: &str,
    target_version: u32,
) -> Result<()> {
    fs::create_dir_all(output_dir)?;
    validate_versioned_wrapper_shapes(api)?;

    let code = generate_explorer_registry_code(api, target_version)?;
    fs::write(Path::new(output_dir).join("registry.ts"), code)?;

    Ok(())
}

fn generate_index() -> String {
    "export * from './types.js';\nexport * from './client.js';\n".to_string()
}

fn generate_explorer_registry_code(api: &ApiDefinition, target_version: u32) -> Result<String> {
    let wrappers = collect_versioned_wrappers(api);
    let ctx = CodecContext::default();
    let mut out = String::new();

    writeln!(out, "// Auto-generated by truapi-codegen. Do not edit.").unwrap();
    writeln!(out).unwrap();
    writeln!(
        out,
        "export type ExplorerPattern = \"unary\" | \"subscription\";"
    )
    .unwrap();
    writeln!(out).unwrap();
    writeln!(out, "export interface ExplorerField {{").unwrap();
    writeln!(out, "  name: string;").unwrap();
    writeln!(out, "  type: string;").unwrap();
    writeln!(out, "  description?: string;").unwrap();
    writeln!(out, "}}").unwrap();
    writeln!(out).unwrap();
    writeln!(out, "export interface ExplorerVariant {{").unwrap();
    writeln!(out, "  name: string;").unwrap();
    writeln!(out, "  type: string;").unwrap();
    writeln!(out, "  description?: string;").unwrap();
    writeln!(out, "}}").unwrap();
    writeln!(out).unwrap();
    writeln!(out, "export interface ExplorerType {{").unwrap();
    writeln!(out, "  id: string;").unwrap();
    writeln!(out, "  name: string;").unwrap();
    writeln!(out, "  category: string;").unwrap();
    writeln!(out, "  definition: string;").unwrap();
    writeln!(out, "  description?: string;").unwrap();
    writeln!(out, "  source: string;").unwrap();
    writeln!(out, "  fields?: ExplorerField[];").unwrap();
    writeln!(out, "  variants?: ExplorerVariant[];").unwrap();
    writeln!(out, "}}").unwrap();
    writeln!(out).unwrap();
    writeln!(out, "export interface ExplorerMethod {{").unwrap();
    writeln!(out, "  id: string;").unwrap();
    writeln!(out, "  name: string;").unwrap();
    writeln!(out, "  groupId: string;").unwrap();
    writeln!(out, "  groupName: string;").unwrap();
    writeln!(out, "  wireId: number;").unwrap();
    writeln!(out, "  pattern: ExplorerPattern;").unwrap();
    writeln!(out, "  request: string;").unwrap();
    writeln!(out, "  response: string;").unwrap();
    writeln!(out, "  errorType?: string;").unwrap();
    writeln!(out, "  description?: string;").unwrap();
    writeln!(out, "  usageExample?: string;").unwrap();
    writeln!(out, "}}").unwrap();
    writeln!(out).unwrap();
    writeln!(out, "export interface ExplorerGroup {{").unwrap();
    writeln!(out, "  id: string;").unwrap();
    writeln!(out, "  name: string;").unwrap();
    writeln!(out, "  description?: string;").unwrap();
    writeln!(out, "  methods: string[];").unwrap();
    writeln!(out, "}}").unwrap();
    writeln!(out).unwrap();
    writeln!(out, "export interface ExplorerVersion {{").unwrap();
    writeln!(out, "  id: string;").unwrap();
    writeln!(out, "  label: string;").unwrap();
    writeln!(out, "  slug: string;").unwrap();
    writeln!(out, "  status: \"stable\" | \"preview\";").unwrap();
    writeln!(out, "  groups: ExplorerGroup[];").unwrap();
    writeln!(out, "  methods: ExplorerMethod[];").unwrap();
    writeln!(out, "  dataTypes: ExplorerType[];").unwrap();
    writeln!(out, "}}").unwrap();
    writeln!(out).unwrap();
    writeln!(out, "export const versions: ExplorerVersion[] = [").unwrap();

    for version in 1..=target_version {
        let groups = explorer_groups(api, &wrappers, version)?;
        let methods = explorer_methods(api, &wrappers, &ctx, version)?;
        let types = explorer_types(api, &wrappers, version)?;
        writeln!(out, "  {{").unwrap();
        writeln!(
            out,
            "    id: {},",
            ts_string_literal(&protocol_minor(version))
        )
        .unwrap();
        writeln!(
            out,
            "    label: {},",
            ts_string_literal(&format!("v{}", protocol_minor(version)))
        )
        .unwrap();
        writeln!(
            out,
            "    slug: {},",
            ts_string_literal(&protocol_minor(version))
        )
        .unwrap();
        writeln!(out, "    status: \"stable\",").unwrap();
        write_explorer_groups_array(&mut out, "    ", &groups);
        write_explorer_methods_array(&mut out, "    ", &methods);
        write_explorer_types_array(&mut out, "    ", &types);
        writeln!(out, "  }},").unwrap();
    }

    writeln!(out, "];").unwrap();
    writeln!(out).unwrap();
    writeln!(
        out,
        "export const defaultVersion: ExplorerVersion = versions[versions.length - 1];"
    )
    .unwrap();
    writeln!(out).unwrap();
    writeln!(
        out,
        "export function getVersion(slug: string): ExplorerVersion | undefined {{"
    )
    .unwrap();
    writeln!(
        out,
        "  return versions.find((version) => version.slug === slug);"
    )
    .unwrap();
    writeln!(out, "}}").unwrap();
    writeln!(out).unwrap();
    writeln!(
        out,
        "export function getTypeById(version: ExplorerVersion, id: string): ExplorerType | undefined {{"
    )
    .unwrap();
    writeln!(
        out,
        "  return version.dataTypes.find((typeDef) => typeDef.id === id);"
    )
    .unwrap();
    writeln!(out, "}}").unwrap();
    writeln!(out).unwrap();
    writeln!(
        out,
        "export function getMethodById(version: ExplorerVersion, id: string): ExplorerMethod | undefined {{"
    )
    .unwrap();
    writeln!(
        out,
        "  return version.methods.find((method) => method.id === id);"
    )
    .unwrap();
    writeln!(out, "}}").unwrap();

    Ok(out)
}

#[derive(Debug)]
struct ExplorerGroupRecord {
    id: String,
    name: String,
    description: Option<String>,
    methods: Vec<String>,
}

#[derive(Debug)]
struct ExplorerMethodRecord {
    id: String,
    name: String,
    group_id: String,
    group_name: String,
    wire_id: u8,
    pattern: &'static str,
    request: String,
    response: String,
    error_type: Option<String>,
    description: Option<String>,
    client_example: Option<String>,
}

#[derive(Debug)]
struct ExplorerTypeRecord {
    id: String,
    name: String,
    category: String,
    definition: String,
    description: Option<String>,
    source: String,
    fields: Vec<ExplorerFieldRecord>,
    variants: Vec<ExplorerVariantRecord>,
}

#[derive(Debug)]
struct ExplorerFieldRecord {
    name: String,
    ty: String,
    description: Option<String>,
}

#[derive(Debug)]
struct ExplorerVariantRecord {
    name: String,
    ty: String,
    description: Option<String>,
}

fn explorer_groups(
    api: &ApiDefinition,
    wrappers: &HashMap<String, VersionedWrapper>,
    target_version: u32,
) -> Result<Vec<ExplorerGroupRecord>> {
    let mut traits = explorer_traits(api);
    let mut groups = Vec::new();
    for trait_def in traits.drain(..) {
        let methods = included_methods(trait_def, wrappers, target_version)?
            .into_iter()
            .map(|method| method.name.clone())
            .collect::<Vec<_>>();
        if methods.is_empty() {
            continue;
        }
        groups.push(ExplorerGroupRecord {
            id: explorer_group_id(&trait_def.name),
            name: service_name(&trait_def.name),
            description: trait_def
                .docs
                .as_deref()
                .map(strip_playground_doc_blocks)
                .filter(|docs| !docs.is_empty()),
            methods,
        });
    }
    Ok(groups)
}

fn explorer_methods(
    api: &ApiDefinition,
    wrappers: &HashMap<String, VersionedWrapper>,
    ctx: &CodecContext,
    target_version: u32,
) -> Result<Vec<ExplorerMethodRecord>> {
    let mut methods = Vec::new();
    for trait_def in explorer_traits(api) {
        let mut included = included_methods(trait_def, wrappers, target_version)?;
        included.sort_by_key(|method| (method.wire_id.unwrap_or(u8::MAX), method.name.as_str()));
        for method in included {
            let wire_version = method_wire_version(method, wrappers, target_version)?;
            let payload = emit_payload(&method.params, wrappers, ctx, wire_version)?;
            let docs = split_playground_docs(method.docs.as_deref(), &method.name)?;
            let (response, error_type) = explorer_response(method, wrappers, ctx, wire_version)?;
            methods.push(ExplorerMethodRecord {
                id: method.name.clone(),
                name: method.name.clone(),
                group_id: explorer_group_id(&trait_def.name),
                group_name: service_name(&trait_def.name),
                wire_id: method.wire_id.expect("validated by included_methods"),
                pattern: match method.kind {
                    MethodKind::Request => "unary",
                    MethodKind::Subscription | MethodKind::ResultSubscription => "subscription",
                },
                request: if method.name == "host_handshake" || payload.param_list.is_empty() {
                    "undefined".to_string()
                } else {
                    playground_type_name(&payload.inner_type_ts)
                },
                response,
                error_type,
                description: docs.description,
                client_example: docs.client_example,
            });
        }
    }
    methods.sort_by_key(|method| method.wire_id);
    Ok(methods)
}

fn explorer_response(
    method: &MethodDef,
    wrappers: &HashMap<String, VersionedWrapper>,
    ctx: &CodecContext,
    wire_version: Option<u32>,
) -> Result<(String, Option<String>)> {
    match (&method.kind, &method.return_type) {
        (MethodKind::Request, ReturnType::Result { ok, err }) => {
            let ok = emit_response(ok, wrappers, ctx, wire_version)?;
            let err = emit_error_response(err, wrappers, ctx, wire_version)?;
            Ok((
                playground_type_name(&ok.inner_type_ts),
                Some(playground_type_name(&err.inner_type_ts)),
            ))
        }
        (MethodKind::Subscription, ReturnType::Subscription(item)) => {
            let item = emit_response(item, wrappers, ctx, wire_version)?;
            Ok((playground_type_name(&item.inner_type_ts), None))
        }
        (MethodKind::ResultSubscription, ReturnType::ResultSubscription { item, err }) => {
            let item = emit_response(item, wrappers, ctx, wire_version)?;
            let err = emit_error_response(err, wrappers, ctx, wire_version)?;
            Ok((
                playground_type_name(&item.inner_type_ts),
                Some(playground_type_name(&err.inner_type_ts)),
            ))
        }
        (kind, return_type) => bail!(
            "Explorer internal mismatch for method `{}`: kind {:?} does not match return type {:?}",
            method.name,
            kind,
            return_type
        ),
    }
}

fn explorer_types(
    api: &ApiDefinition,
    wrappers: &HashMap<String, VersionedWrapper>,
    target_version: u32,
) -> Result<Vec<ExplorerTypeRecord>> {
    let visible = explorer_visible_type_names(api, wrappers, target_version)?;
    let mut types = Vec::new();
    for ty in &api.types {
        if !visible.contains(&ty.name) {
            continue;
        }
        types.push(explorer_type_record(ty, target_version)?);
    }
    types.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(types)
}

fn explorer_visible_type_names(
    api: &ApiDefinition,
    wrappers: &HashMap<String, VersionedWrapper>,
    target_version: u32,
) -> Result<BTreeSet<String>> {
    let mut names = BTreeSet::new();
    for trait_def in explorer_traits(api) {
        for method in included_methods(trait_def, wrappers, target_version)? {
            let wire_version = method_wire_version(method, wrappers, target_version)?;
            for param in &method.params {
                collect_explorer_type_ref(&param.type_ref, wrappers, wire_version, &mut names);
            }
            match &method.return_type {
                ReturnType::Result { ok, err } => {
                    collect_explorer_type_ref(ok, wrappers, wire_version, &mut names);
                    collect_explorer_type_ref(
                        call_error_inner(err).unwrap_or(err),
                        wrappers,
                        wire_version,
                        &mut names,
                    );
                }
                ReturnType::Subscription(item) => {
                    collect_explorer_type_ref(item, wrappers, wire_version, &mut names);
                }
                ReturnType::ResultSubscription { item, err } => {
                    collect_explorer_type_ref(item, wrappers, wire_version, &mut names);
                    collect_explorer_type_ref(
                        call_error_inner(err).unwrap_or(err),
                        wrappers,
                        wire_version,
                        &mut names,
                    );
                }
            }
        }
    }

    loop {
        let before = names.len();
        for ty in &api.types {
            if names.contains(&ty.name) {
                collect_explorer_type_def_refs(ty, wrappers, target_version, &mut names);
            }
        }
        if names.len() == before {
            break;
        }
    }

    Ok(names)
}

fn collect_explorer_type_def_refs(
    ty: &TypeDef,
    wrappers: &HashMap<String, VersionedWrapper>,
    target_version: u32,
    names: &mut BTreeSet<String>,
) {
    match &ty.kind {
        TypeDefKind::Alias(type_ref) => {
            collect_explorer_type_ref(type_ref, wrappers, None, names);
        }
        TypeDefKind::Struct(fields) => {
            for field in fields {
                collect_explorer_type_ref(&field.type_ref, wrappers, None, names);
            }
        }
        TypeDefKind::TupleStruct(items) => {
            for item in items {
                collect_explorer_type_ref(item, wrappers, None, names);
            }
        }
        TypeDefKind::Enum(variants) => {
            for variant in variants {
                if version_number(&variant.name).is_some_and(|version| version > target_version) {
                    continue;
                }
                match &variant.fields {
                    VariantFields::Unit => {}
                    VariantFields::Unnamed(items) => {
                        for item in items {
                            collect_explorer_type_ref(item, wrappers, None, names);
                        }
                    }
                    VariantFields::Named(fields) => {
                        for field in fields {
                            collect_explorer_type_ref(&field.type_ref, wrappers, None, names);
                        }
                    }
                }
            }
        }
    }
}

fn collect_explorer_type_ref(
    ty: &TypeRef,
    wrappers: &HashMap<String, VersionedWrapper>,
    wire_version: Option<u32>,
    names: &mut BTreeSet<String>,
) {
    match ty {
        TypeRef::Named { name, args } => {
            names.insert(name.clone());
            if args.is_empty() {
                if let Some(wrapper) = wrappers.get(name) {
                    let versions = match wire_version {
                        Some(version) => vec![version],
                        None => wrapper.variants.keys().copied().collect::<Vec<_>>(),
                    };
                    for version in versions {
                        if let Some(variant) = wrapper.variants.get(&version) {
                            if let VersionedKind::Tuple(inner) = &variant.kind {
                                collect_explorer_type_ref(inner, wrappers, None, names);
                            }
                        }
                    }
                }
            }
            for arg in args {
                collect_explorer_type_ref(arg, wrappers, None, names);
            }
        }
        TypeRef::Vec(inner) | TypeRef::Option(inner) | TypeRef::Array(inner, _) => {
            collect_explorer_type_ref(inner, wrappers, None, names);
        }
        TypeRef::Tuple(items) => {
            for item in items {
                collect_explorer_type_ref(item, wrappers, None, names);
            }
        }
        TypeRef::Primitive(_) | TypeRef::Generic(_) | TypeRef::Unit => {}
    }
}

fn explorer_type_record(ty: &TypeDef, target_version: u32) -> Result<ExplorerTypeRecord> {
    let mut fields = Vec::new();
    let mut variants = Vec::new();
    match &ty.kind {
        TypeDefKind::Struct(items) => {
            for field in items {
                let (name, optional) = ts_field_name(&field.name, &field.type_ref);
                fields.push(ExplorerFieldRecord {
                    name,
                    ty: if optional {
                        ts_inner_option(&field.type_ref)?
                    } else {
                        ts_type(&field.type_ref)?
                    },
                    description: clean_explorer_docs(field.docs.as_deref()),
                });
            }
        }
        TypeDefKind::Enum(items) => {
            for variant in items {
                let include_variant =
                    version_number(&variant.name).is_none_or(|version| version <= target_version);
                if !include_variant {
                    continue;
                }
                variants.push(ExplorerVariantRecord {
                    name: variant.name.clone(),
                    ty: variant_value_type(&variant.fields)?,
                    description: clean_explorer_docs(variant.docs.as_deref()),
                });
            }
        }
        TypeDefKind::Alias(_) | TypeDefKind::TupleStruct(_) => {}
    }

    Ok(ExplorerTypeRecord {
        id: ty.name.clone(),
        name: ty.name.clone(),
        category: explorer_type_category(ty).to_string(),
        definition: explorer_type_definition(ty, target_version)?,
        description: clean_explorer_docs(ty.docs.as_deref()),
        source: explorer_type_source(&ty.name),
        fields,
        variants,
    })
}

fn explorer_type_definition(ty: &TypeDef, target_version: u32) -> Result<String> {
    let generic_decl = generic_param_declaration(&ty.generic_params);
    match &ty.kind {
        TypeDefKind::Alias(type_ref) => Ok(format!(
            "type {}{} = {}",
            ty.name,
            generic_decl,
            ts_type(type_ref)?
        )),
        TypeDefKind::Struct(fields) => Ok(format!(
            "interface {}{} {{ {} }}",
            ty.name,
            generic_decl,
            fields
                .iter()
                .map(|field| {
                    let (name, optional) = ts_field_name(&field.name, &field.type_ref);
                    if optional {
                        Ok(format!("{}?: {}", name, ts_inner_option(&field.type_ref)?))
                    } else {
                        Ok(format!("{}: {}", name, ts_type(&field.type_ref)?))
                    }
                })
                .collect::<Result<Vec<_>>>()?
                .join("; ")
        )),
        TypeDefKind::TupleStruct(types) => Ok(format!(
            "type {}{} = {}",
            ty.name,
            generic_decl,
            unnamed_fields_type(types)?
        )),
        TypeDefKind::Enum(variants) => {
            let rendered = variants
                .iter()
                .filter(|variant| {
                    version_number(&variant.name).is_none_or(|version| version <= target_version)
                })
                .map(|variant| {
                    Ok(format!(
                        "{{ tag: \"{}\"; value: {} }}",
                        variant.name,
                        variant_value_type(&variant.fields)?
                    ))
                })
                .collect::<Result<Vec<_>>>()?
                .join(" | ");
            Ok(format!("type {}{} = {}", ty.name, generic_decl, rendered))
        }
    }
}

fn explorer_type_category(ty: &TypeDef) -> &'static str {
    match &ty.kind {
        TypeDefKind::Alias(_) => "alias",
        TypeDefKind::Struct(_) => "struct",
        TypeDefKind::TupleStruct(_) => "tuple",
        TypeDefKind::Enum(variants)
            if !variants.is_empty()
                && variants.iter().all(|v| is_versioned_variant_name(&v.name)) =>
        {
            "versioned"
        }
        TypeDefKind::Enum(_) => "enum",
    }
}

fn explorer_type_source(name: &str) -> String {
    if name.starts_with("V01") {
        "v0.1".to_string()
    } else if name.starts_with("V02") {
        "v0.2".to_string()
    } else {
        "shared".to_string()
    }
}

fn clean_explorer_docs(docs: Option<&str>) -> Option<String> {
    docs.map(strip_playground_doc_blocks)
        .filter(|docs| !docs.is_empty())
}

fn explorer_traits(api: &ApiDefinition) -> Vec<&TraitDef> {
    let mut traits: Vec<&TraitDef> = api
        .traits
        .iter()
        .filter(|trait_def| service_order(&trait_def.name).is_some())
        .collect();
    traits.sort_by_key(|trait_def| {
        (
            service_order(&trait_def.name).unwrap_or(usize::MAX),
            trait_def.name.as_str(),
        )
    });
    traits
}

fn write_explorer_groups_array(out: &mut String, indent: &str, groups: &[ExplorerGroupRecord]) {
    writeln!(out, "{}groups: [", indent).unwrap();
    for group in groups {
        writeln!(out, "{}  {{", indent).unwrap();
        writeln!(out, "{}    id: {},", indent, ts_string_literal(&group.id)).unwrap();
        writeln!(
            out,
            "{}    name: {},",
            indent,
            ts_string_literal(&group.name)
        )
        .unwrap();
        if let Some(description) = &group.description {
            writeln!(
                out,
                "{}    description: {},",
                indent,
                ts_string_literal(description)
            )
            .unwrap();
        }
        writeln!(out, "{}    methods: [", indent).unwrap();
        for method in &group.methods {
            writeln!(out, "{}      {},", indent, ts_string_literal(method)).unwrap();
        }
        writeln!(out, "{}    ],", indent).unwrap();
        writeln!(out, "{}  }},", indent).unwrap();
    }
    writeln!(out, "{}],", indent).unwrap();
}

fn write_explorer_methods_array(out: &mut String, indent: &str, methods: &[ExplorerMethodRecord]) {
    writeln!(out, "{}methods: [", indent).unwrap();
    for method in methods {
        writeln!(out, "{}  {{", indent).unwrap();
        writeln!(out, "{}    id: {},", indent, ts_string_literal(&method.id)).unwrap();
        writeln!(
            out,
            "{}    name: {},",
            indent,
            ts_string_literal(&method.name)
        )
        .unwrap();
        writeln!(
            out,
            "{}    groupId: {},",
            indent,
            ts_string_literal(&method.group_id)
        )
        .unwrap();
        writeln!(
            out,
            "{}    groupName: {},",
            indent,
            ts_string_literal(&method.group_name)
        )
        .unwrap();
        writeln!(out, "{}    wireId: {},", indent, method.wire_id).unwrap();
        writeln!(
            out,
            "{}    pattern: {},",
            indent,
            ts_string_literal(method.pattern)
        )
        .unwrap();
        writeln!(
            out,
            "{}    request: {},",
            indent,
            ts_string_literal(&method.request)
        )
        .unwrap();
        writeln!(
            out,
            "{}    response: {},",
            indent,
            ts_string_literal(&method.response)
        )
        .unwrap();
        if let Some(error_type) = &method.error_type {
            writeln!(
                out,
                "{}    errorType: {},",
                indent,
                ts_string_literal(error_type)
            )
            .unwrap();
        }
        if let Some(description) = &method.description {
            writeln!(
                out,
                "{}    description: {},",
                indent,
                ts_string_literal(description)
            )
            .unwrap();
        }
        if let Some(client_example) = &method.client_example {
            writeln!(
                out,
                "{}    usageExample: {},",
                indent,
                ts_string_literal(client_example)
            )
            .unwrap();
        }
        writeln!(out, "{}  }},", indent).unwrap();
    }
    writeln!(out, "{}],", indent).unwrap();
}

fn write_explorer_types_array(out: &mut String, indent: &str, types: &[ExplorerTypeRecord]) {
    writeln!(out, "{}dataTypes: [", indent).unwrap();
    for ty in types {
        writeln!(out, "{}  {{", indent).unwrap();
        writeln!(out, "{}    id: {},", indent, ts_string_literal(&ty.id)).unwrap();
        writeln!(out, "{}    name: {},", indent, ts_string_literal(&ty.name)).unwrap();
        writeln!(
            out,
            "{}    category: {},",
            indent,
            ts_string_literal(&ty.category)
        )
        .unwrap();
        writeln!(
            out,
            "{}    definition: {},",
            indent,
            ts_string_literal(&ty.definition)
        )
        .unwrap();
        if let Some(description) = &ty.description {
            writeln!(
                out,
                "{}    description: {},",
                indent,
                ts_string_literal(description)
            )
            .unwrap();
        }
        writeln!(
            out,
            "{}    source: {},",
            indent,
            ts_string_literal(&ty.source)
        )
        .unwrap();
        if !ty.fields.is_empty() {
            writeln!(out, "{}    fields: [", indent).unwrap();
            for field in &ty.fields {
                writeln!(out, "{}      {{", indent).unwrap();
                writeln!(
                    out,
                    "{}        name: {},",
                    indent,
                    ts_string_literal(&field.name)
                )
                .unwrap();
                writeln!(
                    out,
                    "{}        type: {},",
                    indent,
                    ts_string_literal(&field.ty)
                )
                .unwrap();
                if let Some(description) = &field.description {
                    writeln!(
                        out,
                        "{}        description: {},",
                        indent,
                        ts_string_literal(description)
                    )
                    .unwrap();
                }
                writeln!(out, "{}      }},", indent).unwrap();
            }
            writeln!(out, "{}    ],", indent).unwrap();
        }
        if !ty.variants.is_empty() {
            writeln!(out, "{}    variants: [", indent).unwrap();
            for variant in &ty.variants {
                writeln!(out, "{}      {{", indent).unwrap();
                writeln!(
                    out,
                    "{}        name: {},",
                    indent,
                    ts_string_literal(&variant.name)
                )
                .unwrap();
                writeln!(
                    out,
                    "{}        type: {},",
                    indent,
                    ts_string_literal(&variant.ty)
                )
                .unwrap();
                if let Some(description) = &variant.description {
                    writeln!(
                        out,
                        "{}        description: {},",
                        indent,
                        ts_string_literal(description)
                    )
                    .unwrap();
                }
                writeln!(out, "{}      }},", indent).unwrap();
            }
            writeln!(out, "{}    ],", indent).unwrap();
        }
        writeln!(out, "{}  }},", indent).unwrap();
    }
    writeln!(out, "{}],", indent).unwrap();
}

fn protocol_minor(version: u32) -> String {
    format!("0.{}", version)
}

fn explorer_group_id(name: &str) -> String {
    to_kebab_case(&service_name(name))
}

fn to_kebab_case(value: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = false;
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            prev_dash = false;
        } else if !prev_dash && !out.is_empty() {
            out.push('-');
            prev_dash = true;
        }
    }
    if out.ends_with('-') {
        out.pop();
    }
    out
}

fn generate_playground_services_code(api: &ApiDefinition, target_version: u32) -> Result<String> {
    let wrappers = collect_versioned_wrappers(api);
    let ctx = CodecContext::default();
    let mut traits: Vec<&TraitDef> = api
        .traits
        .iter()
        .filter(|trait_def| service_order(&trait_def.name).is_some())
        .collect();
    traits.sort_by_key(|trait_def| {
        (
            service_order(&trait_def.name).unwrap_or(usize::MAX),
            trait_def.name.as_str(),
        )
    });

    let mut out = String::new();
    writeln!(out, "// Auto-generated by truapi-codegen. Do not edit.").unwrap();
    writeln!(out).unwrap();
    writeln!(out, "export interface MethodInfo {{").unwrap();
    writeln!(out, "  name: string;").unwrap();
    writeln!(out, "  type: \"unary\" | \"subscription\";").unwrap();
    writeln!(out, "  description?: string;").unwrap();
    writeln!(out, "  requestDescription?: string;").unwrap();
    writeln!(out, "  defaultRequest?: string;").unwrap();
    writeln!(out, "  noParams?: boolean;").unwrap();
    writeln!(out, "}}").unwrap();
    writeln!(out).unwrap();
    writeln!(out, "export interface ServiceInfo {{").unwrap();
    writeln!(out, "  name: string;").unwrap();
    writeln!(out, "  methods: MethodInfo[];").unwrap();
    writeln!(out, "}}").unwrap();
    writeln!(out).unwrap();
    writeln!(out, "export const services: ServiceInfo[] = [").unwrap();

    for trait_def in traits {
        let mut methods = included_methods(trait_def, &wrappers, target_version)?;
        methods.sort_by_key(|method| (method.wire_id.unwrap_or(u8::MAX), method.name.as_str()));
        if methods.is_empty() {
            continue;
        }

        writeln!(out, "  {{").unwrap();
        writeln!(
            out,
            "    name: {},",
            ts_string_literal(&service_name(&trait_def.name))
        )
        .unwrap();
        writeln!(out, "    methods: [").unwrap();

        for method in methods {
            let wire_version = method_wire_version(method, &wrappers, target_version)?;
            let payload = emit_payload(&method.params, &wrappers, &ctx, wire_version)?;
            let docs = split_playground_docs(method.docs.as_deref(), &method.name)?;
            let method_type = match method.kind {
                MethodKind::Request => "unary",
                MethodKind::Subscription | MethodKind::ResultSubscription => "subscription",
            };

            writeln!(out, "      {{").unwrap();
            writeln!(out, "        name: {},", ts_string_literal(&method.name)).unwrap();
            writeln!(out, "        type: {},", ts_string_literal(method_type)).unwrap();
            if let Some(description) = docs.description {
                writeln!(
                    out,
                    "        description: {},",
                    ts_string_literal(&description)
                )
                .unwrap();
            }
            let no_params = method.name == "host_handshake" || payload.param_list.is_empty();
            if !no_params {
                writeln!(
                    out,
                    "        requestDescription: {},",
                    ts_string_literal(&playground_type_name(&payload.inner_type_ts))
                )
                .unwrap();
            }
            if no_params {
                writeln!(out, "        noParams: true,").unwrap();
            } else if let Some(default_request) = docs.default_request {
                writeln!(
                    out,
                    "        defaultRequest: {},",
                    ts_string_literal(&default_request)
                )
                .unwrap();
            }
            writeln!(out, "      }},").unwrap();
        }

        writeln!(out, "    ],").unwrap();
        writeln!(out, "  }},").unwrap();
    }

    writeln!(out, "];").unwrap();

    Ok(out)
}

fn service_order(name: &str) -> Option<usize> {
    [
        "TrUApiCalls",
        "Permissions",
        "LocalStorage",
        "AccountManagement",
        "Signing",
        "Chat",
        "StatementStore",
        "Preimage",
        "ChainInteraction",
        "Payment",
        "EntropyDerivation",
    ]
    .iter()
    .position(|candidate| *candidate == name)
}

fn service_name(name: &str) -> String {
    match name {
        "TrUApiCalls" => "TrUAPI Calls".to_string(),
        "LocalStorage" => "Local Storage".to_string(),
        "AccountManagement" => "Account Management".to_string(),
        "StatementStore" => "Statement Store".to_string(),
        "ChainInteraction" => "Chain Interaction".to_string(),
        "EntropyDerivation" => "Entropy Derivation".to_string(),
        other => split_pascal_case(other),
    }
}

fn split_pascal_case(name: &str) -> String {
    let mut out = String::new();
    let mut prev_lowercase = false;
    for ch in name.chars() {
        if ch.is_uppercase() && prev_lowercase {
            out.push(' ');
        }
        out.push(ch);
        prev_lowercase = ch.is_lowercase();
    }
    out
}

#[derive(Debug)]
struct PlaygroundDocs {
    description: Option<String>,
    default_request: Option<String>,
    client_example: Option<String>,
}

fn split_playground_docs(docs: Option<&str>, method_name: &str) -> Result<PlaygroundDocs> {
    let Some(docs) = docs else {
        return Ok(PlaygroundDocs {
            description: None,
            default_request: None,
            client_example: None,
        });
    };

    let mut description = Vec::new();
    let mut example = Vec::new();
    let mut client_example = Vec::new();
    let mut in_playground_example = false;
    let mut in_client_example = false;
    for line in docs.lines() {
        let trimmed = line.trim();
        if trimmed == "```truapi-playground-request" {
            in_playground_example = true;
            continue;
        }
        if trimmed == "```truapi-client-example" {
            in_client_example = true;
            continue;
        }
        if in_playground_example && trimmed == "```" {
            in_playground_example = false;
            continue;
        }
        if in_client_example && trimmed == "```" {
            in_client_example = false;
            continue;
        }
        if in_playground_example {
            example.push(line);
        } else if in_client_example {
            client_example.push(line);
        } else {
            description.push(line);
        }
    }

    let description = trim_doc_lines(&description);
    let default_request = trim_doc_lines(&example);
    if let Some(default_request) = &default_request {
        serde_json::from_str::<serde_json::Value>(default_request).map_err(|err| {
            anyhow::anyhow!(
                "invalid truapi-playground-request JSON for `{}`: {}",
                method_name,
                err
            )
        })?;
    }

    Ok(PlaygroundDocs {
        description,
        default_request,
        client_example: trim_doc_lines(&client_example),
    })
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

fn playground_type_name(value: &str) -> String {
    value.replace("T.", "")
}

fn ts_string_literal(value: &str) -> String {
    serde_json::to_string(value).expect("string serialization is infallible")
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

fn generate_types(api: &ApiDefinition, target_version: u32) -> Result<String> {
    let mut out = String::new();
    writeln!(out, "// Auto-generated by truapi-codegen. Do not edit.").unwrap();
    writeln!(out).unwrap();
    writeln!(out, "import * as S from '../scale.js';").unwrap();
    writeln!(out).unwrap();

    let wrappers = collect_versioned_wrappers(api);
    let emit_versions = versioned_wrapper_emit_versions(api, &wrappers, target_version)?;

    for ty in &api.types {
        write_type_definition(&mut out, ty, &emit_versions)?;
        writeln!(out).unwrap();
        write_codec_definition(&mut out, ty, &emit_versions)?;
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
    writeln!(out, "function toError(error: unknown): Error {{").unwrap();
    writeln!(
        out,
        "  return error instanceof Error ? error : new Error(String(error));"
    )
    .unwrap();
    writeln!(out, "}}").unwrap();
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
    // handing bytes to the transport. The host-side dispatcher decodes the
    // full wrapper (variant byte included) from the wire payload.
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

    if params.is_empty() {
        // No-param methods (subscribe-with-no-start-payload, etc.) still need
        // a versioned envelope on the wire so legacy hosts that decode an
        // `Enum({v1: _void})` payload receive at least the version byte.
        let version = wire_version.unwrap_or(1);
        let wire_codec_expr =
            indexed_versioned_codec_expr(std::iter::once((version, "S.unit".to_string())))?;
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
        param_list: format!("request: {}", inner_type_ts),
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

/// HACK: every emitted variant pins its wire discriminant to `0` regardless
/// of the declared version. This works around `triangle-js-sdks` hosts that
/// don't correctly route on the SCALE version prefix and instead treat the
/// payload as if the codec had no version envelope at all. Safe today
/// because [`versioned_wrapper_emit_versions`] reduces every method-envelope
/// wrapper to a single emitted variant, so there is no collision between
/// V1 and V2 sharing index 0. Remove once hosts decode the version byte
/// correctly; the Rust side already pins each `Vn` arm to
/// `#[codec(index = n - 1)]` and is unaffected.
fn indexed_versioned_codec_expr(
    variants: impl IntoIterator<Item = (u32, String)>,
) -> Result<String> {
    let mut entries = Vec::new();
    for (version, codec) in variants {
        // Validate version is non-zero (V0 is reserved/invalid).
        version
            .checked_sub(1)
            .ok_or_else(|| anyhow::anyhow!("versioned wrapper uses invalid V0 variant"))?;
        let index = 0u32;
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
                "{ codecVersion: this.transport.codecVersion }"
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
/// method-specific input fields with the universal subscription callbacks.
/// `_interrupt` is payloadless for compatibility, so generated methods decode
/// only `_receive` payloads.
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
        "Pick<SubscribeCallbacks<{}>, 'onData' | 'onInterrupt' | 'onError' | 'onClose'>",
        item_type_ts
    );
    let args_type = if payload.param_list.is_empty() {
        pick
    } else {
        format!("{{ {} }} & {}", payload.param_list, pick)
    };

    let mut destructure = vec![
        "onData".to_string(),
        "onInterrupt".to_string(),
        "onError".to_string(),
        "onClose".to_string(),
    ];
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
    writeln!(out, "      onData: (payload) => {{").unwrap();
    writeln!(out, "        let item: {}; ", item_type_ts).unwrap();
    writeln!(out, "        try {{").unwrap();
    writeln!(out, "          item = {}; ", item_value).unwrap();
    writeln!(out, "        }} catch (error) {{").unwrap();
    writeln!(out, "          onError?.(toError(error));").unwrap();
    writeln!(out, "          return;").unwrap();
    writeln!(out, "        }}").unwrap();
    writeln!(out, "        onData(item);").unwrap();
    writeln!(out, "      }},").unwrap();
    writeln!(out, "      onInterrupt,").unwrap();
    writeln!(out, "      onClose,").unwrap();
    writeln!(out, "    }});").unwrap();
    writeln!(out, "  }}").unwrap();

    Ok(())
}

fn write_type_definition(
    out: &mut String,
    ty: &TypeDef,
    emit_versions: &HashMap<String, BTreeSet<u32>>,
) -> Result<()> {
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
        TypeDefKind::TupleStruct(fields) => {
            writeln!(
                out,
                "export type {}{} = {};",
                ty.name,
                generic_decl,
                unnamed_fields_type(fields)?
            )
            .unwrap();
        }
        TypeDefKind::Enum(variants) => {
            // For versioned wrappers, only emit the variant(s) the client
            // actually wire-encodes. The wire byte index is preserved by the
            // codec definition (`indexed_versioned_codec_expr`).
            let selected = emit_versions.get(&ty.name);
            writeln!(out, "export type {}{} =", ty.name, generic_decl).unwrap();
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

fn write_codec_definition(
    out: &mut String,
    ty: &TypeDef,
    emit_versions: &HashMap<String, BTreeSet<u32>>,
) -> Result<()> {
    if ty.generic_params.is_empty() {
        let ctx = CodecContext::default();
        if let Some(wrapper) = detect_versioned_wrapper(ty) {
            let selected = emit_versions.get(&ty.name);
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
        TypeDefKind::TupleStruct(fields) => unnamed_fields_codec_expr(fields, false, ctx),
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
        VariantFields::Unnamed(types) => unnamed_fields_type(types),
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
        VariantFields::Unnamed(types) => unnamed_fields_codec_expr(types, qualified, ctx),
        VariantFields::Named(fields) => struct_codec_expr(
            fields,
            &inline_object_type(fields, qualified)?,
            qualified,
            ctx,
        ),
    }
}

fn unnamed_fields_type(types: &[TypeRef]) -> Result<String> {
    if types.is_empty() {
        Ok("undefined".to_string())
    } else if types.len() == 1 {
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

fn unnamed_fields_codec_expr(
    types: &[TypeRef],
    qualified: bool,
    ctx: &CodecContext,
) -> Result<String> {
    if types.is_empty() {
        Ok("S.unit".to_string())
    } else if types.len() == 1 {
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
    fn generate_client_selects_highest_shared_wrapper_variant() {
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

        // V2 is the highest variant supported by every wrapper at or below the
        // target version. The codegen prefers the newest shared variant so
        // callers see the latest request/response shape the host advertises.
        assert!(client_source.contains("request: T.LatestRequest"));
        assert!(client_source
            .contains("payload: T.ExampleRequest.enc({ tag: \"V2\", value: request }),"));
        assert!(client_source.contains("Promise<Result<T.LatestResponse, undefined>>"));
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
