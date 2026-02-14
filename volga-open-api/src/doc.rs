//! Types and utils for OpenAPI documents.

use std::collections::BTreeMap;
use serde::{Deserialize, Serialize};
use super::{op::OpenApiOperation, schema::OpenApiSchema};

/// Represents OpenAPI document.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OpenApiDocument {
    pub(super) openapi: String,
    pub(super) info: OpenApiInfo,
    pub(super) components: OpenApiComponents,
    pub(super) paths: BTreeMap<String, BTreeMap<String, OpenApiOperation>>,
}

/// Represents OpenAPI info.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct OpenApiInfo {
    pub(super) title: String,
    pub(super) version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) description: Option<String>,
}

/// Represents OpenAPI components.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct OpenApiComponents {
    pub(super) schemas: BTreeMap<String, OpenApiSchema>,
}