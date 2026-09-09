//! Macros for Authentication and Authorization

use proc_macro2::TokenStream;
use quote::quote;
use syn::spanned::Spanned;

/// Expands a derive-macro for AuthClaims
pub(super) fn expand_claims(input: &syn::DeriveInput) -> syn::Result<TokenStream> {
    // A JWT payload is a JSON object, so the claims it deserializes into is a struct.
    // A struct carrying none of the three recognised fields is fine - it simply has no
    // roles to report and every method of the trait has a default - but an enum or a
    // union could never carry them, and would otherwise expand to an empty impl.
    let syn::Data::Struct(data_struct) = &input.data else {
        return Err(syn::Error::new(
            input.span(),
            "`Claims` can only be derived for structs",
        ));
    };

    let name = &input.ident;
    let mut role_impl = quote! {};
    let mut roles_impl = quote! {};
    let mut permissions_impl = quote! {};
    if let syn::Fields::Named(fields) = &data_struct.fields {
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

#[cfg(test)]
mod tests {
    use super::*;
    use syn::parse_quote;

    #[test]
    fn it_expands_a_struct_carrying_none_of_the_recognised_fields() {
        let input: syn::DeriveInput = parse_quote! { struct Claims { sub: String } };
        assert!(expand_claims(&input).is_ok());
    }

    #[test]
    fn it_expands_a_struct_without_named_fields() {
        let input: syn::DeriveInput = parse_quote! { struct Claims; };
        assert!(expand_claims(&input).is_ok());
    }

    #[test]
    fn it_rejects_an_enum() {
        let input: syn::DeriveInput = parse_quote! { enum Claims { Anonymous } };
        let err = expand_claims(&input).unwrap_err();
        assert_eq!(err.to_string(), "`Claims` can only be derived for structs");
    }

    #[test]
    fn it_rejects_a_union() {
        let input: syn::DeriveInput = parse_quote! { union Claims { role: u32 } };
        let err = expand_claims(&input).unwrap_err();
        assert_eq!(err.to_string(), "`Claims` can only be derived for structs");
    }
}
