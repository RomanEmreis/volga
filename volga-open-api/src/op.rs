//! Types and utils for OpenAPI operations.

use std::collections::BTreeMap;
use mime::APPLICATION_JSON;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::{
    param::{parse_path_parameters, OpenApiParameter},
    schema::OpenApiSchema,
};

/// Represents OpenAPI operation.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct OpenApiOperation {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) operation_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) tags: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) parameters: Option<Vec<OpenApiParameter>>,
    #[serde(rename = "requestBody", skip_serializing_if = "Option::is_none")]
    request_body: Option<OpenApiRequestBody>,
    responses: BTreeMap<String, OpenApiResponse>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct OpenApiMediaType {
    schema: OpenApiSchema,
    #[serde(skip_serializing_if = "Option::is_none")]
    example: Option<Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct OpenApiResponse {
    description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<BTreeMap<String, OpenApiMediaType>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct OpenApiRequestBody {
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    content: BTreeMap<String, OpenApiMediaType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    required: Option<bool>,
}

impl Default for OpenApiOperation {
    fn default() -> Self {
        let mut responses = BTreeMap::new();
        responses.insert(
            "200".to_string(),
            OpenApiResponse {
                description: "OK".to_string(),
                content: None,
            },
        );
        Self {
            summary: None,
            description: None,
            operation_id: None,
            tags: None,
            parameters: None,
            request_body: None,
            responses,
        }
    }
}

impl OpenApiRequestBody {
    fn json_payload() -> Self {
        Self {
            description: None,
            content: default_json_content(),
            required: Some(true),
        }
    }
}

impl OpenApiOperation {
    pub(super) fn for_method(_method: String, path: &str) -> Self {
        let mut operation = Self::default();

        let parameters = parse_path_parameters(path);
        if !parameters.is_empty() {
            operation.parameters = Some(parameters);
        }

        operation
    }

    pub(super) fn set_request_body(
        &mut self,
        schema: OpenApiSchema,
        example: Option<Value>,
        content_type: &str,
    ) {
        let request_body = self
            .request_body
            .get_or_insert_with(OpenApiRequestBody::json_payload);
        request_body.content = media_content(content_type, schema, example);
    }

    pub(super) fn set_response_body(
        &mut self,
        schema: OpenApiSchema,
        example: Option<Value>,
        content_type: &str,
    ) {
        let response = self
            .responses
            .entry("200".to_string())
            .or_insert_with(|| OpenApiResponse {
                description: "OK".to_string(),
                content: None,
            });
        response.content = Some(media_content(content_type, schema, example));
    }
}

fn media_content(
    content_type: &str,
    schema: OpenApiSchema,
    example: Option<Value>,
) -> BTreeMap<String, OpenApiMediaType> {
    let mut content = BTreeMap::new();
    content.insert(
        content_type.to_string(),
        OpenApiMediaType { schema, example },
    );
    content
}

fn default_json_content() -> BTreeMap<String, OpenApiMediaType> {
    media_content(
        APPLICATION_JSON.as_ref(),
        OpenApiSchema::object(),
        Some(default_example()),
    )
}

fn default_example() -> Value {
    json!({})
}

#[cfg(test)]
mod tests {
    use super::OpenApiOperation;
    use crate::schema::OpenApiSchema;
    use serde_json::{Value, json};

    #[test]
    fn for_method_does_not_prepopulate_request_body() {
        let post = OpenApiOperation::for_method("post".to_string(), "/users/{id}");
        let get = OpenApiOperation::for_method("get".to_string(), "/users/{id}");

        let post_json = serde_json::to_value(post).expect("serialize");
        let get_json = serde_json::to_value(get).expect("serialize");

        assert!(post_json.get("requestBody").is_none());
        assert!(get_json.get("requestBody").is_none());

        let parameters = get_json["parameters"].as_array().expect("parameters array");
        assert_eq!(parameters.len(), 1);
        assert_eq!(parameters[0]["name"], "id");
        assert_eq!(parameters[0]["in"], "path");
    }

    #[test]
    fn set_request_and_response_body_use_provided_content_type() {
        let mut operation = OpenApiOperation::default();
        operation.set_request_body(
            OpenApiSchema::string(),
            Some(json!("example")),
            "text/plain",
        );
        operation.set_response_body(
            OpenApiSchema::integer(),
            Some(json!(42)),
            "application/custom",
        );

        let json = serde_json::to_value(operation).expect("serialize");

        assert!(json["requestBody"]["content"].get("text/plain").is_some());
        assert_eq!(json["requestBody"]["required"], Value::Bool(true));
        assert_eq!(
            json["requestBody"]["content"]["text/plain"]["example"],
            Value::String("example".to_string())
        );

        assert!(
            json["responses"]["200"]["content"]
                .get("application/custom")
                .is_some()
        );
        assert_eq!(
            json["responses"]["200"]["content"]["application/custom"]["example"],
            json!(42)
        );
    }
}
