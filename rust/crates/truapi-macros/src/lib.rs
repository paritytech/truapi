//! Proc-macros for TrUAPI trait annotations.
//!
//! The single attribute exposed is [`wire`], which marks a trait method with
//! its wire-protocol discriminant id. The id appears on the wire as the u8 discriminant in the
//! `Struct { request_id: str, payload: Enum(<methods>) }` envelope; method
//! ordering becomes part of the wire protocol.
//!
//! At compile time the macro validates that the id literal is a `u8`. It emits
//! a hidden doc line so the value survives into rustdoc JSON, where
//! `truapi-codegen` reads it to build generated wire tables.
//!
//! Why doc-smuggling instead of leaving `#[wire(id = N)]` on the method for
//! the codegen to read directly: rustdoc's JSON `attrs` field stringifies
//! attributes, but rustc rejects unknown helper attributes on trait methods
//! unless they are declared via a tool prefix or consumed by an active
//! proc-macro. Re-emitting the marker as a `#[doc]` line lets the value reach
//! rustdoc through the only attribute that is always preserved verbatim.

use proc_macro::TokenStream;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::{parse_macro_input, Ident, ItemFn, LitInt, Token, TraitItemFn};

struct WireArgs {
    id: u8,
}

impl Parse for WireArgs {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let mut id = None;

        while !input.is_empty() {
            let key: Ident = input.parse()?;
            input.parse::<Token![=]>()?;

            if key == "id" {
                if id.is_some() {
                    return Err(syn::Error::new(key.span(), "duplicate `id`"));
                }
                let lit: LitInt = input.parse()?;
                id = Some(lit.base10_parse().map_err(|err| {
                    syn::Error::new(lit.span(), format!("wire id must fit in a u8: {err}"))
                })?);
            } else {
                return Err(syn::Error::new(
                    key.span(),
                    "expected `id`, e.g. `#[wire(id = 4)]`",
                ));
            }

            if input.is_empty() {
                break;
            }
            input.parse::<Token![,]>()?;
        }

        let id = id.ok_or_else(|| input.error("missing `id = N`"))?;

        Ok(Self { id })
    }
}

/// Mark a TrUAPI trait method with its wire-protocol discriminant id.
///
/// ```ignore
/// #[wire(id = 4)]
/// async fn host_account_get(...) -> ...;
/// ```
///
/// Expands to the original method plus hidden doc tags that `truapi-codegen`
/// extracts from rustdoc JSON to build the wire table and versioned clients.
#[proc_macro_attribute]
pub fn wire(args: TokenStream, item: TokenStream) -> TokenStream {
    let WireArgs { id } = parse_macro_input!(args as WireArgs);
    let id_tag = format!("@wire_id={id}");

    if let Ok(mut method) = syn::parse::<TraitItemFn>(item.clone()) {
        method.attrs.push(syn::parse_quote!(#[doc = #id_tag]));
        return quote!(#method).into();
    }

    if let Ok(mut function) = syn::parse::<ItemFn>(item) {
        function.attrs.push(syn::parse_quote!(#[doc = #id_tag]));
        return quote!(#function).into();
    }

    syn::Error::new(
        proc_macro2::Span::call_site(),
        "#[wire] can only be applied to trait methods or free functions",
    )
    .to_compile_error()
    .into()
}
