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
    response_schema: Option<Option<OpenApiSchema>>,
    request_example: Option<Value>,
    response_example: Option<Option<Value>>,
    request_content_type: Option<String>,
    response_content_type: Option<Option<String>>,
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
        self.response_schema = Some(Some(schema));
        self
    }

    /// Generates path parameters schema.
    pub fn consumes_named_path<T: DeserializeOwned>(self) -> Self {
        self.with_named_path_parameters_from_deserialize::<T>()
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

        self.response_schema = Some(Some(OpenApiSchema::from_example(&example)));
        self.response_example = Some(Some(example));
        self.response_content_type = Some(Some(APPLICATION_JSON.to_string()));

        self
    }

    /// Generates form response schema and example.
    pub fn produces_form_example<T: Serialize>(mut self, example: T) -> Self {
        let example = serde_json::to_value(example).unwrap_or_else(|_| json!({}));

        let encoded = match &example {
            Value::Object(map) => serde_urlencoded::to_string(map).unwrap_or_default(),
            _ => String::new(),
        };

        self.response_schema = Some(Some(OpenApiSchema::string()));
        self.response_example = Some(Some(Value::String(encoded)));

        self.response_content_type = Some(Some(APPLICATION_WWW_FORM_URLENCODED.to_string()));

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
            .get_or_insert_with(|| Some(OpenApiSchema::object()));
        self.response_content_type = Some(Some(APPLICATION_JSON.to_string()));
        self
    }

    /// Generates default form response schema
    fn with_form_response(mut self) -> Self {
        self.response_schema
            .get_or_insert_with(|| Some(OpenApiSchema::object()));
        self.response_content_type = Some(Some(APPLICATION_WWW_FORM_URLENCODED.to_string()));
        self
    }

    /// Generates default text response schema
    fn with_text_response(mut self) -> Self {
        self.response_schema
            .get_or_insert_with(|| Some(OpenApiSchema::string()));
        self.response_content_type = Some(Some(TEXT_PLAIN_UTF_8.to_string()));
        self
    }

    /// Generates empty response schema and content type
    fn with_empty_response(mut self) -> Self {
        self.response_schema = Some(None);
        self.response_content_type = Some(None);
        self.response_example = Some(None);
        self
    }

    /// Generates SSE stream response schema
    fn with_sse_response(mut self) -> Self {
        self.response_schema
            .get_or_insert_with(|| Some(OpenApiSchema::string().with_title("SSE stream")));
        self.response_content_type = Some(Some(TEXT_EVENT_STREAM.to_string()));
        self
    }

    /// Generates default stream response schema
    fn with_stream_response(mut self) -> Self {
        self.response_schema
            .get_or_insert_with(|| Some(OpenApiSchema::binary()));
        self.response_content_type = Some(Some(APPLICATION_OCTET_STREAM.to_string()));
        self
    }

    fn with_query_parameter(mut self, name: String, schema: OpenApiSchema, required: bool) -> Self {
        self.extra_parameters.push(OpenApiParameter {
            name,
            location: "query".to_string(),
            required,
            schema,
        });
        self
    }

    fn with_path_parameter(mut self, name: String, schema: OpenApiSchema) -> Self {
        self.extra_parameters.push(OpenApiParameter {
            name,
            location: "path".to_string(),
            required: true,
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
            && let Some(properties) = schema.properties
        {
            let required = schema.required.unwrap_or_default();
            
            for (name, property_schema) in properties {
                let is_required = required.iter().any(|f| f == &name);
                self = self.with_query_parameter(name, property_schema, is_required);
            }
        }
        self
    }

    fn with_named_path_parameters_from_deserialize<T: DeserializeOwned>(mut self) -> Self {
        if let Some((schema, _)) = schema_and_example_from_deserialize::<T>()
            && let Some(properties) = schema.properties
        {
            for (name, property_schema) in properties {
                self = self.with_path_parameter(name, property_schema);
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

        let touched = self.response_schema.is_some()
            || self.response_example.is_some()
            || self.response_content_type.is_some();

        if touched {
            let schema_clear = matches!(self.response_schema, Some(None));
            let ct_clear = matches!(self.response_content_type, Some(None));
            let example_clear = matches!(self.response_example, Some(None));
        
            if schema_clear || ct_clear || example_clear {
                operation.clear_response_body();
            } else {
                let mut schema = match &self.response_schema {
                    Some(Some(s)) => s.clone(),
                    _ => OpenApiSchema::object(),
                };
            
                let example: Option<Value> = match &self.response_example {
                    Some(Some(v)) => Some(v.clone()),
                    _ => None,
                };
            
                let content_type: &str = match &self.response_content_type {
                    Some(Some(ct)) => ct.as_str(),
                    _ => APPLICATION_JSON.as_ref(),
                };
            
                schema = intern_schema_if_object_named(schema, schemas);
                operation.set_response_body(schema, example, content_type);
            }
        }

        if !self.extra_parameters.is_empty() {
            let params = operation.parameters.get_or_insert_with(Vec::new);

            for p in &self.extra_parameters {
                if let Some(existing) = params
                    .iter_mut()
                    .find(|x| x.name == p.name && x.location == p.location)
                {
                    *existing = p.clone();
                } else {
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
    use crate::{op::OpenApiOperation, schema::OpenApiSchema};
    use serde::{Deserialize, Serialize};
    use serde_json::json;
    use std::collections::BTreeMap;

    #[derive(Deserialize)]
    struct Payload {
        name: String,
        age: u64,
    }

    #[derive(Serialize, Default)]
    struct ResponsePayload {
        message: String,
    }

    #[derive(Deserialize)]
    struct OptionalQuery {
        required_name: String,
        optional_age: Option<()>,
    }

    #[test]
    fn consumes_query_marks_non_optional_fields_as_required() {
        let cfg = OpenApiRouteConfig::default().consumes_query::<OptionalQuery>();

        let required_name = cfg
            .extra_parameters
            .iter()
            .find(|p| p.name == "required_name")
            .expect("required_name param");
        let optional_age = cfg
            .extra_parameters
            .iter()
            .find(|p| p.name == "optional_age")
            .expect("optional_age param");

        assert!(required_name.required);
        assert!(!optional_age.required);
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

    #[test]
    fn merge_combines_docs_and_preserves_existing_values() {
        let base = OpenApiRouteConfig::default()
            .with_tag("base")
            .with_summary("base summary")
            .with_doc("v1");
        let other = OpenApiRouteConfig::default()
            .with_tag("extra")
            .with_summary("other summary")
            .with_doc("v1")
            .with_doc("admin");

        let merged = base.merge(&other);

        assert_eq!(merged.summary.as_deref(), Some("base summary"));
        assert_eq!(merged.tags.as_slice(), ["base", "extra"]);
        assert_eq!(merged.docs.expect("docs"), vec!["v1", "admin"]);
    }

    #[test]
    fn apply_to_operation_adds_content_and_interns_named_schema() {
        let cfg = OpenApiRouteConfig::default()
            .with_request_schema(
                OpenApiSchema::object()
                    .with_title("CreateUser")
                    .with_property("name", OpenApiSchema::string()),
            )
            .with_response_schema(
                OpenApiSchema::object()
                    .with_title("User")
                    .with_property("id", OpenApiSchema::integer()),
            )
            .produces_json_example(ResponsePayload {
                message: "ok".to_string(),
            })
            .with_summary("create user")
            .with_operation_id("createUser");

        let mut op = OpenApiOperation::default();
        let mut schemas = BTreeMap::new();

        cfg.apply_to_operation(&mut op, &mut schemas);

        let op_json = serde_json::to_value(op).expect("serialize operation");
        assert_eq!(op_json["summary"], "create user");
        assert_eq!(op_json["operationId"], "createUser");
        assert!(
            op_json["requestBody"]["content"]
                .get("application/json")
                .is_some()
        );
        assert_eq!(
            op_json["responses"]["200"]["content"]["application/json"]["example"],
            json!({"message":"ok"})
        );

        assert!(schemas.contains_key("CreateUser"));
    }

    #[test]
    fn produces_no_schema_clears_content() {
        let cfg = OpenApiRouteConfig::default()
            .produces_json::<ResponsePayload>()
            .produces_no_schema();

        let mut op = OpenApiOperation::default();
        cfg.apply_to_operation(&mut op, &mut BTreeMap::new());

        let op_json = serde_json::to_value(op).expect("serialize operation");
        assert!(
            op_json["responses"]["200"]
                .get("content")
                .is_none()
        );
    }

    #[test]
    fn with_tags_extends_existing_tags() {
        let cfg = OpenApiRouteConfig::default()
            .with_tag("base")
            .with_tags(["one", "two"]);

        assert_eq!(cfg.tags, vec!["base", "one", "two"]);
    }

    #[test]
    fn with_docs_initializes_and_extends_docs() {
        let cfg = OpenApiRouteConfig::default()
            .with_docs(["v1", "admin"])
            .with_doc("internal");

        assert_eq!(cfg.docs.expect("docs"), vec!["v1", "admin", "internal"]);
    }

    #[test]
    fn with_description_sets_operation_description() {
        let cfg = OpenApiRouteConfig::default().with_description("desc");
        assert_eq!(cfg.description.as_deref(), Some("desc"));
    }

    #[test]
    fn consumes_methods_set_request_content_type() {
        let query_cfg = OpenApiRouteConfig::default().consumes_query::<Payload>();
        assert_eq!(query_cfg.extra_parameters.len(), 2);

        let json_cfg = OpenApiRouteConfig::default().consumes_json::<Payload>();
        assert_eq!(
            json_cfg.request_content_type.as_deref(),
            Some("application/json")
        );
        assert!(json_cfg.request_schema.is_some());

        let form_cfg = OpenApiRouteConfig::default().consumes_form::<Payload>();
        assert_eq!(
            form_cfg.request_content_type.as_deref(),
            Some("application/x-www-form-urlencoded")
        );
        assert!(form_cfg.request_schema.is_some());

        let multipart_cfg = OpenApiRouteConfig::default().consumes_multipart();
        assert_eq!(
            multipart_cfg.request_content_type.as_deref(),
            Some("multipart/form-data")
        );

        let stream_cfg = OpenApiRouteConfig::default().consumes_stream();
        assert_eq!(
            stream_cfg.request_content_type.as_deref(),
            Some("application/octet-stream")
        );
        assert_eq!(
            stream_cfg.request_schema.expect("schema").format.as_deref(),
            Some("binary")
        );
    }

    #[test]
    fn produces_helpers_set_expected_response_schema_and_content_type() {
        let text_cfg = OpenApiRouteConfig::default().produces_text();
        assert_eq!(
            text_cfg
                .response_content_type
                .as_ref()
                .and_then(|x| x.as_deref()),
            Some("text/plain; charset=utf-8")
        );
        assert_eq!(
            text_cfg
                .response_schema
                .expect("schema")
                .expect("schema")
                .schema_type
                .as_deref(),
            Some("string")
        );

        let empty_json_cfg = OpenApiRouteConfig::default().produces_empty_json();
        assert_eq!(
            empty_json_cfg
                .response_content_type
                .as_ref()
                .and_then(|x| x.as_deref()),
            Some("application/json")
        );
        assert_eq!(
            empty_json_cfg
                .response_schema
                .expect("schema")
                .expect("schema")
                .schema_type
                .as_deref(),
            Some("object")
        );

        let form_cfg = OpenApiRouteConfig::default().produces_form::<ResponsePayload>();
        assert_eq!(
            form_cfg                
                .response_content_type
                .as_ref()
                .and_then(|x| x.as_deref()),
            Some("application/x-www-form-urlencoded")
        );
        assert_eq!(
            form_cfg
                .response_schema
                .expect("schema")
                .expect("schema")
                .schema_type
                .as_deref(),
            Some("string")
        );

        let empty_form_cfg = OpenApiRouteConfig::default().produces_empty_form();
        assert_eq!(
            empty_form_cfg                
                .response_content_type
                .as_ref()
                .and_then(|x| x.as_deref()),
            Some("application/x-www-form-urlencoded")
        );
        assert_eq!(
            empty_form_cfg
                .response_schema
                .expect("schema")
                .expect("schema")
                .schema_type
                .as_deref(),
            Some("object")
        );

        let stream_cfg = OpenApiRouteConfig::default().produces_stream();
        assert_eq!(
            stream_cfg                
                .response_content_type
                .as_ref()
                .and_then(|x| x.as_deref()),
            Some("application/octet-stream")
        );
        assert_eq!(
            stream_cfg
                .response_schema
                .expect("schema")
                .expect("schema")
                .format
                .as_deref(),
            Some("binary")
        );

        let sse_cfg = OpenApiRouteConfig::default().produces_sse();
        assert_eq!(
            sse_cfg                
                .response_content_type
                .as_ref()
                .and_then(|x| x.as_deref()),
            Some("text/event-stream")
        );
        assert_eq!(
            sse_cfg.response_schema
                .expect("schema")
                .expect("schema")
                .title
                .as_deref(),
            Some("SSE stream")
        );
    }

    #[test]
    fn produces_form_example_encodes_object_into_string_example() {
        #[derive(Serialize)]
        struct FormOut {
            first: String,
            second: i32,
        }

        let cfg = OpenApiRouteConfig::default().produces_form_example(FormOut {
            first: "hello".to_string(),
            second: 7,
        });

        assert_eq!(
            cfg.response_content_type
                .as_ref()
                .and_then(|x| x.as_deref()),
            Some("application/x-www-form-urlencoded")
        );
        assert_eq!(
            cfg.response_schema
                .expect("schema")
                .expect("schema")
                .schema_type
                .as_deref(),
            Some("string")
        );
        assert_eq!(
            cfg.response_example,
            Some(Some(serde_json::Value::String(
                "first=hello&second=7".to_string()
            )))
        );
    }
}