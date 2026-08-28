//! Proc-Macros implementations for different features of Volga
//!

use proc_macro::TokenStream;
use syn::parse_macro_input;

#[cfg(feature = "jwt-auth-derive")]
mod auth;
mod http;
#[cfg(feature = "validation-derive")]
mod validation;

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

/// Implements the `Validate` trait from the field attributes.
///
/// # Example
/// ```ignore
/// use volga::validation::Validate;
/// use serde::Deserialize;
///
/// #[derive(Deserialize, Validate)]
/// #[validate(schema = "check_range")]
/// struct Filter {
///     #[validate(range(min = 1, max = 100))]
///     per_page: u32,
///
///     #[validate(length(min = 1, max = 128))]
///     key: String,
/// }
/// ```
///
/// Field rules: `length(min, max, equal)`, `range(min, max)`, `nested`,
/// `custom = "path::to::fn"`, each taking an optional `message = ".."`;
/// `rename = ".."` overrides the name a failure is reported under.
/// The container takes `schema = "path::to::fn"` for rules spanning several fields.
#[cfg(feature = "validation-derive")]
#[proc_macro_derive(Validate, attributes(validate))]
pub fn derive_validate(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as syn::DeriveInput);
    validation::expand_validate(&input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}
