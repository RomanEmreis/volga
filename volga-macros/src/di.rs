//! Macros for dependency injection

use proc_macro2::TokenStream;
use quote::quote;

/// Creates a derive-macro for a singleton
pub(super) fn expand_singleton(input: &syn::DeriveInput) -> syn::Result<TokenStream> {
    let name = &input.ident;
    Ok(quote! {
        impl ::volga::di::Inject for #name {
            #[inline]
            fn inject(_: &::volga::di::Container) -> Result<Self, ::volga::di::error::Error> {
                Err(::volga::di::error::Error::ResolveFailed(stringify!(#name)))
            }
        }
    })
}