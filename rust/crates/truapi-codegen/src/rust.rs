//! Rust code generation from extracted API definitions.
//!
//! Emits the server-side wire dispatcher (`dispatcher.rs`) and the
//! discriminant lookup table (`wire_table.rs`). The generated files are
//! intended to be included in the `truapi-server` crate.

use std::fs;
use std::path::Path;

use anyhow::Result;

use crate::rustdoc::*;

mod dispatcher;
mod wire_table;

pub use dispatcher::generate_dispatcher;
pub use wire_table::generate_wire_table;

/// Generates the Rust wire dispatcher and wire-table sources into `output_dir`.
pub fn generate(api: &ApiDefinition, output_dir: &Path) -> Result<()> {
    fs::create_dir_all(output_dir)?;
    let dispatcher = generate_dispatcher(api)?;
    fs::write(output_dir.join("dispatcher.rs"), dispatcher)?;
    let wire_table = generate_wire_table(api)?;
    fs::write(output_dir.join("wire_table.rs"), wire_table)?;
    Ok(())
}

/// Trait -> versioned-module mapping. Trait names are PascalCase
/// (`JsonRpc`, `LocalStorage`); module names are snake_case
/// (`jsonrpc`, `local_storage`). The mapping is irregular enough
/// (e.g. `JsonRpc` -> `jsonrpc`) that it is hardcoded.
const TRAIT_MODULE_MAP: &[(&str, &str)] = &[
    ("Account", "account"),
    ("Chain", "chain"),
    ("Chat", "chat"),
    ("Entropy", "entropy"),
    ("JsonRpc", "jsonrpc"),
    ("LocalStorage", "local_storage"),
    ("Payment", "payment"),
    ("Permissions", "permissions"),
    ("Preimage", "preimage"),
    ("ResourceAllocation", "resource_allocation"),
    ("Signing", "signing"),
    ("StatementStore", "statement_store"),
    ("System", "system"),
    ("Theme", "theme"),
];

/// Returns the versioned-module name for a trait, falling back to a
/// snake_case conversion of the trait name when no explicit mapping is
/// declared. New traits should be added to [`TRAIT_MODULE_MAP`] so the
/// emission stays deterministic.
fn module_for_trait(trait_name: &str) -> String {
    for (name, module) in TRAIT_MODULE_MAP {
        if *name == trait_name {
            return (*module).to_string();
        }
    }
    snake_case(trait_name)
}

/// Returns the wire-protocol method name for a trait/method pair, used both
/// as the dispatcher's registration key and as the prefix of the action tag
/// (`{wire_method}_{request|response|...}`). The form is
/// `{trait_snake}_{method}` so collisions between sibling traits (e.g.
/// `StatementStore::submit` and `Preimage::submit`) become distinct keys
/// (`statement_store_submit`, `preimage_submit`).
pub(crate) fn wire_method_name(trait_name: &str, method_name: &str) -> String {
    format!("{}_{}", snake_case(trait_name), method_name)
}

