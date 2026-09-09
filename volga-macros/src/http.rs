//! Macros for HTTP

use proc_macro2::TokenStream;
use quote::quote;
use syn::spanned::Spanned;

pub(super) mod attr;

/// Expands a header struct into a FromHeaders implementation.
pub(super) fn expand_http_header(
    header: &attr::HeaderInput,
    input: &syn::ItemStruct,
) -> syn::Result<TokenStream> {
    // The name is the whole of a typed header: the value it carries lives in
    // `Header<T>`, never in `T` itself, so a field here is read by nothing and the
    // constructors below would silently ignore it.
    if !matches!(input.fields, syn::Fields::Unit) {
        return Err(syn::Error::new(
            input.fields.span(),
            "`#[http_header]` can only be applied to a unit-like struct",
        ));
    }

    let struct_name = &input.ident;
    let header_expr = header.as_token_stream();
    Ok(quote! {
        #[derive(Clone)]
        #input
        impl #struct_name {
            /// Creates a new instance of [`Header<T>`] from a `static str`
            #[inline(always)]
            pub const fn from_static(value: &'static str) -> ::volga::headers::Header<#struct_name> {
                ::volga::headers::Header::<#struct_name>::from_static(value)
            }

            /// Construct a typed header from bytes (validated).
            #[inline]
            pub fn from_bytes(bytes: &[u8]) -> Result<::volga::headers::Header<#struct_name>, ::volga::error::Error> {
                ::volga::headers::Header::<#struct_name>::from_bytes(bytes)
            }

            /// Wrap an owned raw HeaderValue (validated elsewhere).
            #[inline]
            pub fn new(value: ::volga::headers::HeaderValue) -> ::volga::headers::Header<#struct_name> {
                ::volga::headers::Header::<#struct_name>::new(value)
            }

            /// Wrap a borrowed raw HeaderValue (validated elsewhere).
            #[inline]
            pub fn from_ref(value: &::volga::headers::HeaderValue) -> ::volga::headers::Header<#struct_name> {
                ::volga::headers::Header::<#struct_name>::from_ref(value)
            }
        }
        impl ::volga::headers::FromHeaders for #struct_name {
            const NAME: ::volga::headers::HeaderName = ::volga::headers::HeaderName::from_static(#header_expr);

            #[inline]
            fn from_headers(headers: &::volga::headers::HeaderMap) -> Option<&::volga::headers::HeaderValue> {
                headers.get(Self::NAME)
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use syn::parse_quote;

    fn header() -> attr::HeaderInput {
        attr::HeaderInput::Literal(parse_quote!("x-api-key"))
    }

    #[test]
    fn it_expands_a_unit_like_struct() {
        let input: syn::ItemStruct = parse_quote! { pub struct ApiKey; };
        assert!(expand_http_header(&header(), &input).is_ok());
    }

    #[test]
    fn it_rejects_named_fields() {
        let input: syn::ItemStruct = parse_quote! { pub struct ApiKey { inner: String } };
        let err = expand_http_header(&header(), &input).unwrap_err();
        assert_eq!(
            err.to_string(),
            "`#[http_header]` can only be applied to a unit-like struct"
        );
    }

    #[test]
    fn it_rejects_unnamed_fields() {
        let input: syn::ItemStruct = parse_quote! { pub struct ApiKey(String); };
        let err = expand_http_header(&header(), &input).unwrap_err();
        assert_eq!(
            err.to_string(),
            "`#[http_header]` can only be applied to a unit-like struct"
        );
    }
}
