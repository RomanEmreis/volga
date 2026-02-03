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
                headers.get(#header_expr)
            }
        }
    })
}