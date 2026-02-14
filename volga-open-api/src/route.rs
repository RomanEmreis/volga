//! Types and utils for OpenAPI route config.

use std::collections::BTreeMap;
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{json, Value};

use mime::{
    APPLICATION_JSON,
    APPLICATION_OCTET_STREAM,
    APPLICATION_WWW_FORM_URLENCODED,
    MULTIPART_FORM_DATA,
    TEXT_EVENT_STREAM,
    TEXT_PLAIN_UTF_8
};

use super::{
    schema::{OpenApiSchema, Probe},
    param::OpenApiParameter,
    op::OpenApiOperation,
};

/// Per-route OpenAPI metadata.
#[derive(Clone, Debug, Default)]
pub struct OpenApiRouteConfig {
    tags: Vec<String>,
    docs: Option<Vec<String>>,
    summary: Option<String>,
    description: Option<String>,
    operation_id: Option<String>,
    request_schema: Option<OpenApiSchema>,
    response_schema: Option<OpenApiSchema>,
    request_example: Option<Value>,
    response_example: Option<Value>,
    request_content_type: Option<String>,
    response_content_type: Option<String>,
    extra_parameters: Vec<OpenApiParameter>,
}

impl OpenApiRouteConfig {
    /// Returns a list of docs that this route is assigned to    
    pub(crate) fn docs(&self) -> Option<&[String]> {
        self.docs.as_deref()
    }

