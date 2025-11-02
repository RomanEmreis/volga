//! Macros for HTTP

use proc_macro2::TokenStream;
use quote::quote;

pub(super) mod attr;

/// Expands a header struct into a FromHeaders implementation.
pub(super) fn expand_http_header(header: &attr::HeaderInput, input: &syn::ItemStruct) -> syn::Result<TokenStream> {
    let struct_name = &input.ident;
    let header_expr = header.as_token_stream();
    Ok(quote! {
        #input
        impl ::volga::headers::FromHeaders for #struct_name {
            #[inline]
            fn from_headers(headers: &::volga::headers::HeaderMap) -> Option<&::volga::headers::HeaderValue> {
                headers.get(#header_expr)
            }
            #[inline]
            fn header_type() -> &'static str {
                #header_expr
            }
        }
    })
}