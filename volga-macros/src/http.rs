//! Macros for HTTP

use proc_macro2::TokenStream;
use quote::quote;

pub(super) mod attr;

/// Expands a header struct into a FromHeaders implementation.
pub(super) fn expand_http_header(header: &attr::HeaderInput, input: &syn::ItemStruct) -> syn::Result<TokenStream> {
    let struct_name = &input.ident;
    let header_expr = header.as_token_stream();
    Ok(quote! {
        #[derive(Clone)]
        #input
        impl ::volga::headers::FromHeaders for #struct_name {
            const NAME: ::volga::headers::HeaderName = ::volga::headers::HeaderName::from_static(#header_expr);
            
            #[inline]
            fn from_headers(headers: &::volga::headers::HeaderMap) -> Option<&::volga::headers::HeaderValue> {
                headers.get(#header_expr)
            }
        }
    })
}