    /// Adds a tag to the operation.
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self
    }

    /// Adds multiple tags to the operation.
    pub fn with_tags<I, T>(mut self, tags: I) -> Self
    where
        I: IntoIterator<Item = T>,
        T: Into<String>,
    {
        self.tags.extend(tags.into_iter().map(Into::into));
        self
    }

    /// Binds the operation with a document
    pub fn with_doc(mut self, doc: impl Into<String>) -> Self {
        self.docs.get_or_insert_with(Vec::new).push(doc.into());
        self
    }

    /// Binds the operation with documents
    pub fn with_docs<I, S>(mut self, docs: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.docs
            .get_or_insert_with(Vec::new)
            .extend(docs.into_iter().map(Into::into));
        self
    }

    /// Sets the operation summary.
    pub fn with_summary(mut self, summary: impl Into<String>) -> Self {
        self.summary = Some(summary.into());
        self
    }

    /// Sets the operation description.
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Sets the operation id.
    pub fn with_operation_id(mut self, operation_id: impl Into<String>) -> Self {
        self.operation_id = Some(operation_id.into());
        self
    }

    /// Sets the request body schema for this operation.
    pub fn with_request_schema(mut self, schema: OpenApiSchema) -> Self {
        self.request_schema = Some(schema);
        self
    }

    /// Sets the response schema for this operation.
    pub fn with_response_schema(mut self, schema: OpenApiSchema) -> Self {
        self.response_schema = Some(schema);
        self
    }
    
    /// Generates query parameters schema.
    pub fn consumes_query<T: DeserializeOwned>(self) -> Self {
        self.with_query_parameters_from_deserialize::<T>()
    }

    /// Generates JSON request schema and example.
    pub fn consumes_json<T: DeserializeOwned>(self) -> Self {
        self.with_request_type_from_deserialize::<T>(APPLICATION_JSON.as_ref())
    }

    /// Generates form request schema and example.
    pub fn consumes_form<T: DeserializeOwned>(self) -> Self {
        self.with_request_type_from_deserialize::<T>(APPLICATION_WWW_FORM_URLENCODED.as_ref())
    }

    /// Generates multipart request schema.
    pub fn consumes_multipart(self) -> Self {
        self.with_multipart_request()
    }

    /// Generates stream request schema.
    pub fn consumes_stream(self) -> Self {
        self.with_stream_request()
    }
    
    /// Generate text/plain response schema.
    pub fn produces_text(self) -> Self {
        self.with_text_response()
    }

    /// Generate a response without a schema.
    pub fn produces_no_schema(self) -> Self {
        self.with_empty_response()
    }

    /// Generates JSON response schema and example from `T::default()`.
    pub fn produces_json<T: Serialize + Default>(self) -> Self {
        self.produces_json_example(T::default())
    }

    /// Generates empty JSON response schema.
    pub fn produces_empty_json(self) -> Self {
        self.with_json_response()
    }

    /// Generates form response schema and example from `T::default()`.
    pub fn produces_form<T: Serialize + Default>(self) -> Self {
        self.produces_form_example(T::default())
    }

    /// Generates empty form response schema.
    pub fn produces_empty_form(self) -> Self {
        self.with_form_response()
    }

    /// Generates stream response schema.
    pub fn produces_stream(self) -> Self {
        self.with_stream_response()
    }

    /// Generates SSE stream response schema.
    pub fn produces_sse(self) -> Self {
        self.with_sse_response()
    }

    /// Generates JSON response schema and example.
    pub fn produces_json_example<T: Serialize>(mut self, example: T) -> Self {
        let example = serde_json::to_value(example).unwrap_or_else(|_| json!({}));

        self.response_schema = Some(OpenApiSchema::from_example(&example));
        self.response_example = Some(example);
        self.response_content_type = Some(APPLICATION_JSON.to_string());

        self
    }

    /// Generates form response schema and example.
    pub fn produces_form_example<T: Serialize>(mut self, example: T) -> Self {
        let example = serde_json::to_value(example).unwrap_or_else(|_| json!({}));

        let encoded = match &example {
            Value::Object(map) => serde_urlencoded::to_string(map).unwrap_or_default(),
            _ => String::new(),
        };

        self.response_schema = Some(OpenApiSchema::string());
        self.response_example = Some(Value::String(encoded));

        self.response_content_type = Some(APPLICATION_WWW_FORM_URLENCODED.to_string());

        self
    }

    /// Generates default Multipart request schema
    fn with_multipart_request(mut self) -> Self {
        self.request_schema
            .get_or_insert_with(OpenApiSchema::multipart);
        self.request_content_type = Some(MULTIPART_FORM_DATA.to_string());
        self
    }

    /// Generates default stream request schema
    fn with_stream_request(mut self) -> Self {
        self.request_schema
            .get_or_insert_with(OpenApiSchema::binary);
        self.request_content_type = Some(APPLICATION_OCTET_STREAM.to_string());
        self
    }

    /// Generates default JSON response schema
    fn with_json_response(mut self) -> Self {
        self.response_schema
            .get_or_insert_with(OpenApiSchema::object);
        self.response_content_type = Some(APPLICATION_JSON.to_string());
        self
    }

    /// Generates default form response schema
    fn with_form_response(mut self) -> Self {
        self.response_schema
            .get_or_insert_with(OpenApiSchema::object);
        self.response_content_type = Some(APPLICATION_WWW_FORM_URLENCODED.to_string());
        self
    }

    /// Generates default text response schema
    fn with_text_response(mut self) -> Self {
        self.response_schema
            .get_or_insert_with(OpenApiSchema::string);
        self.response_content_type = Some(TEXT_PLAIN_UTF_8.to_string());
        self
    }

    /// Generates empty response schema and content type
    fn with_empty_response(mut self) -> Self {
        self.response_schema = None;
        self.response_content_type = None;
        self
    }

    /// Generates SSE stream response schema
    fn with_sse_response(mut self) -> Self {
        self.response_schema
            .get_or_insert_with(|| OpenApiSchema::string().with_title("SSE stream"));
        self.response_content_type = Some(TEXT_EVENT_STREAM.to_string());
        self
    }

    /// Generates default stream response schema
    fn with_stream_response(mut self) -> Self {
        self.response_schema
            .get_or_insert_with(OpenApiSchema::binary);
        self.response_content_type = Some(APPLICATION_OCTET_STREAM.to_string());
        self
    }

    fn with_query_parameter(mut self, name: String, schema: OpenApiSchema) -> Self {
        self.extra_parameters.push(OpenApiParameter {
            name,
            location: "query".to_string(),
            required: false,
            schema,
        });
        self
    }

    /// Generates request schema and example from `T`.
    fn with_request_type_from_deserialize<T: DeserializeOwned>(
        mut self,
        content_type: &str,
    ) -> Self {
        if let Some((schema, example)) = schema_and_example_from_deserialize::<T>() {
            self.request_schema = Some(schema.with_title(type_display_name::<T>()));
            self.request_example = Some(example);
        }
        self.request_content_type = Some(content_type.to_string());
        self
    }

    fn with_query_parameters_from_deserialize<T: DeserializeOwned>(mut self) -> Self {
        if let Some((schema, _)) = schema_and_example_from_deserialize::<T>()
            && let Some(properties) = schema.properties {
            for (name, property_schema) in properties {
                self = self.with_query_parameter(name, property_schema);
            }
        }
        self
    }

    /// Merges configurations.
    pub fn merge(mut self, other: &Self) -> Self {
        if self.tags.is_empty() {
            self.tags = other.tags.clone();
        } else {
            self.tags.extend(other.tags.clone());
        }
        if self.summary.is_none() {
            self.summary = other.summary.clone();
        }
        if self.description.is_none() {
            self.description = other.description.clone();
        }
        if self.operation_id.is_none() {
            self.operation_id = other.operation_id.clone();
        }
        if self.request_schema.is_none() {
            self.request_schema = other.request_schema.clone();
        }
        if self.response_schema.is_none() {
            self.response_schema = other.response_schema.clone();
        }
        if self.request_example.is_none() {
            self.request_example = other.request_example.clone();
        }
        if self.response_example.is_none() {
            self.response_example = other.response_example.clone();
        }
        if self.request_content_type.is_none() {
            self.request_content_type = other.request_content_type.clone();
        }
        if self.response_content_type.is_none() {
            self.response_content_type = other.response_content_type.clone();
        }
        if !other.extra_parameters.is_empty() {
            self.extra_parameters.extend(other.extra_parameters.clone());
        }
        match (&mut self.docs, &other.docs) {
            (None, Some(d)) => self.docs = Some(d.clone()),
            (Some(dst), Some(src)) => {
                for s in src {
                    if !dst.iter().any(|x| x == s) {
                        dst.push(s.clone());
                    }
                }
            }
            _ => {}
        }
        self
    }

    pub(super) fn apply_to_operation(
        &self,
        operation: &mut OpenApiOperation,
        schemas: &mut BTreeMap<String, OpenApiSchema>
    ) {
        if let Some(summary) = &self.summary {
            operation.summary = Some(summary.clone());
        }
        if let Some(description) = &self.description {
            operation.description = Some(description.clone());
        }
        if let Some(operation_id) = &self.operation_id {
            operation.operation_id = Some(operation_id.clone());
        }

        if !self.tags.is_empty() {
            operation.tags = Some(self.tags.clone());
        }

        if self.request_schema.is_some() || self.request_example.is_some() {
            let mut schema = self.request_schema.clone().unwrap_or_else(OpenApiSchema::object);
            let example = self.request_example.clone();
            let content_type = self.request_content_type.as_deref().unwrap_or(APPLICATION_JSON.as_ref());

            schema = intern_schema_if_object_named(schema, schemas);
            operation.set_request_body(schema, example, content_type);
        }

        if self.response_schema.is_some() || self.response_example.is_some() {
            let mut schema = self.response_schema.clone().unwrap_or_else(OpenApiSchema::object);
            let example = self.response_example.clone();
            let content_type = self.response_content_type.as_deref().unwrap_or(APPLICATION_JSON.as_ref());

            schema = intern_schema_if_object_named(schema, schemas);
            operation.set_response_body(schema, example, content_type);
        }

        if !self.extra_parameters.is_empty() {
            let params = operation.parameters.get_or_insert_with(Vec::new);

            for p in &self.extra_parameters {
                let exists = params
                    .iter()
                    .any(|x| x.name == p.name && x.location == p.location);
                if !exists {
                    params.push(p.clone());
                }
            }
        }
    }
}

