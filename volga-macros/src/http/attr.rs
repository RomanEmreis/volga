//! Attribute macro helpers

use syn::{
    parse::{Parse, ParseStream},
    Ident, LitStr, Result,
};

/// Represents the input to the `#[http_header(...)]` macro.
///
/// This can either be:
/// - A string literal (e.g. `"x-api-key"`)
/// - An identifier (e.g. `X_API_KEY` constant)
///
/// The actual header name will be extracted and used as an argument
/// to the `HeaderMap::get()` method.
pub(crate) enum HeaderInput {
    /// A literal string (e.g., `"x-api-key"`)
    Literal(LitStr),

    /// A constant identifier (e.g., `X_API_KEY`)
    Constant(Ident),
}

impl Parse for HeaderInput {
    /// Parses the header attribute from macro input.
    ///
    /// Accepts:
    /// - A string literal, e.g. `"x-api-key"`
    /// - An identifier, e.g. `X_API_KEY`
    ///
    /// Returns an error if input is empty or of an unsupported form.
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        if input.peek(LitStr) {
            let lit: LitStr = input.parse()?;
            Ok(HeaderInput::Literal(lit))
        } else if input.peek(Ident) {
            let ident: Ident = input.parse()?;
            Ok(HeaderInput::Constant(ident))
        } else {
            Err(input.error("expected a string literal or an identifier"))
        }
    }
}

impl HeaderInput {
    /// Converts the parsed attribute into a usable token stream,
    /// for insertion into the generated `FromHeaders` implementation.
    ///
    /// Returns either:
    /// - `quote! { "x-api-key" }` if literal
    /// - `quote! { X_API_KEY }` if constant
    pub(super) fn as_token_stream(&self) -> proc_macro2::TokenStream {
        match self {
            HeaderInput::Literal(lit) => quote::quote! { #lit },
            HeaderInput::Constant(ident) => quote::quote! { #ident },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use syn::parse_str;

    #[test]
    fn it_parses_literal_header() {
        let parsed: HeaderInput = parse_str("\"x-api-key\"").unwrap();
        match parsed {
            HeaderInput::Literal(lit) => assert_eq!(lit.value(), "x-api-key"),
            _ => panic!("Expected literal"),
        }
    }

    #[test]
    fn it_parses_identifier_header() {
        let parsed: HeaderInput = parse_str("X_API_KEY").unwrap();
        match parsed {
            HeaderInput::Constant(ident) => assert_eq!(ident.to_string(), "X_API_KEY"),
            _ => panic!("Expected identifier"),
        }
    }

    #[test]
    fn it_fails_on_number() {
        let parsed: Result<HeaderInput> = parse_str("123");
        assert!(parsed.is_err());
    }

    #[test]
    fn it_fails_on_empty() {
        let parsed: Result<HeaderInput> = parse_str("");
        assert!(parsed.is_err());
    }
}

