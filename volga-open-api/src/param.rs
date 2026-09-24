//! Type and utils for OpenAPI parameters.

use crate::schema::OpenApiSchema;
use serde::{Deserialize, Serialize};

/// OpenAPI parameter definition.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct OpenApiParameter {
    pub(super) name: String,
    #[serde(rename = "in")]
    pub(super) location: String,
    pub(super) required: bool,
    pub(super) schema: OpenApiSchema,
}

/// Normalize Open API route path
///
/// A catch-all parameter, `{*name}`, is described as the path parameter `{name}`: OpenAPI
/// templates a path one segment at a time, so a value spanning several segments has no
/// faithful spelling there, and this is the closest one.
pub(super) fn normalize_openapi_path(path: &str) -> (String, Vec<OpenApiParameter>) {
    let mut params = Vec::new();
    let mut out = String::with_capacity(path.len());

    let path = if path.is_empty() { "/" } else { path };

    if path.starts_with('/') {
        out.push('/');
    }

    for seg in path.split('/').filter(|s| !s.is_empty()) {
        if !out.ends_with('/') {
            out.push('/');
        }

        if let Some((name, schema_opt)) = parse_typed_param_segment(seg) {
            out.push('{');
            out.push_str(&name);
            out.push('}');

            let schema = schema_opt.unwrap_or_else(OpenApiSchema::string);

            params.push(OpenApiParameter {
                name,
                location: "path".to_string(),
                required: true,
                schema,
            });
        } else {
            out.push_str(seg);
        }
    }

    (out, params)
}

/// Returns `true` when two normalized OpenAPI paths are the same hierarchy: the same literal
/// segments, with a parameter wherever the other has one, whatever the two call it.
///
/// OpenAPI 3.0 forbids two such paths in one document (the Paths Object: templated paths
/// with the same hierarchy but different names "MUST NOT exist as they are identical").
pub(super) fn is_same_hierarchy(left: &str, right: &str) -> bool {
    fn segments(path: &str) -> impl Iterator<Item = &str> {
        path.split('/').filter(|segment| !segment.is_empty())
    }

    fn is_param(segment: &str) -> bool {
        segment.starts_with('{') && segment.ends_with('}')
    }

    let mut left = segments(left);
    let mut right = segments(right);
    loop {
        match (left.next(), right.next()) {
            (None, None) => return true,
            (Some(l), Some(r)) if is_param(l) && is_param(r) => {}
            (Some(l), Some(r)) if l == r && !is_param(l) => {}
            _ => return false,
        }
    }
}

/// Renames the path parameters of an operation described under another route's template.
///
/// `own` and `template` are the path parameters of two patterns at one position, in path
/// order, so the parameter at each position is renamed to the template's name for it. Each
/// parameter is renamed once, so two routes naming two positions the other way around swap
/// their names rather than collapse into one.
pub(super) fn rename_path_parameters(
    parameters: &mut [OpenApiParameter],
    own: &[OpenApiParameter],
    template: &[OpenApiParameter],
) {
    // Patterns of one position carry one parameter per templated segment; anything else is
    // not a rename this can make
    if own.len() != template.len() {
        return;
    }

    for parameter in parameters
        .iter_mut()
        .filter(|parameter| parameter.location == "path")
    {
        if let Some(to) = own
            .iter()
            .zip(template)
            .find_map(|(own, to)| (own.name == parameter.name).then_some(&to.name))
        {
            parameter.name.clone_from(to);
        }
    }
}

