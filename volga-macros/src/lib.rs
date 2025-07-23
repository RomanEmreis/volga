//! Proc-Macros implementations for different features of Volga

#[cfg(feature = "jwt-auth-derive")]
use {
    syn::{parse_macro_input, DeriveInput, Data, Fields},
    proc_macro::TokenStream,
    quote::quote
};

#[cfg(feature = "jwt-auth-derive")]
#[proc_macro_derive(AuthClaims)]
pub fn derive_auth_claims(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let name = &input.ident;

    let mut role_impl = quote! {};
    let mut roles_impl = quote! {};
    let mut permissions_impl = quote! {};

    if let Data::Struct(data_struct) = &input.data {
        if let Fields::Named(fields) = &data_struct.fields {
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
    }

    let expanded = quote! {
        impl ::volga::auth::AuthClaims for #name {
            #role_impl
            #roles_impl
            #permissions_impl
        }
    };

    TokenStream::from(expanded)
}
