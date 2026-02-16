//! Type and utils for OpenAPI parameters.

use serde::{Deserialize, Serialize};
use crate::schema::OpenApiSchema;

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
pub(super) fn normalize_openapi_path(path: &str) -> (String, Vec<OpenApiParameter>) {
    let mut params = Vec::new();
    let mut out = String::with_capacity(path.len());

    if path.starts_with('/') {
        out.push('/');
    }

    for seg in path.split('/').filter(|s| !s.is_empty()) {
        if !out.ends_with('/') {
            out.push('/');
        }

        if let Some((name, schema_opt)) = parse_typed_param_segment(seg) {
            // в paths ключе только {name}
            out.push('{');
            out.push_str(&name);
            out.push('}');

            // schema: override если задан, иначе string
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

fn parse_typed_param_segment(seg: &str) -> Option<(String, Option<OpenApiSchema>)> {
    let inner = seg.strip_prefix('{')?.strip_suffix('}')?.trim();
    if inner.is_empty() { return None; }

    let mut it = inner.split(':').map(str::trim);
    let name = it.next()?.trim();
    if name.is_empty() { return None; }

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
    fn normalize_openapi_path_defaults_unknown_type_to_string() {
        let (_, params) = normalize_openapi_path("/items/{slug}");

        assert_eq!(params.len(), 1);
        assert_eq!(params[0].name, "slug");
        assert_eq!(params[0].schema.schema_type.as_deref(), Some("string"));
    }
}
