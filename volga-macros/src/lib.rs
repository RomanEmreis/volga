//! Proc-Macros implementations for different features of Volga
//! 

use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, ItemStruct};
#[cfg(feature = "jwt-auth-derive")]
use syn::{DeriveInput, Data, Fields};

mod attr;

/// Implements the `AuthClaims` trait for the custom claims structure
/// 
/// # Example
/// ```ignore
/// use volga::auth::Claims;
/// use serde::{Serialize, Deserialize}
/// 
/// #[derive(Claims, Serialize, Deserialize)]
/// struct Claims {
///     sub: String,
///     iss: String,
///     aud: String,
///     company: String,
///     roles: Vec<String>,
///     permissions: Vec<String>,
///     exp: u64,
/// }
/// ```
#[cfg(feature = "jwt-auth-derive")]
#[proc_macro_derive(Claims)]
pub fn derive_claims(input: TokenStream) -> TokenStream {
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

/// Attribute macro to implement the `FromHeaders` trait for a struct,
/// based on a specified HTTP header.
///
/// # Example
/// Provide either a string literal for the inline header name:
/// ```ignore
/// use volga::headers::http_header;
/// 
/// #[http_header("x-api-key")]
/// pub struct ApiKey;
/// ```
/// Or use a constant:
/// ```ignore
/// use volga::headers::http_header;
/// 
/// const X_HEADER: &str = "x-auth-token";
///
/// #[http_header(X_HEADER)]
/// pub struct AuthToken;
/// ```
/// # Errors
/// This macro will fail to compile if:
/// - The attribute is missing
/// - The argument is not a string literal or identifier
/// - The input is not a unit-like struct
#[proc_macro_attribute]
pub fn http_header(attr: TokenStream, item: TokenStream) -> TokenStream {
    let header = parse_macro_input!(attr as attr::HeaderInput);
    let input = parse_macro_input!(item as ItemStruct);

    let struct_name = &input.ident;
    let header_expr = header.as_token_stream();

    let expanded = quote! {
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
    };

    expanded.into()
}
