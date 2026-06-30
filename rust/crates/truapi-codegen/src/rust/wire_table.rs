//! Emits `wire_table.rs`: the (id, tag) lookup table the server uses to
//! pair incoming wire frames with their request, response, or
//! subscription role.
//!
//! Per-method `#[wire(...)]` annotations decide id assignment:
//! - request methods reserve `(request_id, response_id)`.
//! - subscription methods reserve `(start_id, stop_id, interrupt_id, receive_id)`.
//!
//! Missing annotations and collisions both hard-fail codegen.

use std::collections::BTreeMap;
use std::fmt::Write;

use anyhow::{Result, bail};
use indoc::{formatdoc, writedoc};

use crate::rustdoc::*;

use super::{const_name, wire_method_name};

#[derive(Debug, Clone, Copy)]
struct WireEntry {
    request_id: u8,
    response_id: u8,
}

#[derive(Debug, Clone, Copy)]
struct SubEntry {
    start_id: u8,
    stop_id: u8,
    interrupt_id: u8,
    receive_id: u8,
}

#[derive(Debug, Clone, Copy)]
enum MethodEntry {
    Request(WireEntry),
    Subscription(SubEntry),
}

/// Emit the contents of `wire_table.rs`.
pub fn generate_wire_table(api: &ApiDefinition) -> Result<String> {
    let mut method_entries: Vec<(String, MethodEntry, bool)> = Vec::new();
    let mut seen: BTreeMap<u8, String> = BTreeMap::new();
    let mut seen_methods: BTreeMap<String, String> = BTreeMap::new();

    for trait_def in &api.traits {
        for method in &trait_def.methods {
            let entry = method_entry(trait_def, method)?;
            let wire_method = wire_method_name(&trait_def.name, &method.name);
            if let Some(existing) = seen_methods.insert(
                wire_method.clone(),
                format!("{}::{}", trait_def.name, method.name),
            ) {
                bail!(
                    "wire method name `{wire_method}` reused: `{existing}` and `{}::{}` collide",
                    trait_def.name,
                    method.name
                );
            }
            insert_entry(&mut seen, &wire_method, entry)?;
            method_entries.push((wire_method, entry, trait_def.name == "Testing"));
        }
    }

    method_entries.sort_by_key(|(_, entry, _)| match entry {
        MethodEntry::Request(WireEntry { request_id, .. }) => *request_id,
        MethodEntry::Subscription(SubEntry { start_id, .. }) => *start_id,
    });

    render(&method_entries)
}

fn method_entry(trait_def: &TraitDef, method: &MethodDef) -> Result<MethodEntry> {
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
            let response_id = infer_id(wire.response_id, request_id, 1, &method.name)?;
            Ok(MethodEntry::Request(WireEntry {
                request_id,
                response_id,
            }))
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
            let stop_id = infer_id(wire.stop_id, start_id, 1, &method.name)?;
            let interrupt_id = infer_id(wire.interrupt_id, start_id, 2, &method.name)?;
            let receive_id = infer_id(wire.receive_id, start_id, 3, &method.name)?;
            Ok(MethodEntry::Subscription(SubEntry {
                start_id,
                stop_id,
                interrupt_id,
                receive_id,
            }))
        }
    }
}

fn infer_id(explicit: Option<u8>, anchor: u8, offset: u8, method_name: &str) -> Result<u8> {
    if let Some(id) = explicit {
        return Ok(id);
    }
    anchor
        .checked_add(offset)
        .ok_or_else(|| anyhow::anyhow!("wire id overflow on `{method_name}` (base {anchor})"))
}

fn insert_entry(
    seen: &mut BTreeMap<u8, String>,
    method_name: &str,
    entry: MethodEntry,
) -> Result<()> {
    let pairs: Vec<(u8, String)> = match entry {
        MethodEntry::Request(WireEntry {
            request_id,
            response_id,
        }) => vec![
            (request_id, format!("{method_name}_request")),
            (response_id, format!("{method_name}_response")),
        ],
        MethodEntry::Subscription(SubEntry {
            start_id,
            stop_id,
            interrupt_id,
            receive_id,
        }) => vec![
            (start_id, format!("{method_name}_start")),
            (stop_id, format!("{method_name}_stop")),
            (interrupt_id, format!("{method_name}_interrupt")),
            (receive_id, format!("{method_name}_receive")),
        ],
    };
    for (id, tag) in pairs {
        if let Some(existing) = seen.insert(id, tag.clone()) {
            bail!("wire id {id} reused: `{existing}` and `{tag}` collide");
        }
    }
    Ok(())
}