/// Convert a PascalCase identifier into snake_case.
fn snake_case(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 4);
    for (idx, ch) in name.chars().enumerate() {
        if ch.is_ascii_uppercase() {
            if idx != 0 {
                out.push('_');
            }
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_request_method(name: &str, request_id: u8) -> MethodDef {
        MethodDef {
            name: name.to_string(),
            kind: MethodKind::Request,
            params: vec![ParamDef {
                name: "request".to_string(),
                type_ref: TypeRef::Named {
                    name: "ReqWrapper".to_string(),
                    args: vec![],
                },
            }],
            return_type: ReturnType::Result {
                ok: TypeRef::Named {
                    name: "RespWrapper".to_string(),
                    args: vec![],
                },
                err: TypeRef::Named {
                    name: "CallError".to_string(),
                    args: vec![TypeRef::Named {
                        name: "ErrWrapper".to_string(),
                        args: vec![],
                    }],
                },
            },
            wire: WireAttrs {
                request_id: Some(request_id),
                response_id: None,
                start_id: None,
                stop_id: None,
                interrupt_id: None,
                receive_id: None,
            },
            docs: None,
        }
    }

    fn make_subscription_method(name: &str, start_id: u8) -> MethodDef {
        MethodDef {
            name: name.to_string(),
            kind: MethodKind::Subscription,
            params: vec![],
            return_type: ReturnType::Subscription(TypeRef::Named {
                name: "ItemWrapper".to_string(),
                args: vec![],
            }),
            wire: WireAttrs {
                request_id: None,
                response_id: None,
                start_id: Some(start_id),
                stop_id: None,
                interrupt_id: None,
                receive_id: None,
            },
            docs: None,
        }
    }

    fn parse_entries(src: &str) -> Vec<(u8, String)> {
        // Lines look like `    WireEntry { method: "x", ... request_id: 7, ... },`
        // For the assertion we extract (id, tag) pairs from the embedded
        // helper comment block instead. Keep a simpler parser of
        // `// id=NN tag="..."` lines emitted by the generator.
        let mut out = Vec::new();
        for line in src.lines() {
            let trimmed = line.trim();
            if let Some(rest) = trimmed.strip_prefix("// entry id=") {
                let mut parts = rest.splitn(2, ' ');
                let id = parts.next().unwrap().parse::<u8>().unwrap();
                let tag = parts
                    .next()
                    .unwrap()
                    .trim_start_matches("tag=\"")
                    .trim_end_matches('"')
                    .to_string();
                out.push((id, tag));
            }
        }
        out
    }

    /// A single subscription method must reserve four consecutive wire
    /// ids (start/stop/interrupt/receive) even when no sibling methods
    /// exist to mask off-by-one errors.
    #[test]
    fn wire_table_subscribe_method_reserves_four_ids() {
        let api = ApiDefinition {
            traits: vec![TraitDef {
                name: "Account".to_string(),
                methods: vec![make_subscription_method("connection_status_subscribe", 18)],
                docs: None,
            }],
            public_trait_order: vec!["Account".to_string()],
            types: vec![],
        };

        let src = generate_wire_table(&api).expect("generate_wire_table");
        let entries = parse_entries(&src);
        assert_eq!(
            entries,
            vec![
                (18, "account_connection_status_subscribe_start".into()),
                (19, "account_connection_status_subscribe_stop".into()),
                (20, "account_connection_status_subscribe_interrupt".into()),
                (21, "account_connection_status_subscribe_receive".into()),
            ],
        );
    }

    /// Two traits each declaring a method named `submit` must produce two
    /// distinct, non-colliding wire method keys; the emitter prefixes by
    /// the snake_case trait name (e.g. `statement_store_submit` /
    /// `preimage_submit`).
    #[test]
    fn collision_safe_when_two_traits_share_method_name() {
        let api = ApiDefinition {
            traits: vec![
                TraitDef {
                    name: "StatementStore".to_string(),
                    methods: vec![make_request_method("submit", 62)],
                    docs: None,
                },
                TraitDef {
                    name: "Preimage".to_string(),
                    methods: vec![make_request_method("submit", 68)],
                    docs: None,
                },
            ],
            public_trait_order: vec!["StatementStore".to_string(), "Preimage".to_string()],
            types: vec![],
        };

        let dispatcher = generate_dispatcher(&api).expect("dispatcher");
        assert!(
            dispatcher.contains("\"statement_store_submit\""),
            "dispatcher missing prefixed StatementStore key:\n{dispatcher}"
        );
        assert!(
            dispatcher.contains("\"preimage_submit\""),
            "dispatcher missing prefixed Preimage key:\n{dispatcher}"
        );

        let table = generate_wire_table(&api).expect("wire_table");
        let entries = parse_entries(&table);
        assert!(
            entries
                .iter()
                .any(|(_, tag)| tag == "statement_store_submit_request"),
            "wire_table missing prefixed StatementStore tag:\n{table}"
        );
        assert!(
            entries
                .iter()
                .any(|(_, tag)| tag == "preimage_submit_request"),
            "wire_table missing prefixed Preimage tag:\n{table}"
        );
    }

    /// If a future change ever produces the same wire method key from two
    /// different (trait, method) pairs, both emitters must fail loudly
    /// rather than silently overwrite a handler.
    #[test]
    fn wire_table_rejects_method_name_collision() {
        // `Foo::bar_baz` and `FooBar::baz` both snake-case to
        // `foo_bar_baz`. The emitter must reject the pair.
        let api = ApiDefinition {
            traits: vec![
                TraitDef {
                    name: "Foo".to_string(),
                    methods: vec![make_request_method("bar_baz", 10)],
                    docs: None,
                },
                TraitDef {
                    name: "FooBar".to_string(),
                    methods: vec![make_request_method("baz", 12)],
                    docs: None,
                },
            ],
            public_trait_order: vec!["Foo".to_string(), "FooBar".to_string()],
            types: vec![],
        };
        let err = generate_wire_table(&api).expect_err("duplicate wire method name must error");
        let msg = format!("{err}");
        assert!(
            msg.contains("wire method name `foo_bar_baz` reused"),
            "unexpected error message: {msg}",
        );

        let err = generate_dispatcher(&api).expect_err("duplicate wire method name must error");
        let msg = format!("{err}");
        assert!(
            msg.contains("Wire method name `foo_bar_baz` registered twice"),
            "unexpected dispatcher error message: {msg}",
        );
    }

    /// Emission must be deterministic: running the codegen twice on the
    /// same API produces byte-identical output.
    #[test]
    fn idempotent_emission() {
        let api = ApiDefinition {
            traits: vec![TraitDef {
                name: "Permissions".to_string(),
                methods: vec![make_request_method("request_device_permission", 8)],
                docs: None,
            }],
            public_trait_order: vec!["Permissions".to_string()],
            types: vec![],
        };

        let dispatcher_a = generate_dispatcher(&api).expect("dispatcher a");
        let dispatcher_b = generate_dispatcher(&api).expect("dispatcher b");
        assert_eq!(dispatcher_a, dispatcher_b);

        let table_a = generate_wire_table(&api).expect("wire_table a");
        let table_b = generate_wire_table(&api).expect("wire_table b");
        assert_eq!(table_a, table_b);
    }

    /// Methods with a `#[wire(request_id = N)]` annotation get a 2-id
    /// slot (request/response). Methods with `#[wire(start_id = N)]`
    /// get a 4-id slot (start/stop/interrupt/receive). The emitter
    /// must enforce that, and reject collisions.
    #[test]
    fn wire_table_rejects_collisions() {
        let api = ApiDefinition {
            traits: vec![TraitDef {
                name: "Permissions".to_string(),
                methods: vec![
                    make_request_method("alpha", 10),
                    make_request_method("beta", 10),
                ],
                docs: None,
            }],
            public_trait_order: vec!["Permissions".to_string()],
            types: vec![],
        };
        let err = generate_wire_table(&api).expect_err("duplicate ids must error");
        let msg = format!("{err}");
        assert!(
            msg.contains("wire id 10 reused"),
            "unexpected error message: {msg}",
        );
    }

    #[test]
    fn module_for_trait_maps_irregular_names() {
        assert_eq!(module_for_trait("JsonRpc"), "jsonrpc");
        assert_eq!(module_for_trait("LocalStorage"), "local_storage");
        assert_eq!(
            module_for_trait("ResourceAllocation"),
            "resource_allocation"
        );
        assert_eq!(module_for_trait("Account"), "account");
    }
}
