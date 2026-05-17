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
//! and references `crate::dispatcher::Dispatcher`. The Phase 1 codegen
//! does not attempt to compile the output; only string-diff golden
//! tests guard it.

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::fmt::Write;

use anyhow::{Result, bail};

use crate::rustdoc::*;

use super::{module_for_trait, wire_method_name};

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
    for trait_def in &traits {
        modules.push(ModuleEmission::build(trait_def)?);
    }

    let mut out = String::new();
    write_header(&mut out);
    write_imports(&mut out, &traits);
    writeln!(out).unwrap();
    write_top_register(&mut out, &traits);

    for module in &modules {
        writeln!(out).unwrap();
        out.push_str(&module.code);
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
}

impl ModuleEmission {
    fn build(trait_def: &TraitDef) -> Result<Self> {
        let module = module_for_trait(&trait_def.name);

        let mut methods = Vec::with_capacity(trait_def.methods.len());
        for method in &trait_def.methods {
            let wire_method = wire_method_name(&trait_def.name, &method.name);
            methods.push(MethodEmission::build(&module, &wire_method, method)?);
        }

        let fn_name = format!("register_{module}");
        let mut code = String::new();
        writeln!(
            code,
            "fn {fn_name}<P>(dispatcher: &mut Dispatcher, host: Arc<P>)"
        )
        .unwrap();
        writeln!(code, "where").unwrap();
        writeln!(code, "    P: {} + Send + Sync + 'static,", trait_def.name).unwrap();
        writeln!(code, "{{").unwrap();
        let last = methods.len().saturating_sub(1);
        for (idx, method) in methods.iter().enumerate() {
            let host_expr = if idx == last { "host" } else { "host.clone()" };
            method.write(&mut code, host_expr);
        }
        writeln!(code, "}}").unwrap();

        Ok(ModuleEmission { code })
    }
}

struct MethodEmission {
    /// Rust method name on the host trait (used for the `host.<name>(...)` call).
    name: String,
    /// Fully-qualified wire method name (`{trait_snake}_{method}`); used as the
    /// dispatcher registration key and the tag prefix.
    wire_name: String,
    module: String,
    kind: MethodKind,
    request_wrapper: Option<String>,
    response_wrapper: Option<String>,
    item_wrapper: Option<String>,
}

impl MethodEmission {
    fn build(module: &str, wire_method: &str, method: &MethodDef) -> Result<Self> {
        let request_wrapper = match method.params.as_slice() {
            [] => None,
            [param] => match &param.type_ref {
                TypeRef::Named { name, args } if args.is_empty() => Some(name.clone()),
                _ => bail!(
                    "Method `{}`: expected a single versioned-wrapper request parameter",
                    method.name
                ),
            },
            _ => bail!(
                "Method `{}`: expected at most one request parameter (got {})",
                method.name,
                method.params.len()
            ),
        };

        let (response_wrapper, item_wrapper) = match &method.return_type {
            // `Result<(), _>` returns produce an empty wire payload.
            // The trait method is called for its side effects and the
            // dispatcher encodes `()` (zero bytes) on success.
            ReturnType::Result {
                ok: TypeRef::Unit, ..
            } => (None, None),
            ReturnType::Result { ok, .. } => (
                Some(named_root(ok).ok_or_else(|| {
                    anyhow::anyhow!(
                        "Method `{}`: response is not a versioned wrapper",
                        method.name
                    )
                })?),
                None,
            ),
            ReturnType::Subscription(item) => (
                None,
                Some(named_root(item).ok_or_else(|| {
                    anyhow::anyhow!(
                        "Method `{}`: subscription item is not a versioned wrapper",
                        method.name
                    )
                })?),
            ),
            ReturnType::ResultSubscription { item, .. } => (
                None,
                Some(named_root(item).ok_or_else(|| {
                    anyhow::anyhow!(
                        "Method `{}`: subscription item is not a versioned wrapper",
                        method.name
                    )
                })?),
            ),
        };

        Ok(MethodEmission {
            name: method.name.clone(),
            wire_name: wire_method.to_string(),
            module: module.to_string(),
            kind: match method.kind {
                MethodKind::Request => MethodKind::Request,
                MethodKind::Subscription => MethodKind::Subscription,
                MethodKind::ResultSubscription => MethodKind::ResultSubscription,
            },
            request_wrapper,
            response_wrapper,
            item_wrapper,
        })
    }

