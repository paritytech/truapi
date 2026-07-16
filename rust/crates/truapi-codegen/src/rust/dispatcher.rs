//! Emits `dispatcher.rs`: the server-side wire dispatcher that routes
//! incoming frames to the host trait implementation.
//!
//! For each method the emitter produces an `on_request` (or
//! `on_subscription`) registration that:
//! 1. SCALE-decodes the versioned request wrapper from the wire bytes.
//! 2. Calls the host trait method (which receives the wrapper directly
//!    and matches `_::V1(inner)` internally).
//! 3. SCALE-encodes the versioned response wrapper back onto the wire.
//!
//! The generated file expects to live inside a `truapi-server` crate
//! and references `crate::dispatcher::Dispatcher`. The codegen itself
//! does not compile the output; string-diff golden tests guard it.

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::fmt::Write;

use anyhow::{Context, Result, bail};
use indoc::{formatdoc, indoc, writedoc};

use crate::rustdoc::*;

use super::{const_name, module_for_trait, wire_method_name};

/// Emit the contents of `dispatcher.rs`.
pub fn generate_dispatcher(api: &ApiDefinition) -> Result<String> {
    let traits = order_traits(api)?;

    // Reject any duplicate wire method name across traits before emission, so
    // a future addition can't silently overwrite a handler in the HashMap.
    let mut seen: BTreeSet<String> = BTreeSet::new();
    for trait_def in &traits {
        for method in &trait_def.methods {
            let key = wire_method_name(&trait_def.name, &method.name);
            if !seen.insert(key.clone()) {
                bail!(
                    "Wire method name `{key}` registered twice; \
                     change `{}::{}` or its sibling trait to disambiguate",
                    trait_def.name,
                    method.name
                );
            }
        }
    }

    let mut modules = Vec::with_capacity(traits.len());
    let mut uses_raw_err_payload = false;
    let mut uses_raw_unit_ok_payload = false;
    for trait_def in &traits {
        let module = build_module(api, trait_def)?;
        uses_raw_err_payload |= module.uses_raw_err_payload;
        uses_raw_unit_ok_payload |= module.uses_raw_unit_ok_payload;
        modules.push(module.code);
    }

    let mut out = String::new();
    write_header(&mut out);
    write_imports(
        &mut out,
        &traits,
        uses_raw_err_payload,
        uses_raw_unit_ok_payload,
    );
    writeln!(out).unwrap();
    write_top_register(&mut out, &traits);

    for module in &modules {
        writeln!(out).unwrap();
        out.push_str(module);
    }

    Ok(out)
}

/// Returns the traits to emit, in the order declared by the top-level
/// `TrUApi` super-trait. Falls back to alphabetical order if the
/// extractor did not record a public ordering (e.g. synthetic tests).
fn order_traits(api: &ApiDefinition) -> Result<Vec<&TraitDef>> {
    let by_name: BTreeMap<&str, &TraitDef> =
        api.traits.iter().map(|t| (t.name.as_str(), t)).collect();

    if api.public_trait_order.is_empty() {
        return Ok(api.traits.iter().collect());
    }

    let mut ordered = Vec::with_capacity(api.public_trait_order.len());
    for name in &api.public_trait_order {
        let Some(trait_def) = by_name.get(name.as_str()) else {
            bail!("trait `{name}` appears in TrUApi but was not extracted");
        };
        ordered.push(*trait_def);
    }
    Ok(ordered)
}

struct ModuleEmission {
    code: String,
    uses_raw_err_payload: bool,
    uses_raw_unit_ok_payload: bool,
}

