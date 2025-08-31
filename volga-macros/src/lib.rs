//! Proc-Macros implementations for different features of Volga
//! 

use proc_macro::TokenStream;
use syn::parse_macro_input;

mod http;
#[cfg(feature = "jwt-auth-derive")]
mod auth;
#[cfg(feature = "di-derive")]
mod di;

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
    let input = parse_macro_input!(input as syn::DeriveInput);
    auth::expand_claims(&input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}


/// Derive macro for the `Inject` trait that always returns an error when resolving the type
///
/// Equivalent to using the `singleton!` macro.
///
/// # Example
/// ```ignore
/// use volga::di::Singleton;
/// 
/// #[derive(Singleton)]
/// struct MyType;
///
/// // This expands to:
/// // impl Inject for MyType {
/// //     fn inject(_: &Container) -> Result<Self, Error> {
/// //         Err(Error::ResolveFailed("MyType"))
/// //     }
/// // }
/// ```
#[cfg(feature = "di-derive")]
#[proc_macro_derive(Singleton)]
pub fn derive_singleton(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as syn::DeriveInput);
    di::expand_singleton(&input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
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
    let header = parse_macro_input!(attr as http::attr::HeaderInput);
    let input = parse_macro_input!(item as syn::ItemStruct);
    http::expand_http_header(&header, &input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}
