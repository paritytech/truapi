//! Proc-macros for TrUAPI trait annotations.
//!
//! The single attribute exposed is [`wire`], which marks a trait method with
//! its wire-protocol discriminant ids. The ids appear on the wire as the u8 discriminant in the
//! `Struct { request_id: str, payload: Enum(<methods>) }` envelope; method
//! ordering becomes part of the wire protocol.
//!
//! At compile time the macro validates that every id literal is a `u8`. It emits
//! a hidden doc line so the value survives into rustdoc JSON, where
//! `truapi-codegen` reads it to build generated wire tables.
//!
//! Why doc-smuggling instead of leaving `#[wire(request_id = N)]` on the method for
//! the codegen to read directly: rustdoc's JSON `attrs` field stringifies
//! attributes, but rustc rejects unknown helper attributes on trait methods
//! unless they are declared via a tool prefix or consumed by an active
//! proc-macro. Re-emitting the marker as a `#[doc]` line lets the value reach
//! rustdoc through the only attribute that is always preserved verbatim.

use proc_macro::TokenStream;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::{Ident, ItemFn, LitInt, Token, TraitItemFn, parse_macro_input};

#[derive(Default)]
struct WireArgs {
    request_id: Option<u8>,
    response_id: Option<u8>,
    start_id: Option<u8>,
    stop_id: Option<u8>,
    interrupt_id: Option<u8>,
    receive_id: Option<u8>,
}

impl Parse for WireArgs {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let mut args = WireArgs::default();

        while !input.is_empty() {
            let key: Ident = input.parse()?;
            input.parse::<Token![=]>()?;
            let lit: LitInt = input.parse()?;
            let value = lit.base10_parse().map_err(|err| {
                syn::Error::new(lit.span(), format!("wire id must fit in a u8: {err}"))
            })?;

            set_id(&mut args, &key, value)?;

            if input.is_empty() {
                break;
            }
            input.parse::<Token![,]>()?;
        }

        if args.request_id.is_none() && args.start_id.is_none() {
            return Err(input.error("missing `request_id = N` or `start_id = N`"));
        }

        Ok(args)
    }
}

fn set_id(args: &mut WireArgs, key: &Ident, value: u8) -> syn::Result<()> {
    let target = if key == "request_id" {
        &mut args.request_id
    } else if key == "response_id" {
        &mut args.response_id
    } else if key == "start_id" {
        &mut args.start_id
    } else if key == "stop_id" {
        &mut args.stop_id
    } else if key == "interrupt_id" {
        &mut args.interrupt_id
    } else if key == "receive_id" {
        &mut args.receive_id
    } else {
        return Err(syn::Error::new(
            key.span(),
            "expected one of `request_id`, `response_id`, `start_id`, `stop_id`, `interrupt_id`, `receive_id`",
        ));
    };

    if target.replace(value).is_some() {
        return Err(syn::Error::new(key.span(), format!("duplicate `{key}`")));
    }

    Ok(())
}

/// Mark a TrUAPI trait method with its wire-protocol discriminant id.
///
/// ```ignore
/// #[wire(request_id = 4)]
/// async fn host_account_get(...) -> ...;
///
/// #[wire(start_id = 42)]
/// async fn host_account_connection_status_subscribe(...) -> ...;
/// ```
///
/// Expands to the original method plus hidden doc tags that `truapi-codegen`
/// extracts from rustdoc JSON to build the wire table and versioned clients.
#[proc_macro_attribute]
pub fn wire(args: TokenStream, item: TokenStream) -> TokenStream {
    let args = parse_macro_input!(args as WireArgs);
    let tags = wire_tags(&args);

    if let Ok(mut method) = syn::parse::<TraitItemFn>(item.clone()) {
        for tag in tags {
            method.attrs.push(syn::parse_quote!(#[doc = #tag]));
        }
        return quote!(#method).into();
    }

    if let Ok(mut function) = syn::parse::<ItemFn>(item) {
        for tag in tags {
            function.attrs.push(syn::parse_quote!(#[doc = #tag]));
        }
        return quote!(#function).into();
    }

    syn::Error::new(
        proc_macro2::Span::call_site(),
        "#[wire] can only be applied to trait methods or free functions",
    )
    .to_compile_error()
    .into()
}

fn wire_tags(args: &WireArgs) -> Vec<String> {
    [
        ("request_id", args.request_id),
        ("response_id", args.response_id),
        ("start_id", args.start_id),
        ("stop_id", args.stop_id),
        ("interrupt_id", args.interrupt_id),
        ("receive_id", args.receive_id),
    ]
    .into_iter()
    .filter_map(|(name, value)| value.map(|id| format!("@wire_{name}={id}")))
    .collect()
}