fn parse_typed_param_segment(seg: &str) -> Option<(String, Option<OpenApiSchema>)> {
    let inner = seg.strip_prefix('{')?.strip_suffix('}')?.trim();
    let inner = inner.strip_prefix('*').unwrap_or(inner);
    if inner.is_empty() {
        return None;
    }

    let mut it = inner.split(':').map(str::trim);
    let name = it.next()?.trim();
    if name.is_empty() {
        return None;
    }

    let ty = it.next().filter(|s| !s.is_empty());
    let fmt = it.next().filter(|s| !s.is_empty());

    if ty.is_none() {
        return Some((name.to_string(), None));
    }

    let mut schema = match ty.unwrap() {
        "integer" => OpenApiSchema::integer(),
        "number" => OpenApiSchema::number(),
        "boolean" => OpenApiSchema::boolean(),
        "string" => OpenApiSchema::string(),
        _ => return Some((name.to_string(), None)),
    };

    if let Some(fmt) = fmt {
        schema = schema.with_format(fmt);
    }

    Some((name.to_string(), Some(schema)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_openapi_path_extracts_all_valid_segments() {
        let (_, params) = normalize_openapi_path("/teams/{team_id}/users/{user_id}");

        assert_eq!(params.len(), 2);
        assert_eq!(params[0].name, "team_id");
        assert_eq!(params[1].name, "user_id");
        assert!(params.iter().all(|p| p.required));
        assert!(params.iter().all(|p| p.location == "path"));
    }

    #[test]
    fn normalize_openapi_path_skips_invalid_placeholders() {
        let (_, params) = normalize_openapi_path("/users/{}/raw/{broken/id}");
        assert!(params.is_empty());
    }

    #[test]
    fn normalize_openapi_path_supports_typed_placeholders() {
        let (_, params) = normalize_openapi_path("/users/{id:integer}/posts/{published:boolean}");

        assert_eq!(params.len(), 2);
        assert_eq!(params[0].name, "id");
        assert_eq!(params[0].schema.schema_type.as_deref(), Some("integer"));
        assert_eq!(params[1].name, "published");
        assert_eq!(params[1].schema.schema_type.as_deref(), Some("boolean"));
    }

    #[test]
    fn normalize_openapi_path_describes_a_catch_all_as_a_path_parameter() {
        let (path, params) = normalize_openapi_path("/users/{id}/files/{*path}");

        assert_eq!(path, "/users/{id}/files/{path}");
        assert_eq!(params.len(), 2);
        assert_eq!(params[1].name, "path");
        assert!(params[1].required);
        assert_eq!(params[1].schema.schema_type.as_deref(), Some("string"));
    }

    #[test]
    fn normalize_openapi_path_skips_an_unnamed_catch_all() {
        let (_, params) = normalize_openapi_path("/files/{*}");
        assert!(params.is_empty());
    }

    #[test]
    fn is_same_hierarchy_compares_literals_and_parameter_positions() {
        assert!(is_same_hierarchy("/users/{id}", "/users/{name}"));
        assert!(is_same_hierarchy("/users/{id}/posts", "/users/{uid}/posts"));
        assert!(is_same_hierarchy("/", "/"));

        assert!(!is_same_hierarchy("/users/{id}", "/users/me"));
        assert!(!is_same_hierarchy("/users/{id}", "/users/{id}/posts"));
        assert!(!is_same_hierarchy("/users/{id}/posts", "/users/{id}/likes"));
        assert!(!is_same_hierarchy("/users/{id}", "/"));
    }

    #[test]
    fn rename_path_parameters_renames_by_position() {
        let (_, own) = normalize_openapi_path("/pairs/{b:integer}/{a}");
        let (_, template) = normalize_openapi_path("/pairs/{a}/{b}");

        let mut parameters = own.clone();
        parameters.push(OpenApiParameter {
            name: "a".to_string(),
            location: "query".to_string(),
            required: false,
            schema: OpenApiSchema::string(),
        });

        rename_path_parameters(&mut parameters, &own, &template);

        let renamed = parameters
            .iter()
            .map(|p| (p.name.as_str(), p.location.as_str()))
            .collect::<Vec<_>>();

        // Swapped rather than collapsed, and a query parameter is left alone
        assert_eq!(renamed, [("a", "path"), ("b", "path"), ("a", "query")]);
        assert_eq!(parameters[0].schema.schema_type.as_deref(), Some("integer"));
    }

    #[test]
    fn normalize_openapi_path_defaults_unknown_type_to_string() {
        let (_, params) = normalize_openapi_path("/items/{slug}");

        assert_eq!(params.len(), 1);
        assert_eq!(params[0].name, "slug");
        assert_eq!(params[0].schema.schema_type.as_deref(), Some("string"));
    }
}
