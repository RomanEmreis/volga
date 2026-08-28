//! Macros for input validation

use proc_macro2::TokenStream;
use quote::quote;
use syn::{Data, DeriveInput, Fields, LitStr, Path, Type, parse::Parse, spanned::Spanned};

/// A rule attached to a single field
enum Rule {
    Length {
        min: Option<usize>,
        max: Option<usize>,
        equal: Option<usize>,
        message: Option<String>,
    },
    Range {
        min: Option<Box<syn::Expr>>,
        max: Option<Box<syn::Expr>>,
        message: Option<String>,
    },
    Nested,
    Custom(Path),
}

/// Everything the expansion needs to know about one field
struct Field<'a> {
    ident: &'a syn::Ident,
    ty: &'a Type,
    /// The name this field is reported under, matching what the client sent
    name: String,
    rules: Vec<Rule>,
}

/// Expands the `Validate` derive macro
pub(super) fn expand_validate(input: &DeriveInput) -> syn::Result<TokenStream> {
    let Data::Struct(data) = &input.data else {
        return Err(syn::Error::new(
            input.span(),
            "`Validate` can only be derived for structs",
        ));
    };
    let Fields::Named(fields) = &data.fields else {
        return Err(syn::Error::new(
            input.span(),
            "`Validate` can only be derived for structs with named fields",
        ));
    };

    let schema_fns = container_schema_fns(&input.attrs)?;
    let rename_all = serde_rename_all(&input.attrs);

    let mut parsed = Vec::with_capacity(fields.named.len());
    for field in &fields.named {
        let ident = field
            .ident
            .as_ref()
            .ok_or_else(|| syn::Error::new(field.span(), "expected a named field"))?;
        let rules = field_rules(&field.attrs)?;
        let name = field_name(&field.attrs, ident, rename_all.as_deref())?;
        parsed.push(Field {
            ident,
            ty: &field.ty,
            name,
            rules,
        });
    }

    let checks = parsed.iter().map(render_field).collect::<Vec<_>>();
    let schema_checks = schema_fns.iter().map(|path| {
        quote! {
            if let ::std::result::Result::Err(__failed) = #path(self) {
                __errors.merge(__failed);
            }
        }
    });

    let constraints = parsed
        .iter()
        .flat_map(render_constraints)
        .collect::<Vec<_>>();

    let name = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    Ok(quote! {
        impl #impl_generics ::volga::validation::Validate for #name #ty_generics #where_clause {
            type Error = ::volga::validation::ValidationError;

            fn validate(&self) -> ::std::result::Result<(), Self::Error> {
                let mut __errors = ::volga::validation::ValidationError::new();
                #(#checks)*
                #(#schema_checks)*
                __errors.into_result()
            }

            fn constraints() -> &'static [::volga::validation::Constraint] {
                const __CONSTRAINTS: &[::volga::validation::Constraint] = &[#(#constraints),*];
                __CONSTRAINTS
            }
        }
    })
}

/// Renders the checks of a single field, unwrapping an `Option` first
fn render_field(field: &Field<'_>) -> TokenStream {
    if field.rules.is_empty() {
        return quote! {};
    }

    let ident = field.ident;
    let checks = field.rules.iter().map(|rule| render_rule(field, rule));

    if inner_type(field.ty, "Option").is_some() {
        quote! {
            if let ::std::option::Option::Some(__value) = &self.#ident {
                #(#checks)*
            }
        }
    } else {
        quote! {
            {
                let __value = &self.#ident;
                #(#checks)*
            }
        }
    }
}