fn intern_schema_if_object_named(
    mut schema: OpenApiSchema,
    schemas: &mut BTreeMap<String, OpenApiSchema>
) -> OpenApiSchema {
    if schema.schema_ref.is_some() {
        return schema;
    }

    let base = match schema.title.clone() {
        Some(t) if !t.is_empty() => t,
        _ => return schema,
    };

    if schema.properties.is_none() {
        return schema;
    }

    let mut name = base.clone();
    if schemas.contains_key(&name) {
        let mut i = 2;
        while schemas.contains_key(&format!("{base}_{i}")) {
            i += 1;
        }
        name = format!("{base}_{i}");
    }

    schema.title = None;

    schemas
        .entry(name.clone())
        .or_insert(schema);

    OpenApiSchema::reference(&name)
}

fn schema_and_example_from_deserialize<T: DeserializeOwned>() -> Option<(OpenApiSchema, Value)> {
    let mut probe = Probe::new();
    let _ = T::deserialize(&mut probe);
    probe.finish()
}

fn type_display_name<T>() -> String {
    std::any::type_name::<T>()
        .rsplit("::")
        .next()
        .unwrap_or("Model")
        .to_string()
}

#[cfg(test)]
#[allow(unused)]
mod tests {
    use super::OpenApiRouteConfig;
    use serde::Deserialize;

    #[derive(Deserialize)]
    struct Payload {
        name: String,
        age: u64,
    }

    #[test]
    fn infer_request_schema_from_deserialize_type() {
        let cfg = OpenApiRouteConfig::default()
            .with_request_type_from_deserialize::<Payload>("application/json");

        let schema = cfg
            .request_schema
            .expect("request schema should be inferred");
        let props = schema.properties.expect("object properties should exist");

        assert!(props.contains_key("name"));
        assert!(props.contains_key("age"));
        assert_eq!(
            cfg.request_content_type.as_deref(),
            Some("application/json")
        );
    }

    #[test]
    fn infer_query_parameters_from_deserialize_type() {
        let cfg = OpenApiRouteConfig::default().with_query_parameters_from_deserialize::<Payload>();

        assert_eq!(cfg.extra_parameters.len(), 2);
        assert!(cfg.extra_parameters.iter().any(|p| p.name == "name"));
        assert!(cfg.extra_parameters.iter().any(|p| p.name == "age"));
        assert!(cfg.extra_parameters.iter().all(|p| p.location == "query"));
    }
}