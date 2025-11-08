//! Macros for Authentication and Authorization

use proc_macro2::TokenStream;
use quote::quote;

/// Expands a derive-macro for AuthClaims
pub(super) fn expand_claims(input: &syn::DeriveInput) -> syn::Result<TokenStream> {
    let name = &input.ident;
    let mut role_impl = quote! {};
    let mut roles_impl = quote! {};
    let mut permissions_impl = quote! {};
    if let syn::Data::Struct(data_struct) = &input.data 
        && let syn::Fields::Named(fields) = &data_struct.fields {
        for field in &fields.named {
            if let Some(ident) = &field.ident {
                let ident_str = ident.to_string();
                match ident_str.as_str() {
                    "role" => {
                        role_impl = quote! {
                                fn role(&self) -> Option<&str> {
                                    Some(&self.role)
                                }
                            };
                    }
                    "roles" => {
                        roles_impl = quote! {
                                fn roles(&self) -> Option<&[String]> {
                                    Some(&self.roles)
                                }
                            };
                    }
                    "permissions" => {
                        permissions_impl = quote! {
                                fn permissions(&self) -> Option<&[String]> {
                                    Some(&self.permissions)
                                }
                            };
                    }
                    _ => {}
                }
            }
        }
    }
    Ok(quote! {
        impl ::volga::auth::AuthClaims for #name {
            #role_impl
            #roles_impl
            #permissions_impl
        }
    })
}