/// Renders a single rule against `__value`
fn render_rule(field: &Field<'_>, rule: &Rule) -> TokenStream {
    let name = &field.name;
    match rule {
        Rule::Length {
            min,
            max,
            equal,
            message,
        } => {
            let (condition, default) = length_condition(*min, *max, *equal);
            let message = message.clone().unwrap_or(default);
            quote! {
                {
                    let __len = ::volga::validation::rules::length(__value);
                    if #condition {
                        __errors.push(#name, #message);
                    }
                }
            }
        }
        Rule::Range { min, max, message } => {
            let (condition, default) = range_condition(min.as_deref(), max.as_deref());
            let message = message.clone().unwrap_or(default);
            quote! {
                if #condition {
                    __errors.push(#name, #message);
                }
            }
        }
        Rule::Nested => {
            // Detect a collection at expansion time, the way the `Option` unwrapping does
            let is_seq = inner_type(field.ty, "Vec").is_some()
                || inner_type(field.ty, "Option")
                    .and_then(|ty| inner_type(ty, "Vec"))
                    .is_some();
            if is_seq {
                quote! { ::volga::validation::rules::nested_each(&mut __errors, #name, __value); }
            } else {
                quote! { ::volga::validation::rules::nested(&mut __errors, #name, __value); }
            }
        }
        Rule::Custom(path) => {
            // Called directly rather than through a helper, so that a check written
            // against `&str` still accepts a `&String` field by deref coercion
            quote! {
                if let ::std::result::Result::Err(__failed) = #path(__value) {
                    __errors.merge_at(#name, __failed);
                }
            }
        }
    }
}

/// Builds the `length` condition and the message that describes it
fn length_condition(
    min: Option<usize>,
    max: Option<usize>,
    equal: Option<usize>,
) -> (TokenStream, String) {
    if let Some(equal) = equal {
        return (
            quote! { __len != #equal },
            format!("length must be exactly {equal}"),
        );
    }
    match (min, max) {
        (Some(min), Some(max)) => (
            quote! { !(#min..=#max).contains(&__len) },
            format!("length must be between {min} and {max}"),
        ),
        (Some(min), None) => (
            quote! { __len < #min },
            format!("length must be at least {min}"),
        ),
        (None, Some(max)) => (
            quote! { __len > #max },
            format!("length must be at most {max}"),
        ),
        (None, None) => (quote! { false }, String::new()),
    }
}

/// Builds the `range` condition and the message that describes it
fn range_condition(min: Option<&syn::Expr>, max: Option<&syn::Expr>) -> (TokenStream, String) {
    match (min, max) {
        (Some(min), Some(max)) => (
            quote! { !(#min..=#max).contains(__value) },
            format!("must be between {} and {}", lit_text(min), lit_text(max)),
        ),
        // Phrased as "does not satisfy the bound" rather than "violates" it, so that a value
        // no comparison holds for - `NaN` - is rejected here the way the two-sided branch
        // already rejects it, instead of passing because every comparison against it is false
        (Some(min), None) => (
            quote! { !(#min..).contains(__value) },
            format!("must be at least {}", lit_text(min)),
        ),
        (None, Some(max)) => (
            quote! { !(..=#max).contains(__value) },
            format!("must be at most {}", lit_text(max)),
        ),
        (None, None) => (quote! { false }, String::new()),
    }
}

/// Renders the constraints of a field for the OpenAPI schema
fn render_constraints(field: &Field<'_>) -> Vec<TokenStream> {
    let name = &field.name;
    let mut out = Vec::new();
    let mut push = |kind: TokenStream| {
        out.push(quote! {
            ::volga::validation::Constraint::new(#name, ::volga::validation::ConstraintKind::#kind)
        });
    };

    for rule in &field.rules {
        match rule {
            Rule::Length {
                min, max, equal, ..
            } => {
                let (min_kind, max_kind) = length_keywords(field.ty);
                if let Some(equal) = equal {
                    push(quote! { #min_kind(#equal) });
                    push(quote! { #max_kind(#equal) });
                } else {
                    if let Some(min) = min {
                        push(quote! { #min_kind(#min) });
                    }
                    if let Some(max) = max {
                        push(quote! { #max_kind(#max) });
                    }
                }
            }
            Rule::Range { min, max, .. } => {
                if let Some(min) = min.as_deref().and_then(lit_as_f64) {
                    push(quote! { Minimum(#min) });
                }
                if let Some(max) = max.as_deref().and_then(lit_as_f64) {
                    push(quote! { Maximum(#max) });
                }
            }
            Rule::Nested | Rule::Custom(_) => {}
        }
    }
    out
}

/// Reads the `#[validate(schema = "..")]` functions of the container
fn container_schema_fns(attrs: &[syn::Attribute]) -> syn::Result<Vec<Path>> {
    let mut out = Vec::new();
    for attr in attrs.iter().filter(|a| a.path().is_ident("validate")) {
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("schema") {
                let lit: LitStr = meta.value()?.parse()?;
                out.push(lit.parse()?);
                Ok(())
            } else {
                Err(meta.error("unknown container attribute, expected `schema = \"path::to::fn\"`"))
            }
        })?;
    }
    Ok(out)
}

/// Reads the rules of a single field
fn field_rules(attrs: &[syn::Attribute]) -> syn::Result<Vec<Rule>> {
    let mut out = Vec::new();
    for attr in attrs.iter().filter(|a| a.path().is_ident("validate")) {
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("length") {
                let (mut min, mut max, mut equal, mut message) = (None, None, None, None);
                meta.parse_nested_meta(|meta| {
                    if meta.path.is_ident("min") {
                        min = Some(meta.value()?.parse::<syn::LitInt>()?.base10_parse()?);
                    } else if meta.path.is_ident("max") {
                        max = Some(meta.value()?.parse::<syn::LitInt>()?.base10_parse()?);
                    } else if meta.path.is_ident("equal") {
                        equal = Some(meta.value()?.parse::<syn::LitInt>()?.base10_parse()?);
                    } else if meta.path.is_ident("message") {
                        message = Some(meta.value()?.parse::<LitStr>()?.value());
                    } else {
                        return Err(meta.error("expected `min`, `max`, `equal` or `message`"));
                    }
                    Ok(())
                })?;
                if min.is_none() && max.is_none() && equal.is_none() {
                    return Err(
                        meta.error("`length` needs at least one of `min`, `max` or `equal`")
                    );
                }
                out.push(Rule::Length {
                    min,
                    max,
                    equal,
                    message,
                });
            } else if meta.path.is_ident("range") {
                let (mut min, mut max, mut message) = (None, None, None);
                meta.parse_nested_meta(|meta| {
                    if meta.path.is_ident("min") {
                        min = Some(Box::new(meta.value()?.parse::<syn::Expr>()?));
                    } else if meta.path.is_ident("max") {
                        max = Some(Box::new(meta.value()?.parse::<syn::Expr>()?));
                    } else if meta.path.is_ident("message") {
                        message = Some(meta.value()?.parse::<LitStr>()?.value());
                    } else {
                        return Err(meta.error("expected `min`, `max` or `message`"));
                    }
                    Ok(())
                })?;
                if min.is_none() && max.is_none() {
                    return Err(meta.error("`range` needs at least one of `min` or `max`"));
                }
                out.push(Rule::Range { min, max, message });
            } else if meta.path.is_ident("nested") {
                out.push(Rule::Nested);
            } else if meta.path.is_ident("custom") {
                let lit: LitStr = meta.value()?.parse()?;
                out.push(Rule::Custom(lit.parse()?));
            } else if meta.path.is_ident("rename") {
                // read by `field_name`
                let _: LitStr = meta.value()?.parse()?;
            } else {
                return Err(meta.error(
                    "unknown rule, expected `length`, `range`, `nested`, `custom` or `rename`",
                ));
            }
            Ok(())
        })?;
    }
    Ok(out)
}

/// Resolves the name a field is reported under: an explicit `rename` wins over
/// what serde renamed it to, which wins over the field's own identifier
fn field_name(
    attrs: &[syn::Attribute],
    ident: &syn::Ident,
    rename_all: Option<&str>,
) -> syn::Result<String> {
    let mut renamed = None;
    for attr in attrs.iter().filter(|a| a.path().is_ident("validate")) {
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("rename") {
                renamed = Some(meta.value()?.parse::<LitStr>()?.value());
            } else if meta.input.peek(syn::Token![=]) {
                let _: syn::Expr = meta.value()?.parse()?;
            } else if meta.input.peek(syn::token::Paren) {
                let content;
                syn::parenthesized!(content in meta.input);
                let _: TokenStream = content.parse()?;
            }
            Ok(())
        })?;
    }
    if let Some(renamed) = renamed {
        return Ok(renamed);
    }
    if let Some(renamed) = serde_rename(attrs) {
        return Ok(renamed);
    }
    let ident = ident.to_string();
    let ident = ident.strip_prefix("r#").unwrap_or(&ident);
    Ok(match rename_all {
        Some(rule) => apply_rename_rule(ident, rule),
        None => ident.to_owned(),
    })
}

/// Reads `#[serde(rename = "..")]` (or its `deserialize` half) off a field
fn serde_rename(attrs: &[syn::Attribute]) -> Option<String> {
    let mut renamed = None;
    for attr in attrs.iter().filter(|a| a.path().is_ident("serde")) {
        // serde's grammar is not ours to police - anything unexpected is skipped
        let _ = attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("rename") {
                if meta.input.peek(syn::Token![=]) {
                    renamed = Some(meta.value()?.parse::<LitStr>()?.value());
                } else {
                    let content;
                    syn::parenthesized!(content in meta.input);
                    let nested = content.parse_terminated(syn::Meta::parse, syn::Token![,])?;
                    for meta in nested {
                        if let syn::Meta::NameValue(nv) = meta
                            && nv.path.is_ident("deserialize")
                            && let syn::Expr::Lit(lit) = &nv.value
                            && let syn::Lit::Str(lit) = &lit.lit
                        {
                            renamed = Some(lit.value());
                        }
                    }
                }
            } else if meta.input.peek(syn::Token![=]) {
                let _: syn::Expr = meta.value()?.parse()?;
            } else if meta.input.peek(syn::token::Paren) {
                let content;
                syn::parenthesized!(content in meta.input);
                let _: TokenStream = content.parse()?;
            }
            Ok(())
        });
    }
    renamed
}

/// Reads `#[serde(rename_all = "..")]` off the container
fn serde_rename_all(attrs: &[syn::Attribute]) -> Option<String> {
    let mut rule = None;
    for attr in attrs.iter().filter(|a| a.path().is_ident("serde")) {
        let _ = attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("rename_all") {
                if meta.input.peek(syn::Token![=]) {
                    rule = Some(meta.value()?.parse::<LitStr>()?.value());
                } else {
                    let content;
                    syn::parenthesized!(content in meta.input);
                    let nested = content.parse_terminated(syn::Meta::parse, syn::Token![,])?;

                    for meta in nested {
                        if let syn::Meta::NameValue(nv) = meta
                            && nv.path.is_ident("deserialize")
                            && let syn::Expr::Lit(lit) = &nv.value
                            && let syn::Lit::Str(lit) = &lit.lit
                        {
                            rule = Some(lit.value());
                        }
                    }
                }
            } else if meta.input.peek(syn::Token![=]) {
                let _: syn::Expr = meta.value()?.parse()?;
            } else if meta.input.peek(syn::token::Paren) {
                let content;
                syn::parenthesized!(content in meta.input);
                let _: TokenStream = content.parse()?;
            }
            Ok(())
        });
    }
    rule
}

/// Applies serde's `rename_all` rules to a snake_case identifier
fn apply_rename_rule(ident: &str, rule: &str) -> String {
    match rule {
        "lowercase" => ident.to_lowercase(),
        "UPPERCASE" => ident.to_uppercase(),
        "PascalCase" => ident
            .split('_')
            .map(capitalize)
            .collect::<Vec<_>>()
            .concat(),
        "camelCase" => {
            let pascal = apply_rename_rule(ident, "PascalCase");
            let mut chars = pascal.chars();
            match chars.next() {
                Some(first) => first.to_lowercase().collect::<String>() + chars.as_str(),
                None => pascal,
            }
        }
        "snake_case" => ident.to_owned(),
        "SCREAMING_SNAKE_CASE" => ident.to_uppercase(),
        "kebab-case" => ident.replace('_', "-"),
        "SCREAMING-KEBAB-CASE" => ident.to_uppercase().replace('_', "-"),
        _ => ident.to_owned(),
    }
}

fn capitalize(word: &str) -> String {
    let mut chars = word.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// Picks the OpenAPI keywords a `length` rule publishes under.
///
/// The keywords are not interchangeable: `minLength` / `maxLength` count the characters of a
/// string, `minItems` / `maxItems` the elements of an array, and `minProperties` /
/// `maxProperties` the members of an object. A collection published as `minLength` reads as
/// unconstrained to a generated client, even though it is enforced at runtime.
///
/// The type is read the way the `Option` unwrapping reads it - syntactically. An alias hides
/// what it names, so anything unrecognized falls back to the string keywords.
fn length_keywords(ty: &Type) -> (TokenStream, TokenStream) {
    let ty = inner_type(ty, "Option").unwrap_or(ty);
    let name = match ty {
        Type::Slice(_) | Type::Array(_) => "Vec".to_owned(),
        Type::Path(path) => match path.path.segments.last() {
            Some(segment) => segment.ident.to_string(),
            None => String::new(),
        },
        _ => String::new(),
    };

    match name.as_str() {
        "Vec" | "VecDeque" | "HashSet" | "BTreeSet" | "BinaryHeap" => {
            (quote! { MinItems }, quote! { MaxItems })
        }
        "HashMap" | "BTreeMap" => (quote! { MinProperties }, quote! { MaxProperties }),
        _ => (quote! { MinLength }, quote! { MaxLength }),
    }
}

/// Returns the type argument of `Wrapper<T>` when the type names that wrapper
fn inner_type<'a>(ty: &'a Type, wrapper: &str) -> Option<&'a Type> {
    let Type::Path(path) = ty else { return None };
    let segment = path.path.segments.last()?;
    if segment.ident != wrapper {
        return None;
    }
    let syn::PathArguments::AngleBracketed(args) = &segment.arguments else {
        return None;
    };
    args.args.iter().find_map(|arg| match arg {
        syn::GenericArgument::Type(ty) => Some(ty),
        _ => None,
    })
}

/// Renders a numeric bound the way it was written, negation included
fn lit_text(expr: &syn::Expr) -> String {
    match unwrap_neg(expr) {
        Some((negative, syn::Lit::Int(lit))) => sign(negative) + lit.base10_digits(),
        Some((negative, syn::Lit::Float(lit))) => sign(negative) + lit.base10_digits(),
        _ => String::new(),
    }
}

/// Reads a numeric bound as `f64` for the OpenAPI constraint table
fn lit_as_f64(expr: &syn::Expr) -> Option<f64> {
    let (negative, lit) = unwrap_neg(expr)?;
    let value: f64 = match lit {
        syn::Lit::Int(lit) => lit.base10_parse().ok()?,
        syn::Lit::Float(lit) => lit.base10_parse().ok()?,
        _ => return None,
    };
    Some(if negative { -value } else { value })
}

/// Reads a literal bound, reporting whether it was negated
fn unwrap_neg(expr: &syn::Expr) -> Option<(bool, &syn::Lit)> {
    match expr {
        syn::Expr::Lit(lit) => Some((false, &lit.lit)),
        syn::Expr::Unary(unary) if matches!(unary.op, syn::UnOp::Neg(_)) => {
            match unary.expr.as_ref() {
                syn::Expr::Lit(lit) => Some((true, &lit.lit)),
                _ => None,
            }
        }
        _ => None,
    }
}

fn sign(negative: bool) -> String {
    if negative { "-".into() } else { String::new() }
}