fn render(methods: &[(String, MethodEntry, bool)]) -> Result<String> {
    let mut out = String::new();
    writedoc!(
        out,
        r#"
        //! Wire-protocol discriminant table.
        //!
        //! Auto-generated by truapi-codegen. Do not edit.
        //!
        //! Each method reserves either two ids (request/response) or four
        //! (start/stop/interrupt/receive). The ids for each method are exposed
        //! as a named const (`PREIMAGE_SUBMIT`, ...); [`WIRE_TABLE`] and the
        //! generated dispatcher both reference those consts so the numbers live
        //! in exactly one place. The table is sorted by request/start id.

        /// Request method wire discriminants.
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub struct RequestFrameIds {{
            /// Discriminant for the request frame.
            pub request_id: u8,
            /// Discriminant for the response frame.
            pub response_id: u8,
        }}

        /// Subscription method wire discriminants.
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub struct SubscriptionFrameIds {{
            /// Discriminant for the start frame.
            pub start_id: u8,
            /// Discriminant for the stop frame.
            pub stop_id: u8,
            /// Discriminant for the interrupt frame (server-initiated termination).
            pub interrupt_id: u8,
            /// Discriminant for each receive frame (a streamed item).
            pub receive_id: u8,
        }}

        /// A single wire-table row.
        pub struct WireEntry {{
            /// Method name from the Rust trait.
            pub method: &'static str,
            /// What kind of slot this entry describes.
            pub kind: WireKind,
        }}

        /// Wire-slot shape: request/response pair or subscription quartet.
        pub enum WireKind {{
            /// Request/response method.
            Request(RequestFrameIds),
            /// Subscription method.
            Subscription(SubscriptionFrameIds),
        }}
        "#
    )
    .unwrap();

    // Per-method consts: the single source of truth for each method's ids.
    for (name, entry, debug_only) in methods {
        let konst = const_name(name);
        if *debug_only {
            out.push('\n');
            out.push_str("#[cfg(debug_assertions)]");
        }
        let block = match entry {
            MethodEntry::Request(WireEntry {
                request_id,
                response_id,
            }) => formatdoc! {
                r#"
                /// Wire discriminants for `{name}`.
                pub const {konst}: RequestFrameIds = RequestFrameIds {{
                    request_id: {request_id},
                    response_id: {response_id},
                }};
                "#
            },
            MethodEntry::Subscription(SubEntry {
                start_id,
                stop_id,
                interrupt_id,
                receive_id,
            }) => formatdoc! {
                r#"
                /// Wire discriminants for `{name}`.
                pub const {konst}: SubscriptionFrameIds = SubscriptionFrameIds {{
                    start_id: {start_id},
                    stop_id: {stop_id},
                    interrupt_id: {interrupt_id},
                    receive_id: {receive_id},
                }};
                "#
            },
        };
        out.push('\n');
        out.push_str(&block);
    }

    out.push('\n');
    writedoc!(
        out,
        r#"
        /// The full wire table. Ordering is part of the wire protocol;
        /// only ever append. Removed methods leave their slot empty.
        pub const WIRE_TABLE: &[WireEntry] = &[
        "#
    )
    .unwrap();
    for (name, entry, debug_only) in methods {
        let konst = const_name(name);
        let variant = match entry {
            MethodEntry::Request(_) => "Request",
            MethodEntry::Subscription(_) => "Subscription",
        };
        let block = formatdoc! {
            r#"
            WireEntry {{
                method: "{name}",
                kind: WireKind::{variant}({konst}),
            }},
            "#
        };
        if *debug_only {
            writeln!(out, "    #[cfg(debug_assertions)]").unwrap();
        }
        for line in block.lines() {
            writeln!(out, "    {line}").unwrap();
        }
    }
    writeln!(out, "];").unwrap();

    Ok(out)
}