/// Emit the `register_{module}` function for a single trait.
fn build_module(api: &ApiDefinition, trait_def: &TraitDef) -> Result<ModuleEmission> {
    let module = module_for_trait(&trait_def.name);

    let mut methods = Vec::with_capacity(trait_def.methods.len());
    for method in &trait_def.methods {
        let wire_method = wire_method_name(&trait_def.name, &method.name);
        methods.push(MethodEmission::build(api, &module, &wire_method, method)?);
    }
    let uses_raw_err_payload = methods.iter().any(MethodEmission::uses_raw_err_payload);
    let uses_raw_unit_ok_payload = methods.iter().any(MethodEmission::uses_raw_unit_ok_payload);

    let fn_name = format!("register_{module}");
    let trait_name = &trait_def.name;
    let mut code = String::new();
    writedoc!(
        code,
        r#"
        fn {fn_name}<P>(dispatcher: &mut Dispatcher, host: Arc<P>)
        where
            P: {trait_name} + Send + Sync + 'static,
        {{
        "#
    )
    .unwrap();
    let last = methods.len().saturating_sub(1);
    for (idx, method) in methods.iter().enumerate() {
        let host_expr = if idx == last { "host" } else { "host.clone()" };
        method.write(&mut code, host_expr)?;
    }
    writeln!(code, "}}").unwrap();

    Ok(ModuleEmission {
        code,
        uses_raw_err_payload,
        uses_raw_unit_ok_payload,
    })
}

struct MethodEmission {
    /// Rust method name on the host trait (used for the `host.<name>(...)` call).
    name: String,
    /// Fully-qualified wire method name (`{trait_snake}_{method}`); uppercased
    /// to the `wire_table` const this method registers against.
    wire_name: String,
    module: String,
    kind: MethodKind,
    request_payload: Option<WirePayload>,
    response_wrapper: Option<String>,
    error_payload: WirePayload,
    /// `|reason| <domain catch-all>` closure literal folding framework
    /// `CallError` variants onto the flat wire error encoding. Present iff
    /// `error_payload` is versioned.
    error_fallback: Option<String>,
    item_wrapper: Option<String>,
}

#[derive(Clone)]
enum WirePayload {
    Versioned(String),
    Raw(TypeRef),
}

impl MethodEmission {
    fn build(
        api: &ApiDefinition,
        module: &str,
        wire_method: &str,
        method: &MethodDef,
    ) -> Result<Self> {
        let versioned_wrappers = versioned_wrapper_names(api);
        let request_payload = match method.params.as_slice() {
            [] => None,
            [param] => match &param.type_ref {
                TypeRef::Named { name, args }
                    if args.is_empty() && versioned_wrappers.contains(name) =>
                {
                    Some(WirePayload::Versioned(name.clone()))
                }
                _ => Some(WirePayload::Raw(param.type_ref.clone())),
            },
            _ => bail!(
                "Method `{}`: expected at most one request parameter (got {})",
                method.name,
                method.params.len()
            ),
        };

        let error_payload = match &method.return_type {
            ReturnType::Result { err, .. } | ReturnType::ResultSubscription { err, .. } => {
                wire_payload_for_error(&method.name, err, &versioned_wrappers)?
            }
            ReturnType::Subscription(_) => WirePayload::Raw(TypeRef::Unit),
        };

        let (response_wrapper, item_wrapper) = match &method.return_type {
            // `Result<(), _>` returns produce an empty wire payload.
            // The trait method is called for its side effects and the
            // dispatcher encodes `()` (zero bytes) on success.
            ReturnType::Result {
                ok: TypeRef::Unit, ..
            } => (None, None),
            ReturnType::Result { ok, .. } => (
                Some(
                    versioned_wrapper_root(&method.name, "response", ok, &versioned_wrappers)?
                        .to_string(),
                ),
                None,
            ),
            ReturnType::Subscription(item) => (
                None,
                Some(
                    versioned_wrapper_root(
                        &method.name,
                        "subscription item",
                        item,
                        &versioned_wrappers,
                    )?
                    .to_string(),
                ),
            ),
            ReturnType::ResultSubscription { item, .. } => (
                None,
                Some(
                    versioned_wrapper_root(
                        &method.name,
                        "subscription item",
                        item,
                        &versioned_wrappers,
                    )?
                    .to_string(),
                ),
            ),
        };

        let error_fallback = match &error_payload {
            WirePayload::Versioned(wrapper) => Some(
                error_fallback_expr(api, wrapper)
                    .with_context(|| format!("Method `{}`", method.name))?,
            ),
            WirePayload::Raw(_) => None,
        };

        Ok(MethodEmission {
            name: method.name.clone(),
            wire_name: wire_method.to_string(),
            module: module.to_string(),
            kind: method.kind,
            request_payload,
            response_wrapper,
            error_fallback,
            error_payload,
            item_wrapper,
        })
    }

    fn write(&self, out: &mut String, host_expr: &str) -> Result<()> {
        match self.kind {
            MethodKind::Request => self.write_request(out, host_expr),
            MethodKind::Subscription | MethodKind::ResultSubscription => {
                self.write_subscription(out, host_expr)
            }
        }
    }

    fn uses_raw_err_payload(&self) -> bool {
        matches!(self.request_payload, Some(WirePayload::Raw(_))) || self.uses_raw_unit_ok_payload()
    }

    /// The catch-all closure literal for this method's versioned error type.
    fn fallback_expr(&self, method: &str) -> Result<&str> {
        self.error_fallback.as_deref().with_context(|| {
            format!("Method `{method}`: versioned error emission requires a domain catch-all")
        })
    }

    fn uses_raw_unit_ok_payload(&self) -> bool {
        matches!(self.kind, MethodKind::Request)
            && self.response_wrapper.is_none()
            && matches!(self.error_payload, WirePayload::Raw(_))
    }

    fn write_request(&self, out: &mut String, host_expr: &str) -> Result<()> {
        let module = &self.module;
        let method = &self.name;
        let ids = const_name(&self.wire_name);

        write_indented(
            out,
            4,
            &formatdoc! {
                r#"
                {{
                    let host = {host_expr};
                    dispatcher.on_request(wire_table::{ids}, move |request_id: String, bytes: Vec<u8>| {{
                        let host = host.clone();
                        Box::pin(async move {{
                "#
            },
        );
        let (call_args, target_version_expr) = match &self.request_payload {
            Some(WirePayload::Versioned(request)) => {
                let Some(error) = self.error_payload.versioned_name() else {
                    bail!("Method `{method}`: versioned request methods must use versioned errors");
                };
                let fallback = self.fallback_expr(method)?;
                write_indented(
                    out,
                    16,
                    &formatdoc! {
                        r#"
                        let request: versioned::{module}::{request} = match Decode::decode(&mut &bytes[..]) {{
                            Ok(request) => request,
                            Err(err) => {{
                                let error: truapi::CallError<versioned::{module}::{error}> =
                                    truapi::CallError::MalformedFrame {{ reason: err.to_string() }};
                                return Ok(encode_versioned_err_payload(
                                    error,
                                    <versioned::{module}::{error} as Versioned>::LATEST,
                                    {fallback},
                                ));
                            }}
                        }};
                        let target_version = request.version();
                        "#
                    },
                );
                (
                    "&cx, request".to_string(),
                    Some("target_version".to_string()),
                )
            }
            Some(WirePayload::Raw(request)) => {
                let request_ty = rust_type_ref(request).with_context(|| {
                    format!("Method `{method}`: raw request type cannot be emitted")
                })?;
                let error_ty = self
                    .error_payload
                    .rust_error_type(module)
                    .with_context(|| {
                        format!("Method `{method}`: raw request methods must have error type")
                    })?;
                write_indented(
                    out,
                    16,
                    &formatdoc! {
                        r#"
                        let request: {request_ty} = match Decode::decode(&mut &bytes[..]) {{
                            Ok(request) => request,
                            Err(err) => {{
                                let error: truapi::CallError<{error_ty}> =
                                    truapi::CallError::MalformedFrame {{ reason: err.to_string() }};
                                return Ok(encode_raw_err_payload(error));
                            }}
                        }};
                        "#
                    },
                );
                ("&cx, request".to_string(), None)
            }
            None => {
                writeln!(out, "                let _ = bytes;").unwrap();
                let target = self
                    .error_payload
                    .versioned_name()
                    .map(|error| format!("<versioned::{module}::{error} as Versioned>::LATEST"));
                ("&cx".to_string(), target)
            }
        };
        writeln!(
            out,
            "                let cx = CallContext::with_request_id(request_id.clone());"
        )
        .unwrap();
        match &self.response_wrapper {
            Some(response) => {
                let Some(target_version_expr) = target_version_expr.as_deref() else {
                    bail!("Method `{method}`: versioned responses require a target version");
                };
                let fallback = self.fallback_expr(method)?;
                write_indented(
                    out,
                    16,
                    &formatdoc! {
                        r#"
                        let response: versioned::{module}::{response} = match host.{method}({call_args}).await {{
                            Ok(value) => value,
                            Err(err) => {{
                                return Ok(encode_versioned_err_payload(err, {target_version_expr}, {fallback}));
                            }}
                        }};
                        Ok(encode_versioned_ok_payload(response))
                        "#
                    },
                );
            }
            None => match (&self.error_payload, target_version_expr.as_deref()) {
                (WirePayload::Versioned(_), Some(target_version_expr)) => {
                    let fallback = self.fallback_expr(method)?;
                    write_indented(
                        out,
                        16,
                        &formatdoc! {
                            r#"
                            match host.{method}({call_args}).await {{
                                Ok(()) => Ok(encode_versioned_unit_ok_payload({target_version_expr})),
                                Err(err) => {{
                                    Ok(encode_versioned_err_payload(err, {target_version_expr}, {fallback}))
                                }}
                            }}
                            "#
                        },
                    );
                }
                (WirePayload::Raw(_), _) => {
                    write_indented(
                        out,
                        16,
                        &formatdoc! {
                            r#"
                            match host.{method}({call_args}).await {{
                                Ok(()) => Ok(encode_raw_unit_ok_payload()),
                                Err(err) => Ok(encode_raw_err_payload(err)),
                            }}
                            "#
                        },
                    );
                }
                (WirePayload::Versioned(_), None) => {
                    bail!("Method `{method}`: versioned unit responses require a target version")
                }
            },
        }
        write_indented(
            out,
            4,
            indoc! {
                r#"
                        })
                    });
                }
                "#
            },
        );
        Ok(())
    }

    fn write_subscription(&self, out: &mut String, host_expr: &str) -> Result<()> {
        let module = &self.module;
        let method = &self.name;
        let ids = const_name(&self.wire_name);
        let Some(item) = self.item_wrapper.as_deref() else {
            bail!("Method `{method}`: subscription methods must have an item wrapper");
        };
        let error = self.error_payload.versioned_name();

        let is_result_sub = matches!(self.kind, MethodKind::ResultSubscription);

        write_indented(
            out,
            4,
            &formatdoc! {
                r#"
                {{
                    let host = {host_expr};
                    dispatcher.on_subscription(wire_table::{ids}, move |request_id: String, bytes: Vec<u8>| {{
                        let host = host.clone();
                        Box::pin(async move {{
                "#
            },
        );
        let (call_args, target_version_expr) = if let Some(WirePayload::Versioned(request)) =
            &self.request_payload
        {
            let decode_error = match error {
                Some(error) => {
                    let fallback = self.fallback_expr(method)?;
                    let block = formatdoc! {
                        r#"
                        Err(err) => {{
                            let error: truapi::CallError<versioned::{module}::{error}> =
                                truapi::CallError::MalformedFrame {{
                                    reason: err.to_string(),
                                }};
                            return Err(encode_versioned_interrupt_payload(
                                error,
                                <versioned::{module}::{error} as Versioned>::LATEST,
                                {fallback},
                            ));
                        }}
                        "#
                    };
                    block
                        .lines()
                        .map(|line| format!("    {line}"))
                        .collect::<Vec<_>>()
                        .join("\n")
                }
                None => "    Err(_) => return Err(Vec::new()),".to_string(),
            };
            write_indented(
                out,
                16,
                &formatdoc! {
                    r#"
                    let request: versioned::{module}::{request} = match Decode::decode(&mut &bytes[..]) {{
                        Ok(request) => request,
                    {decode_error}
                    }};
                    "#
                },
            );
            if is_result_sub {
                writeln!(
                    out,
                    "                let target_version = request.version();"
                )
                .unwrap();
            }
            ("&cx, request".to_string(), "target_version".to_string())
        } else {
            writeln!(out, "                let _ = bytes;").unwrap();
            let target_version = error
                .map(|error| format!("<versioned::{module}::{error} as Versioned>::LATEST"))
                .unwrap_or_else(|| "1".to_string());
            ("&cx".to_string(), target_version)
        };
        writeln!(
            out,
            "                let cx = CallContext::with_request_id(request_id.clone());"
        )
        .unwrap();
        if is_result_sub {
            if error.is_none() {
                bail!("Method `{method}`: result subscription methods must have an error wrapper");
            }
            let fallback = self.fallback_expr(method)?;
            write_indented(
                out,
                16,
                &formatdoc! {
                    r#"
                    let stream = match host.{method}({call_args}).await {{
                        Ok(sub) => sub,
                        Err(err) => {{
                            return Err(encode_versioned_interrupt_payload(err, {target_version_expr}, {fallback}));
                        }}
                    }};
                    "#
                },
            );
        } else {
            writeln!(
                out,
                "                let stream = host.{method}({call_args}).await;"
            )
            .unwrap();
        }
        writeln!(
            out,
            "                Ok(subscription_stream::<versioned::{module}::{item}, _>(stream))"
        )
        .unwrap();
        write_indented(
            out,
            4,
            indoc! {
                r#"
                        })
                    });
                }
                "#
            },
        );
        Ok(())
    }
}

impl WirePayload {
    fn versioned_name(&self) -> Option<&str> {
        match self {
            Self::Versioned(name) => Some(name),
            Self::Raw(_) => None,
        }
    }

    fn rust_error_type(&self, module: &str) -> Result<String> {
        match self {
            Self::Versioned(name) => Ok(format!("versioned::{module}::{name}")),
            Self::Raw(ty) => rust_type_ref(ty),
        }
    }
}

fn wire_payload_for_error(
    method: &str,
    ty: &TypeRef,
    versioned_wrappers: &BTreeSet<String>,
) -> Result<WirePayload> {
    let inner = call_error_inner(ty).unwrap_or(ty);
    match inner {
        TypeRef::Named { name, args } if args.is_empty() && versioned_wrappers.contains(name) => {
            Ok(WirePayload::Versioned(name.clone()))
        }
        _ => {
            if matches!(inner, TypeRef::Unit) {
                bail!("Method `{method}`: error type cannot be unit")
            }
            Ok(WirePayload::Raw(inner.clone()))
        }
    }
}

fn versioned_wrapper_root<'a>(
    method: &str,
    role: &str,
    ty: &'a TypeRef,
    versioned_wrappers: &BTreeSet<String>,
) -> Result<&'a str> {
    let TypeRef::Named { name, args } = ty else {
        bail!("Method `{method}`: {role} is not a versioned wrapper")
    };
    if !args.is_empty() || !versioned_wrappers.contains(name) {
        bail!("Method `{method}`: {role} is not a versioned wrapper")
    }
    Ok(name)
}

/// Build the `|reason| <catch-all>` closure literal for a versioned error
/// wrapper. The closure constructs the latest domain enum's catch-all variant
/// (`Unknown`, or `Internal` where no `Unknown` exists) so framework
/// `CallError` variants can fold onto the flat wire error encoding.
fn error_fallback_expr(api: &ApiDefinition, wrapper_name: &str) -> Result<String> {
    let is_version_variant = |v: &VariantDef| {
        v.name
            .strip_prefix('V')
            .is_some_and(|n| n.parse::<u32>().is_ok())
    };
    let wrapper_variants = api
        .types
        .iter()
        .find_map(|ty| match &ty.kind {
            TypeDefKind::Enum(variants)
                if ty.name == wrapper_name && variants.iter().all(is_version_variant) =>
            {
                Some(variants)
            }
            _ => None,
        })
        .with_context(|| format!("versioned error wrapper `{wrapper_name}` not extracted"))?;
    let latest = wrapper_variants
        .iter()
        .max_by_key(|v| v.name[1..].parse::<u32>().unwrap_or(0))
        .with_context(|| format!("versioned error wrapper `{wrapper_name}` has no variants"))?;
    let VariantFields::Unnamed(inner) = &latest.fields else {
        bail!(
            "versioned error wrapper `{wrapper_name}`: latest variant `{}` must carry \
             exactly one domain payload",
            latest.name
        );
    };
    let [inner_ty] = inner.as_slice() else {
        bail!(
            "versioned error wrapper `{wrapper_name}`: latest variant `{}` must carry \
             exactly one domain payload",
            latest.name
        );
    };
    let inner_path = rust_type_ref(inner_ty)?;
    let TypeRef::Named { name, .. } = inner_ty else {
        bail!("versioned error wrapper `{wrapper_name}`: domain payload must be a named enum");
    };
    let bare_name = version_prefixed_type(name).map_or(name.as_str(), |(_, base)| base);
    // Wrappers over the `GenericError` struct carry the reason directly.
    if bare_name == "GenericError" {
        return Ok(format!("|reason| {inner_path} {{ reason }}"));
    }
    let domain_variants = api
        .types
        .iter()
        .find_map(|ty| match &ty.kind {
            TypeDefKind::Enum(variants)
                if ty.name == *name && !variants.iter().all(is_version_variant) =>
            {
                Some(variants)
            }
            _ => None,
        })
        .with_context(|| format!("domain error enum `{name}` not extracted"))?;
    let catch_all = domain_variants
        .iter()
        .find(|v| v.name == "Unknown")
        .or_else(|| domain_variants.iter().find(|v| v.name == "Internal"))
        .with_context(|| {
            format!(
                "domain error enum `{bare_name}` has no `Unknown`/`Internal` catch-all; \
                 the flat wire error encoding requires one"
            )
        })?;
    let variant = &catch_all.name;
    match &catch_all.fields {
        VariantFields::Unit => Ok(format!("|_reason| {inner_path}::{variant}")),
        VariantFields::Named(fields)
            if fields.len() == 1
                && fields[0].name == "reason"
                && matches!(&fields[0].type_ref, TypeRef::Primitive(p) if p == "str") =>
        {
            Ok(format!("|reason| {inner_path}::{variant} {{ reason }}"))
        }
        VariantFields::Unnamed(types)
            if matches!(
                types.as_slice(),
                [TypeRef::Named { name, args }] if name == "GenericError" && args.is_empty()
            ) =>
        {
            Ok(format!(
                "|reason| {inner_path}::{variant}(truapi::v01::GenericError {{ reason }})"
            ))
        }
        _ => bail!(
            "domain error enum `{bare_name}`: catch-all variant `{variant}` has an \
             unsupported payload shape for reason folding"
        ),
    }
}

fn versioned_wrapper_names(api: &ApiDefinition) -> BTreeSet<String> {
    api.types
        .iter()
        .filter_map(|ty| {
            let TypeDefKind::Enum(variants) = &ty.kind else {
                return None;
            };
            if variants.iter().all(|variant| {
                variant
                    .name
                    .strip_prefix('V')
                    .is_some_and(|version| version.parse::<u32>().is_ok())
            }) {
                Some(ty.name.clone())
            } else {
                None
            }
        })
        .collect()
}

fn rust_type_ref(ty: &TypeRef) -> Result<String> {
    match ty {
        TypeRef::Primitive(name) => Ok(match name.as_str() {
            "str" => "String".to_string(),
            "compact" => "u128".to_string(),
            "optionBool" => "parity_scale_codec::OptionBool".to_string(),
            other => other.to_string(),
        }),
        TypeRef::Named { name, args } if name == "CallError" && args.len() == 1 => {
            Ok(format!("truapi::CallError<{}>", rust_type_ref(&args[0])?))
        }
        TypeRef::Named { name, args } if args.is_empty() => {
            if let Some((version, base)) = version_prefixed_type(name) {
                Ok(format!("truapi::v{version:02}::{base}"))
            } else {
                Ok(format!("truapi::v01::{name}"))
            }
        }
        TypeRef::Named { name, args } => {
            let args = args
                .iter()
                .map(rust_type_ref)
                .collect::<Result<Vec<_>>>()?
                .join(", ");
            Ok(format!("truapi::v01::{name}<{args}>"))
        }
        TypeRef::Vec(inner) => Ok(format!("Vec<{}>", rust_type_ref(inner)?)),
        TypeRef::Option(inner) => Ok(format!("Option<{}>", rust_type_ref(inner)?)),
        TypeRef::Tuple(items) => {
            let items = items
                .iter()
                .map(rust_type_ref)
                .collect::<Result<Vec<_>>>()?
                .join(", ");
            Ok(format!("({items})"))
        }
        TypeRef::Array(inner, len) => Ok(format!("[{}; {len}]", rust_type_ref(inner)?)),
        TypeRef::Generic(name) => Ok(name.clone()),
        TypeRef::Unit => Ok("()".to_string()),
    }
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

fn call_error_inner(ty: &TypeRef) -> Option<&TypeRef> {
    match ty {
        TypeRef::Named { name, args } if name == "CallError" && args.len() == 1 => Some(&args[0]),
        _ => None,
    }
}

/// Append `block` to `out`, prefixing every non-empty line with `indent` spaces.
fn write_indented(out: &mut String, indent: usize, block: &str) {
    let pad = " ".repeat(indent);
    for line in block.lines() {
        if line.is_empty() {
            out.push('\n');
        } else {
            writeln!(out, "{pad}{line}").unwrap();
        }
    }
}

fn write_header(out: &mut String) {
    writedoc!(
        out,
        r#"
        //! Wire dispatcher for the unified `TrUApi` trait.
        //!
        //! Auto-generated by truapi-codegen. Do not edit.

        "#
    )
    .unwrap();
}

fn write_imports(
    out: &mut String,
    traits: &[&TraitDef],
    uses_raw_err_payload: bool,
    uses_raw_unit_ok_payload: bool,
) {
    writedoc!(
        out,
        r#"
        use std::sync::Arc;

        use parity_scale_codec::Decode;

        use truapi::CallContext;
        use truapi::api::{{
        "#
    )
    .unwrap();
    for trait_def in traits {
        writeln!(out, "    {},", trait_def.name).unwrap();
    }
    writedoc!(
        out,
        r#"
        }};
        use truapi::versioned::{{self, Versioned}};

        use crate::dispatcher::Dispatcher;
        use crate::frame::encode_versioned_err_payload;
        use crate::frame::encode_versioned_interrupt_payload;
        use crate::frame::encode_versioned_ok_payload;
        use crate::frame::encode_versioned_unit_ok_payload;
        use crate::generated::wire_table;
        use crate::subscription::subscription_stream;
        "#
    )
    .unwrap();
    if uses_raw_err_payload {
        writeln!(out, "use crate::frame::encode_raw_err_payload;").unwrap();
    }
    if uses_raw_unit_ok_payload {
        writeln!(out, "use crate::frame::encode_raw_unit_ok_payload;").unwrap();
    }
}

fn write_top_register(out: &mut String, traits: &[&TraitDef]) {
    writedoc!(
        out,
        r#"
        /// Register every TrUAPI method with the dispatcher.
        pub fn register<P>(dispatcher: &mut Dispatcher, host: Arc<P>)
        where
            P: truapi::api::TrUApi + 'static,
        {{
        "#
    )
    .unwrap();
    let last = traits.len().saturating_sub(1);
    for (idx, trait_def) in traits.iter().enumerate() {
        let host_expr = if idx == last { "host" } else { "host.clone()" };
        let module = module_for_trait(&trait_def.name);
        writeln!(out, "    register_{module}(dispatcher, {host_expr});").unwrap();
    }
    writeln!(out, "}}").unwrap();
}
