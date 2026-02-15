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

pub(super) fn parse_path_parameters(path: &str) -> Vec<OpenApiParameter> {
    path.split('/')
        .filter_map(|segment| {
            let parameter = segment.strip_prefix('{')?.strip_suffix('}')?;
            if parameter.is_empty() {
                return None;
            }

            Some(OpenApiParameter {
                name: parameter.to_string(),
                location: "path".to_string(),
                required: true,
                schema: OpenApiSchema::string(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::parse_path_parameters;

    #[test]
    fn parse_path_parameters_extracts_all_valid_segments() {
        let params = parse_path_parameters("/teams/{team_id}/users/{user_id}");

        assert_eq!(params.len(), 2);
        assert_eq!(params[0].name, "team_id");
        assert_eq!(params[1].name, "user_id");
        assert!(params.iter().all(|p| p.required));
        assert!(params.iter().all(|p| p.location == "path"));
    }

    #[test]
    fn parse_path_parameters_skips_invalid_placeholders() {
        let params = parse_path_parameters("/users/{}/raw/{broken/id}");
        assert!(params.is_empty());
    }
}
