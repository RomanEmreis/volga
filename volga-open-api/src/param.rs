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