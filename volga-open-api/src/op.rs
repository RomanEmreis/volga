//! Types and utils for OpenAPI operations.

use std::collections::{BTreeMap, BTreeSet};
use mime::APPLICATION_JSON;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::{
    param::OpenApiParameter,
    schema::OpenApiSchema,
};

/// Represents OpenAPI operation.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct OpenApiOperation {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) description: Option<String>,
    #[serde(rename = "operationId", skip_serializing_if = "Option::is_none")]
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
        status: u16,
        schema: OpenApiSchema,
        example: Option<Value>,
        content_type: &str,
    ) {
        let description = http::StatusCode::from_u16(status)
            .ok()
            .and_then(|s| s.canonical_reason())
            .unwrap_or("Response")
            .to_string();
        let response = self
            .responses
            .entry(status.to_string())
            .or_insert_with(|| OpenApiResponse { description, content: None });
        response.content = Some(media_content(content_type, schema, example));
    }

    pub(super) fn clear_response_body(&mut self, status: u16) {
        let description = http::StatusCode::from_u16(status)
            .ok()
            .and_then(|s| s.canonical_reason())
            .unwrap_or("Response")
            .to_string();
        let response = self
            .responses
            .entry(status.to_string())
            .or_insert_with(|| OpenApiResponse { description, content: None });
        response.content = None;
    }

    pub(super) fn clear_all_responses(&mut self) {
        self.responses.clear();
    }

    pub(super) fn collect_component_refs(&self, out: &mut BTreeSet<String>) {
        if let Some(params) = &self.parameters {
            for p in params {
                p.schema.collect_component_refs(out);
            }
        }

        if let Some(body) = &self.request_body {
            for media in body.content.values() {
                media.schema.collect_component_refs(out);
            }
        }

        for resp in self.responses.values() {
            let Some(content) = &resp.content else {
                continue;
            };

            for media in content.values() {
                media.schema.collect_component_refs(out);
            }
        }
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
    use crate::{param::OpenApiParameter, schema::OpenApiSchema};
    use serde_json::{Value, json};
    use std::collections::BTreeSet;

    #[test]
    fn set_request_and_response_body_use_provided_content_type() {
        let mut operation = OpenApiOperation::default();
        operation.set_request_body(
            OpenApiSchema::string(),
            Some(json!("example")),
            "text/plain",
        );
        operation.set_response_body(
            200,
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

    #[test]
    fn set_response_body_non_200_status() {
        let mut operation = OpenApiOperation::default();
        operation.clear_all_responses();
        operation.set_response_body(
            201,
            OpenApiSchema::object(),
            None,
            "application/json",
        );

        let json = serde_json::to_value(operation).expect("serialize");
        assert!(json["responses"].get("200").is_none());
        assert!(json["responses"]["201"]["content"].get("application/json").is_some());
        assert_eq!(json["responses"]["201"]["description"], "Created");
    }

    #[test]
    fn clear_response_body_removes_content_for_status() {
        let mut operation = OpenApiOperation::default();
        operation.set_response_body(200, OpenApiSchema::string(), None, "text/plain");
        operation.clear_response_body(200);

        let json = serde_json::to_value(operation).expect("serialize");
        assert!(json["responses"]["200"].get("content").is_none());
    }

    #[test]
    fn clear_all_responses_empties_map() {
        let mut operation = OpenApiOperation::default();
        operation.clear_all_responses();

        let json = serde_json::to_value(operation).expect("serialize");
        assert!(json["responses"].as_object().expect("responses object").is_empty());
    }

    #[test]
    fn collect_component_refs_includes_params_request_and_response() {
        let mut operation = OpenApiOperation {
            parameters: Some(vec![OpenApiParameter {
                name: "id".to_string(),
                location: "path".to_string(),
                required: true,
                schema: OpenApiSchema::reference("PathParam"),
            }]),
            ..Default::default()
        };

        operation.set_request_body(
            OpenApiSchema::reference("CreateUser"),
            None,
            "application/json",
        );
        operation.set_response_body(
            200,
            OpenApiSchema::reference("User"),
            None,
            "application/json",
        );

        let mut refs = BTreeSet::new();
        operation.collect_component_refs(&mut refs);

        assert!(refs.contains("PathParam"));
        assert!(refs.contains("CreateUser"));
        assert!(refs.contains("User"));
        assert_eq!(refs.len(), 3);
    }
}