    fn write(&self, out: &mut String, host_expr: &str) {
        match self.kind {
            MethodKind::Request => self.write_request(out, host_expr),
            MethodKind::Subscription | MethodKind::ResultSubscription => {
                self.write_subscription(out, host_expr)
            }
        }
    }

    fn write_request(&self, out: &mut String, host_expr: &str) {
        let module = &self.module;
        let method = &self.name;
        let wire = &self.wire_name;

        writeln!(out, "    {{").unwrap();
        writeln!(out, "        let host = {host_expr};").unwrap();
        writeln!(
            out,
            "        dispatcher.on_request(\"{wire}\", move |request_id: String, bytes: Vec<u8>| {{"
        )
        .unwrap();
        writeln!(out, "            let host = host.clone();").unwrap();
        writeln!(out, "            Box::pin(async move {{").unwrap();
        let call_args = if let Some(request) = &self.request_wrapper {
            writeln!(
                out,
                "                let request: versioned::{module}::{request} ="
            )
            .unwrap();
            writeln!(
                out,
                "                    Decode::decode(&mut &bytes[..]).map_err(|e| ProtocolMessage::decode_error(e.to_string()))?;"
            )
            .unwrap();
            "&cx, request"
        } else {
            writeln!(out, "                let _ = bytes;").unwrap();
            "&cx"
        };
        writeln!(
            out,
            "                let cx = CallContext::with_request_id(request_id.clone());"
        )
        .unwrap();
        match &self.response_wrapper {
            Some(response) => {
                writeln!(
                    out,
                    "                let response: versioned::{module}::{response} = match host.{method}({call_args}).await {{",
                )
                .unwrap();
                writeln!(out, "                    Ok(value) => value,").unwrap();
                writeln!(
                    out,
                    "                    Err(err) => return Err(ProtocolMessage::call_error(err)),"
                )
                .unwrap();
                writeln!(out, "                }};").unwrap();
                writeln!(
                    out,
                    "                let mut buf = Vec::with_capacity(1 + response.size_hint());"
                )
                .unwrap();
                writeln!(out, "                buf.push(0u8);").unwrap();
                writeln!(out, "                response.encode_to(&mut buf);").unwrap();
                writeln!(out, "                Ok(buf)").unwrap();
            }
            None => {
                writeln!(
                    out,
                    "                match host.{method}({call_args}).await {{"
                )
                .unwrap();
                writeln!(out, "                    Ok(()) => Ok(vec![0u8]),").unwrap();
                writeln!(
                    out,
                    "                    Err(err) => Err(ProtocolMessage::call_error(err)),"
                )
                .unwrap();
                writeln!(out, "                }}").unwrap();
            }
        }
        writeln!(out, "            }})").unwrap();
        writeln!(out, "        }});").unwrap();
        writeln!(out, "    }}").unwrap();
    }

