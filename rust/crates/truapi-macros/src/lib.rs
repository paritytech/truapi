//! Proc-macros for TrUAPI trait annotations.
//!
//! `versioned_type!` is a function-like macro that generates versioned message
//! envelopes: the `Vn` enums (with SCALE codec indices) plus their
//! `Versioned`/`IntoLatest`/`FromLatest` impls from `truapi::versioned`.
//!
//! The `wire` attribute marks a trait method with
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
use proc_macro2::Literal;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::{
    Attribute, Ident, ItemFn, LitInt, Token, TraitItemFn, Type, Visibility, braced,
    parse_macro_input,
};

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

/// One sequence of versioned envelope declarations passed to `versioned_type!`.
struct VersionedInput {
    enums: Vec<VersionedEnum>,
}

impl Parse for VersionedInput {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let mut enums = Vec::new();
        while !input.is_empty() {
            enums.push(input.parse()?);
        }
        Ok(Self { enums })
    }
}

/// A single `[vis] enum Name { V1 => Ty, ... }` declaration.
struct VersionedEnum {
    attrs: Vec<Attribute>,
    vis: Visibility,
    name: Ident,
    variants: Vec<VersionedVariant>,
}

impl Parse for VersionedEnum {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let attrs = input.call(Attribute::parse_outer)?;
        let vis: Visibility = input.parse()?;
        input.parse::<Token![enum]>()?;
        let name: Ident = input.parse()?;

        let body;
        braced!(body in input);
        let mut variants = Vec::new();
        while !body.is_empty() {
            variants.push(body.parse()?);
            if body.peek(Token![,]) {
                body.parse::<Token![,]>()?;
            } else {
                break;
            }
        }

        Ok(Self {
            attrs,
            vis,
            name,
            variants,
        })
    }
}

/// A single `Vn` or `Vn => Ty` variant.
struct VersionedVariant {
    attrs: Vec<Attribute>,
    ident: Ident,
    ty: Option<Type>,
}

impl Parse for VersionedVariant {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let attrs = input.call(Attribute::parse_outer)?;
        let ident: Ident = input.parse()?;
        let ty = if input.peek(Token![=>]) {
            input.parse::<Token![=>]>()?;
            Some(input.parse()?)
        } else {
            None
        };
        Ok(Self { attrs, ident, ty })
    }
}

/// Parse the `Vn` version number from a variant identifier.
fn variant_version(ident: &Ident) -> syn::Result<u8> {
    let name = ident.to_string();
    let err = || syn::Error::new(ident.span(), "variant must be named `Vn` where n is a u8");
    name.strip_prefix('V')
        .ok_or_else(err)?
        .parse::<u8>()
        .map_err(|_| err())
}

/// Generate versioned message envelopes.
///
/// ```ignore
/// versioned_type! {
///     pub enum HostFooRequest { V1 => v01::HostFooRequest }
///     pub enum HostFooResponse { V1 }
/// }
/// ```
///
/// Each declaration becomes a SCALE enum with positional codec indices and an
/// `impl Versioned` exposing `Latest`, `LATEST`, and `version()`. Single-version
/// envelopes also get trivial `IntoLatest`/`FromLatest` impls; multi-version
/// envelopes leave those to be written by hand, since the conversion is bespoke.
///
/// The declared visibility (`pub`, `pub(crate)`, or none) carries through to the
/// generated enum.
///
/// The generated impls name `crate::versioned::*` traits, so invoke this from
/// within the `truapi` crate.
#[proc_macro]
pub fn versioned_type(item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as VersionedInput);
    match expand_versioned(&input) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

fn expand_versioned(input: &VersionedInput) -> syn::Result<proc_macro2::TokenStream> {
    let mut out = proc_macro2::TokenStream::new();
    for enum_def in &input.enums {
        out.extend(expand_versioned_enum(enum_def)?);
    }
    Ok(out)
}

fn expand_versioned_enum(def: &VersionedEnum) -> syn::Result<proc_macro2::TokenStream> {
    let VersionedEnum {
        attrs,
        vis,
        name,
        variants,
    } = def;

    if variants.is_empty() {
        return Err(syn::Error::new(
            name.span(),
            "versioned enum needs at least one variant",
        ));
    }

    let mut variant_defs = Vec::new();
    let mut version_arms = Vec::new();
    for (i, variant) in variants.iter().enumerate() {
        let expected = i + 1;
        let version = variant_version(&variant.ident)?;
        if usize::from(version) != expected {
            return Err(syn::Error::new(
                variant.ident.span(),
                format!("expected variant `V{expected}`; versions must be contiguous from 1"),
            ));
        }

        let index = Literal::u8_unsuffixed(i as u8);
        let version_lit = Literal::u8_unsuffixed(version);
        let vattrs = &variant.attrs;
        let vident = &variant.ident;
        match &variant.ty {
            Some(ty) => {
                variant_defs.push(quote! { #(#vattrs)* #[codec(index = #index)] #vident(#ty) });
                version_arms.push(quote! { Self::#vident(..) => #version_lit });
            }
            None => {
                variant_defs.push(quote! { #(#vattrs)* #[codec(index = #index)] #vident });
                version_arms.push(quote! { Self::#vident => #version_lit });
            }
        }
    }

    let doc = format!("Versioned envelope for [`{name}`].");
    let latest_lit = Literal::u8_unsuffixed(variants.len() as u8);
    let latest_ty = match &variants.last().expect("checked non-empty").ty {
        Some(ty) => quote! { #ty },
        None => quote! { () },
    };

    let mut tokens = quote! {
        #(#attrs)*
        #[doc = #doc]
        #[derive(Debug, Clone, PartialEq, Eq, parity_scale_codec::Encode, parity_scale_codec::Decode)]
        #vis enum #name {
            #(#variant_defs),*
        }

        impl crate::versioned::Versioned for #name {
            type Latest = #latest_ty;
            const LATEST: u8 = #latest_lit;
            fn version(&self) -> u8 {
                match self {
                    #(#version_arms),*
                }
            }
        }
    };

    if let [only] = &variants[..] {
        let vident = &only.ident;
        let (into_body, from_param, from_body) = match &only.ty {
            Some(_) => (
                quote! { match self { Self::#vident(inner) => inner } },
                quote! { latest },
                quote! { Self::#vident(latest) },
            ),
            None => (
                quote! { match self { Self::#vident => () } },
                quote! { _latest },
                quote! { Self::#vident },
            ),
        };
        tokens.extend(quote! {
            impl crate::versioned::IntoLatest for #name {
                fn into_latest(self) -> Self::Latest {
                    #into_body
                }
            }

            impl crate::versioned::FromLatest for #name {
                fn from_latest(#from_param: Self::Latest, _target: u8) -> Self {
                    #from_body
                }
            }
        });
    }

    Ok(tokens)
}
