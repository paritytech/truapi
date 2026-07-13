//! Rust code generation from extracted API definitions.
//!
//! Emits the server-side wire dispatcher (`dispatcher.rs`) and the
//! discriminant lookup table (`wire_table.rs`). The generated files are
//! intended to be included in the `truapi-server` crate.

use std::fs;
use std::path::Path;

use anyhow::Result;

use convert_case::{Case, Casing};

use crate::platform::PlatformDefinition;
use crate::rustdoc::*;

mod dispatcher;
mod wasm_bridge;
mod wire_table;

pub use dispatcher::generate_dispatcher;
pub use wasm_bridge::generate_wasm_bridge;
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

/// Generates the Rust wasm-bindgen platform bridge source into `output_dir`.
pub fn generate_wasm_bridge_file(
    definition: &PlatformDefinition,
    api: &ApiDefinition,
    output_dir: &Path,
) -> Result<()> {
    fs::create_dir_all(output_dir)?;
    fs::write(
        output_dir.join("generated_bridge.rs"),
        generate_wasm_bridge(definition, api)?,
    )?;
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

/// The `SCREAMING_SNAKE_CASE` const name holding a wire method's ids.
/// Routed through [`convert_case::Case::UpperSnake`] so it follows the same
/// casing rules as the TS wire-table emitter (`ts.rs`).
pub(crate) fn const_name(wire_method: &str) -> String {
    wire_method.to_case(Case::UpperSnake)
}

/// Const name for a trait/method pair's wire ids. Both the Rust and TS
/// wire-table emitters apply `Case::UpperSnake`, so for the real
/// (single-capital PascalCase trait, snake_case method) surface the two
/// generated const names agree.
#[cfg(test)]
pub(crate) fn wire_const_name(trait_name: &str, method_name: &str) -> String {
    const_name(&wire_method_name(trait_name, method_name))
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

    fn versioned_test_type(name: &str) -> TypeDef {
        TypeDef {
            name: name.to_string(),
            module_path: Vec::new(),
            generic_params: Vec::new(),
            kind: TypeDefKind::Enum(vec![VariantDef {
                name: "V1".to_string(),
                fields: VariantFields::Unnamed(vec![TypeRef::Named {
                    name: format!("V01{name}"),
                    args: vec![],
                }]),
                docs: None,
            }]),
            docs: None,
        }
    }

    fn versioned_request_test_types() -> Vec<TypeDef> {
        ["ReqWrapper", "RespWrapper", "ErrWrapper"]
            .into_iter()
            .map(versioned_test_type)
            .collect()
    }

    fn parse_entries(src: &str) -> Vec<(u8, String)> {
        // Each method's ids are emitted as a named const, e.g.
        //   pub const PREIMAGE_SUBMIT: RequestFrameIds = RequestFrameIds {
        //       request_id: 68,
        //       response_id: 69,
        //   };
        // Reconstruct the `(id, "{method}_{suffix}")` pairs the assertions use.
        let mut out = Vec::new();
        let mut lines = src.lines();
        while let Some(line) = lines.next() {
            let Some(rest) = line.trim().strip_prefix("pub const ") else {
                continue;
            };
            let Some(colon) = rest.find(':') else {
                continue;
            };
            let is_sub = rest.contains("SubscriptionFrameIds");
            // Skip non-id consts (e.g. `WIRE_TABLE: &[WireEntry]`).
            if !is_sub && !rest.contains("RequestFrameIds") {
                continue;
            }
            let method = rest[..colon].trim().to_ascii_lowercase();

            let mut ids: std::collections::BTreeMap<&str, u8> = std::collections::BTreeMap::new();
            for inner in lines.by_ref() {
                let t = inner.trim();
                if t.starts_with("};") {
                    break;
                }
                if let Some((field, val)) = t.split_once(':') {
                    let id = val.trim().trim_end_matches(',').parse::<u8>().unwrap();
                    ids.insert(field.trim(), id);
                }
            }

            let suffixes: &[(&str, &str)] = if is_sub {
                &[
                    ("start_id", "start"),
                    ("stop_id", "stop"),
                    ("interrupt_id", "interrupt"),
                    ("receive_id", "receive"),
                ]
            } else {
                &[("request_id", "request"), ("response_id", "response")]
            };
            for (field, suffix) in suffixes {
                out.push((ids[field], format!("{method}_{suffix}")));
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
                module_path: Vec::new(),
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
                    module_path: Vec::new(),
                    methods: vec![make_request_method("submit", 62)],
                    docs: None,
                },
                TraitDef {
                    name: "Preimage".to_string(),
                    module_path: Vec::new(),
                    methods: vec![make_request_method("submit", 68)],
                    docs: None,
                },
            ],
            public_trait_order: vec!["StatementStore".to_string(), "Preimage".to_string()],
            types: versioned_request_test_types(),
        };

        let dispatcher = generate_dispatcher(&api).expect("dispatcher");
        assert!(
            dispatcher.contains("wire_table::STATEMENT_STORE_SUBMIT"),
            "dispatcher missing prefixed StatementStore const:\n{dispatcher}"
        );
        assert!(
            dispatcher.contains("wire_table::PREIMAGE_SUBMIT"),
            "dispatcher missing prefixed Preimage const:\n{dispatcher}"
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
                    module_path: Vec::new(),
                    methods: vec![make_request_method("bar_baz", 10)],
                    docs: None,
                },
                TraitDef {
                    name: "FooBar".to_string(),
                    module_path: Vec::new(),
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
                module_path: Vec::new(),
                methods: vec![make_request_method("request_device_permission", 8)],
                docs: None,
            }],
            public_trait_order: vec!["Permissions".to_string()],
            types: versioned_request_test_types(),
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
                module_path: Vec::new(),
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

    /// Pin `wire_const_name`'s `convert_case::Case::UpperSnake` behavior:
    /// digits split off (`v2` -> `V_2`) and acronyms split (`HTTPServer`
    /// snake-cases to `h_t_t_p_server`, then upper-snakes to
    /// `H_T_T_P_SERVER`). Real traits/methods avoid both, so the committed
    /// output is unaffected; the pin guards future drift.
    #[test]
    fn wire_const_name_pins_digits_and_acronyms() {
        assert_eq!(wire_const_name("Preimage", "submit"), "PREIMAGE_SUBMIT");
        assert_eq!(wire_const_name("Signing", "sign_v2"), "SIGNING_SIGN_V_2");
        assert_eq!(
            wire_const_name("HTTPServer", "serve"),
            "H_T_T_P_SERVER_SERVE"
        );
        assert_eq!(
            wire_const_name("StatementStore", "create_proof"),
            "STATEMENT_STORE_CREATE_PROOF"
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

    /// A request-kind method must not carry subscription wire ids. The
    /// emitter rejects `start_id` / `stop_id` / `interrupt_id` / `receive_id`
    /// on a `MethodKind::Request`.
    #[test]
    fn wire_table_request_with_subscription_id_errors() {
        let mut method = make_request_method("alpha", 10);
        method.wire.start_id = Some(99);
        let api = ApiDefinition {
            traits: vec![TraitDef {
                name: "Permissions".to_string(),
                module_path: Vec::new(),
                methods: vec![method],
                docs: None,
            }],
            public_trait_order: vec!["Permissions".to_string()],
            types: vec![],
        };
        let err = generate_wire_table(&api).expect_err("request kind + start_id must error");
        let msg = format!("{err}");
        assert!(
            msg.contains("must not use subscription wire ids"),
            "unexpected error message: {msg}",
        );
    }

    /// A subscription-kind method must not carry request wire ids.
    #[test]
    fn wire_table_subscription_with_request_id_errors() {
        let mut method = make_subscription_method("connection_status_subscribe", 18);
        method.wire.request_id = Some(99);
        let api = ApiDefinition {
            traits: vec![TraitDef {
                name: "Account".to_string(),
                module_path: Vec::new(),
                methods: vec![method],
                docs: None,
            }],
            public_trait_order: vec!["Account".to_string()],
            types: vec![],
        };
        let err = generate_wire_table(&api).expect_err("subscription kind + request_id must error");
        let msg = format!("{err}");
        assert!(
            msg.contains("must not use request wire ids"),
            "unexpected error message: {msg}",
        );
    }

    /// A request-kind method missing the mandatory `request_id` annotation
    /// must fail emission, not silently default to 0.
    #[test]
    fn wire_table_missing_request_id_errors() {
        let mut method = make_request_method("alpha", 10);
        method.wire.request_id = None;
        let api = ApiDefinition {
            traits: vec![TraitDef {
                name: "Permissions".to_string(),
                module_path: Vec::new(),
                methods: vec![method],
                docs: None,
            }],
            public_trait_order: vec!["Permissions".to_string()],
            types: vec![],
        };
        let err = generate_wire_table(&api).expect_err("missing request_id annotation must error");
        let msg = format!("{err}");
        assert!(
            msg.contains("missing #[wire(request_id"),
            "unexpected error message: {msg}",
        );
    }

    /// Subscription-kind method missing `start_id` is similarly rejected.
    #[test]
    fn wire_table_missing_start_id_errors() {
        let mut method = make_subscription_method("connection_status_subscribe", 18);
        method.wire.start_id = None;
        let api = ApiDefinition {
            traits: vec![TraitDef {
                name: "Account".to_string(),
                module_path: Vec::new(),
                methods: vec![method],
                docs: None,
            }],
            public_trait_order: vec!["Account".to_string()],
            types: vec![],
        };
        let err = generate_wire_table(&api).expect_err("missing start_id annotation must error");
        let msg = format!("{err}");
        assert!(
            msg.contains("missing #[wire(start_id"),
            "unexpected error message: {msg}",
        );
    }

    /// The dispatcher expects each method to take exactly one versioned
    /// wrapper parameter (plus `&self` and `&CallContext`, which are
    /// elided from `params`). A method with two params errors out.
    #[test]
    fn dispatcher_multi_param_method_errors() {
        let mut method = make_request_method("alpha", 10);
        method.params.push(ParamDef {
            name: "extra".to_string(),
            type_ref: TypeRef::Named {
                name: "ExtraWrapper".to_string(),
                args: vec![],
            },
        });
        let api = ApiDefinition {
            traits: vec![TraitDef {
                name: "Permissions".to_string(),
                module_path: Vec::new(),
                methods: vec![method],
                docs: None,
            }],
            public_trait_order: vec!["Permissions".to_string()],
            types: vec![],
        };
        let err = generate_dispatcher(&api).expect_err("two-param method must error");
        let msg = format!("{err}");
        assert!(
            msg.contains("expected at most one request parameter"),
            "unexpected error message: {msg}",
        );
    }

    /// The response wrapper extraction expects a `TypeRef::Named` with no
    /// generic args. Anything else (primitives, tuples, generics) errors.
    #[test]
    fn dispatcher_non_named_root_response_errors() {
        let mut method = make_request_method("alpha", 10);
        method.return_type = ReturnType::Result {
            ok: TypeRef::Primitive("u32".to_string()),
            err: TypeRef::Named {
                name: "CallError".to_string(),
                args: vec![TypeRef::Named {
                    name: "ErrWrapper".to_string(),
                    args: vec![],
                }],
            },
        };
        let api = ApiDefinition {
            traits: vec![TraitDef {
                name: "Permissions".to_string(),
                module_path: Vec::new(),
                methods: vec![method],
                docs: None,
            }],
            public_trait_order: vec!["Permissions".to_string()],
            types: vec![],
        };
        let err = generate_dispatcher(&api).expect_err("primitive response must error");
        let msg = format!("{err}");
        assert!(
            msg.contains("response is not a versioned wrapper"),
            "unexpected error message: {msg}",
        );
    }

    #[test]
    fn dispatcher_versioned_request_with_raw_error_errors() {
        let mut method = make_request_method("alpha", 10);
        method.return_type = ReturnType::Result {
            ok: TypeRef::Named {
                name: "RespWrapper".to_string(),
                args: vec![],
            },
            err: TypeRef::Named {
                name: "CallError".to_string(),
                args: vec![TypeRef::Named {
                    name: "RawError".to_string(),
                    args: vec![],
                }],
            },
        };
        let api = ApiDefinition {
            traits: vec![TraitDef {
                name: "Permissions".to_string(),
                module_path: Vec::new(),
                methods: vec![method],
                docs: None,
            }],
            public_trait_order: vec!["Permissions".to_string()],
            types: vec![
                versioned_test_type("ReqWrapper"),
                versioned_test_type("RespWrapper"),
            ],
        };

        let err = generate_dispatcher(&api).expect_err("raw error wrapper must error");
        let msg = format!("{err}");
        assert!(
            msg.contains("versioned request methods must use versioned errors"),
            "unexpected error message: {msg}",
        );
    }

    #[test]
    fn dispatcher_raw_request_with_versioned_response_errors() {
        let mut method = make_request_method("alpha", 10);
        method.params[0].type_ref = TypeRef::Named {
            name: "RawRequest".to_string(),
            args: vec![],
        };
        let api = ApiDefinition {
            traits: vec![TraitDef {
                name: "Permissions".to_string(),
                module_path: Vec::new(),
                methods: vec![method],
                docs: None,
            }],
            public_trait_order: vec!["Permissions".to_string()],
            types: vec![
                versioned_test_type("RespWrapper"),
                versioned_test_type("ErrWrapper"),
            ],
        };

        let err = generate_dispatcher(&api).expect_err("missing target version must error");
        let msg = format!("{err}");
        assert!(
            msg.contains("versioned responses require a target version"),
            "unexpected error message: {msg}",
        );
    }

    #[test]
    fn dispatcher_result_subscription_with_raw_error_errors() {
        let mut method = make_subscription_method("alpha_subscribe", 20);
        method.kind = MethodKind::ResultSubscription;
        method.return_type = ReturnType::ResultSubscription {
            item: TypeRef::Named {
                name: "ItemWrapper".to_string(),
                args: vec![],
            },
            err: TypeRef::Named {
                name: "CallError".to_string(),
                args: vec![TypeRef::Named {
                    name: "RawError".to_string(),
                    args: vec![],
                }],
            },
        };
        let api = ApiDefinition {
            traits: vec![TraitDef {
                name: "Account".to_string(),
                module_path: Vec::new(),
                methods: vec![method],
                docs: None,
            }],
            public_trait_order: vec!["Account".to_string()],
            types: vec![versioned_test_type("ItemWrapper")],
        };

        let err = generate_dispatcher(&api).expect_err("raw result subscription error must error");
        let msg = format!("{err}");
        assert!(
            msg.contains("result subscription methods must have an error wrapper"),
            "unexpected error message: {msg}",
        );
    }
}