    fn write_subscription(&self, out: &mut String, host_expr: &str) {
        let module = &self.module;
        let method = &self.name;
        let wire = &self.wire_name;
        let item = self
            .item_wrapper
            .as_deref()
            .expect("subscription methods must have an item wrapper");

        let is_result_sub = matches!(self.kind, MethodKind::ResultSubscription);

        writeln!(out, "    {{").unwrap();
        writeln!(out, "        let host = {host_expr};").unwrap();
        writeln!(
            out,
            "        dispatcher.on_subscription(\"{wire}\", move |request_id: String, bytes: Vec<u8>| {{"
        )
        .unwrap();
        writeln!(out, "            let host = host.clone();").unwrap();
        writeln!(out, "            Box::pin(async move {{").unwrap();
        if let Some(request) = &self.request_wrapper {
            writeln!(
                out,
                "                let request: versioned::{module}::{request} ="
            )
            .unwrap();
            writeln!(
                out,
                "                    Decode::decode(&mut &bytes[..]).map_err(|e| ProtocolMessage::decode_error(e.to_string()))?;"
            )
            .unwrap();
            writeln!(
                out,
                "                let cx = CallContext::with_request_id(request_id.clone());"
            )
            .unwrap();
            if is_result_sub {
                writeln!(
                    out,
                    "                let stream = match host.{method}(&cx, request).await {{"
                )
                .unwrap();
                writeln!(out, "                    Ok(sub) => sub,").unwrap();
                writeln!(
                    out,
                    "                    Err(err) => return Err(ProtocolMessage::call_error(err)),"
                )
                .unwrap();
                writeln!(out, "                }};").unwrap();
            } else {
                writeln!(
                    out,
                    "                let stream = host.{method}(&cx, request).await;"
                )
                .unwrap();
            }
        } else {
            writeln!(out, "                let _ = bytes;").unwrap();
            writeln!(
                out,
                "                let cx = CallContext::with_request_id(request_id.clone());"
            )
            .unwrap();
            if is_result_sub {
                writeln!(
                    out,
                    "                let stream = match host.{method}(&cx).await {{"
                )
                .unwrap();
                writeln!(out, "                    Ok(sub) => sub,").unwrap();
                writeln!(
                    out,
                    "                    Err(err) => return Err(ProtocolMessage::call_error(err)),"
                )
                .unwrap();
                writeln!(out, "                }};").unwrap();
            } else {
                writeln!(
                    out,
                    "                let stream = host.{method}(&cx).await;"
                )
                .unwrap();
            }
        }
        writeln!(
            out,
            "                Ok(subscription_stream::<versioned::{module}::{item}, _>(stream))"
        )
        .unwrap();
        writeln!(out, "            }})").unwrap();
        writeln!(out, "        }});").unwrap();
        writeln!(out, "    }}").unwrap();
    }
}

fn named_root(ty: &TypeRef) -> Option<String> {
    if let TypeRef::Named { name, args } = ty
        && args.is_empty()
    {
        return Some(name.clone());
    }
    None
}

fn write_header(out: &mut String) {
    writeln!(out, "//! Wire dispatcher for the unified `TrUApi` trait.").unwrap();
    writeln!(out, "//!").unwrap();
    writeln!(out, "//! Auto-generated by truapi-codegen. Do not edit.").unwrap();
    writeln!(out).unwrap();
}

fn write_imports(out: &mut String, traits: &[&TraitDef]) {
    writeln!(out, "use std::sync::Arc;").unwrap();
    writeln!(out).unwrap();
    writeln!(out, "use parity_scale_codec::{{Decode, Encode}};").unwrap();
    writeln!(out).unwrap();
    writeln!(out, "use truapi::CallContext;").unwrap();
    writeln!(out, "use truapi::api::{{").unwrap();
    for trait_def in traits {
        writeln!(out, "    {},", trait_def.name).unwrap();
    }
    writeln!(out, "}};").unwrap();
    writeln!(out, "use truapi::versioned;").unwrap();
    writeln!(out).unwrap();
    writeln!(out, "use crate::dispatcher::Dispatcher;").unwrap();
    writeln!(out, "use crate::frame::ProtocolMessage;").unwrap();
    writeln!(out, "use crate::subscription::subscription_stream;").unwrap();
}

fn write_top_register(out: &mut String, traits: &[&TraitDef]) {
    writeln!(out, "/// Register every TrUAPI method with the dispatcher.").unwrap();
    writeln!(
        out,
        "pub fn register<P>(dispatcher: &mut Dispatcher, host: Arc<P>)"
    )
    .unwrap();
    writeln!(out, "where").unwrap();
    let trait_names: Vec<&str> = traits.iter().map(|t| t.name.as_str()).collect();
    let bounds = trait_names.join(" + ");
    writeln!(out, "    P: {bounds} + Send + Sync + 'static,").unwrap();
    writeln!(out, "{{").unwrap();
    let last = traits.len().saturating_sub(1);
    for (idx, trait_def) in traits.iter().enumerate() {
        let host_expr = if idx == last { "host" } else { "host.clone()" };
        let module = module_for_trait(&trait_def.name);
        writeln!(out, "    register_{module}(dispatcher, {host_expr});").unwrap();
    }
    writeln!(out, "}}").unwrap();
}
