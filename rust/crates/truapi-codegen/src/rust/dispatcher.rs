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

use anyhow::{Result, bail};
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
            method.write(&mut code, host_expr);
        }
        writeln!(code, "}}").unwrap();

        Ok(ModuleEmission { code })
    }
}

struct MethodEmission {
    /// Rust method name on the host trait (used for the `host.<name>(...)` call).
    name: String,
    /// Fully-qualified wire method name (`{trait_snake}_{method}`); uppercased
    /// to the `wire_table` const this method registers against.
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
            kind: method.kind,
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
        let call_args = if let Some(request) = &self.request_wrapper {
            write_indented(
                out,
                16,
                &formatdoc! {
                    r#"
                    let request: versioned::{module}::{request} =
                        Decode::decode(&mut &bytes[..]).map_err(|e| encode_decode_error(e.to_string()))?;
                    "#
                },
            );
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
            Some(response) => write_indented(
                out,
                16,
                &formatdoc! {
                    r#"
                    let response: versioned::{module}::{response} = match host.{method}({call_args}).await {{
                        Ok(value) => value,
                        Err(err) => return Err(encode_call_error_payload(err)),
                    }};
                    let mut buf = Vec::with_capacity(1 + response.size_hint());
                    buf.push(0u8);
                    response.encode_to(&mut buf);
                    Ok(buf)
                    "#
                },
            ),
            None => write_indented(
                out,
                16,
                &formatdoc! {
                    r#"
                    match host.{method}({call_args}).await {{
                        Ok(()) => Ok(vec![0u8]),
                        Err(err) => Err(encode_call_error_payload(err)),
                    }}
                    "#
                },
            ),
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
    }

    fn write_subscription(&self, out: &mut String, host_expr: &str) {
        let module = &self.module;
        let method = &self.name;
        let ids = const_name(&self.wire_name);
        let item = self
            .item_wrapper
            .as_deref()
            .expect("subscription methods must have an item wrapper");

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
        let call_args = if let Some(request) = &self.request_wrapper {
            write_indented(
                out,
                16,
                &formatdoc! {
                    r#"
                    let request: versioned::{module}::{request} =
                        Decode::decode(&mut &bytes[..]).map_err(|e| encode_decode_error(e.to_string()))?;
                    "#
                },
            );
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
        if is_result_sub {
            write_indented(
                out,
                16,
                &formatdoc! {
                    r#"
                    let stream = match host.{method}({call_args}).await {{
                        Ok(sub) => sub,
                        Err(err) => return Err(encode_call_error_payload(err)),
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

fn write_imports(out: &mut String, traits: &[&TraitDef]) {
    writedoc!(
        out,
        r#"
        use std::sync::Arc;

        use parity_scale_codec::{{Decode, Encode}};

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
        use truapi::versioned;

        use crate::dispatcher::Dispatcher;
        use crate::frame::{{encode_call_error_payload, encode_decode_error}};
        use crate::generated::wire_table;
        use crate::subscription::subscription_stream;
        "#
    )
    .unwrap();
}

fn write_top_register(out: &mut String, traits: &[&TraitDef]) {
    let trait_names: Vec<&str> = traits.iter().map(|t| t.name.as_str()).collect();
    let bounds = trait_names.join(" + ");
    writedoc!(
        out,
        r#"
        /// Register every TrUAPI method with the dispatcher.
        pub fn register<P>(dispatcher: &mut Dispatcher, host: Arc<P>)
        where
            P: {bounds} + Send + Sync + 'static,
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
