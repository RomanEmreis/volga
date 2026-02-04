//! OpenAPI registry and configuration.

use std::{collections::BTreeMap, sync::{Arc, Mutex}};
use serde::Serialize;
use crate::{App, http::Method, headers::ContentType};
use serde_json::{json, Value};

const DEFAULT_OPENAPI_VERSION: &str = "3.0.0";
const DEFAULT_SPEC_PATH: &str = "/openapi.json";
const DEFAULT_UI_PATH: &str = "/openapi";
const DEFAULT_UI_TITLE: &str = "OpenAPI UI";

/// OpenAPI runtime configuration.
#[derive(Clone, Debug)]
pub struct OpenApiConfig {
    title: String,
    version: String,
    description: Option<String>,
    spec_path: String,
    ui_enabled: bool,
    ui_path: String,
}

impl Default for OpenApiConfig {
    fn default() -> Self {
        Self {
            title: "Volga API".to_string(),
            version: "0.1.0".to_string(),
            description: None,
            spec_path: DEFAULT_SPEC_PATH.to_string(),
            ui_enabled: false,
            ui_path: DEFAULT_UI_PATH.to_string(),
        }
    }
}

impl OpenApiConfig {
    /// Creates a new [`OpenApiConfig`] with defaults.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the OpenAPI document title.
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }

    /// Sets the OpenAPI document version.
    pub fn with_version(mut self, version: impl Into<String>) -> Self {
        self.version = version.into();
        self
    }

    /// Sets the OpenAPI document description.
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Sets the path where the OpenAPI JSON document is served.
    pub fn with_spec_path(mut self, path: impl Into<String>) -> Self {
        self.spec_path = path.into();
        self
    }

    /// Enables or disables the OpenAPI UI.
    pub fn with_ui(mut self, enabled: bool) -> Self {
        self.ui_enabled = enabled;
        self
    }

    /// Sets the path where the OpenAPI UI is served.
    pub fn with_ui_path(mut self, path: impl Into<String>) -> Self {
        self.ui_path = path.into();
        self
    }

    pub(crate) fn spec_path(&self) -> &str {
        &self.spec_path
    }

    pub(crate) fn ui_enabled(&self) -> bool {
        self.ui_enabled
    }

    pub(crate) fn ui_path(&self) -> &str {
        &self.ui_path
    }
}

/// Per-route OpenAPI metadata.
#[derive(Clone, Debug, Default)]
pub struct OpenApiRouteConfig {
    tags: Vec<String>,
    summary: Option<String>,
    description: Option<String>,
    operation_id: Option<String>,
    request_schema: Option<OpenApiSchema>,
    response_schema: Option<OpenApiSchema>,
    request_example: Option<Value>,
    response_example: Option<Value>,
}

impl OpenApiRouteConfig {
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

    /// Sets an example request body.
    pub fn with_request_example(mut self, example: Value) -> Self {
        self.request_example = Some(example);
        self
    }

    /// Sets an example request body and infers the schema from it.
    pub fn with_request_example_auto_schema(mut self, example: Value) -> Self {
        self.request_schema = Some(OpenApiSchema::from_example(&example));
        self.request_example = Some(example);
        self
    }

    /// Sets an example response body.
    pub fn with_response_example(mut self, example: Value) -> Self {
        self.response_example = Some(example);
        self
    }

    /// Sets an example response body and infers the schema from it.
    pub fn with_response_example_auto_schema(mut self, example: Value) -> Self {
        self.response_schema = Some(OpenApiSchema::from_example(&example));
        self.response_example = Some(example);
        self
    }

    pub(crate) fn merge(mut self, other: &Self) -> Self {
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
        self
    }

    fn apply_to(&self, operation: &mut OpenApiOperation) {
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
            operation.tags.get_or_insert_with(Vec::new).extend(self.tags.clone());
        }
        if self.request_schema.is_some() || self.request_example.is_some() {
            let schema = self
                .request_schema
                .clone()
                .unwrap_or_else(OpenApiSchema::object);
            let example = self.request_example.clone();
            operation.set_request_body(schema, example);
        }
        if self.response_schema.is_some() || self.response_example.is_some() {
            let schema = self
                .response_schema
                .clone()
                .unwrap_or_else(OpenApiSchema::object);
            let example = self.response_example.clone();
            operation.set_response_body(schema, example);
        }
    }
}

/// OpenAPI runtime registry.
#[derive(Clone, Debug)]
pub struct OpenApiRegistry {
    inner: Arc<Mutex<OpenApiDocument>>,
    spec_path: String,
    ui_path: String,
    ui_enabled: bool,
}

impl OpenApiRegistry {
    pub(crate) fn new(config: OpenApiConfig) -> Self {
        let doc = OpenApiDocument {
            openapi: DEFAULT_OPENAPI_VERSION.to_string(),
            info: OpenApiInfo {
                title: config.title,
                version: config.version,
                description: config.description,
            },
            paths: BTreeMap::new(),
        };
        Self {
            inner: Arc::new(Mutex::new(doc)),
            spec_path: config.spec_path,
            ui_path: config.ui_path,
            ui_enabled: config.ui_enabled,
        }
    }

    pub(crate) fn register_route(&self, method: &Method, path: &str) {
        if self.is_excluded_path(path) {
            return;
        }
        let mut doc = self.lock();
        let method = method.as_str().to_ascii_lowercase();
        let entry = doc.paths.entry(path.to_string()).or_default();
        entry
            .entry(method.clone())
            .or_insert_with(|| OpenApiOperation::for_method(method));
    }

    pub(crate) fn apply_route_config(
        &self,
        method: &Method,
        path: &str,
        config: &OpenApiRouteConfig,
    ) {
        if self.is_excluded_path(path) {
            return;
        }
        let mut doc = self.lock();
        let method = method.as_str().to_ascii_lowercase();
        let entry = doc.paths.entry(path.to_string()).or_default();
        let operation = entry
            .entry(method.clone())
            .or_insert_with(|| OpenApiOperation::for_method(method));
        config.apply_to(operation);
    }

    pub(crate) fn document(&self) -> OpenApiDocument {
        self.lock().clone()
    }

    fn is_excluded_path(&self, path: &str) -> bool {
        path == self.spec_path || (self.ui_enabled && path == self.ui_path)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, OpenApiDocument> {
        self.inner.lock().expect("openapi registry lock poisoned")
    }
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct OpenApiDocument {
    openapi: String,
    info: OpenApiInfo,
    paths: BTreeMap<String, BTreeMap<String, OpenApiOperation>>,
}

#[derive(Clone, Debug, Serialize)]
struct OpenApiInfo {
    title: String,
    version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
struct OpenApiOperation {
    #[serde(skip_serializing_if = "Option::is_none")]
    summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    operation_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tags: Option<Vec<String>>,
    #[serde(rename = "requestBody", skip_serializing_if = "Option::is_none")]
    request_body: Option<OpenApiRequestBody>,
    responses: BTreeMap<String, OpenApiResponse>,
}

impl Default for OpenApiOperation {
    fn default() -> Self {
        let mut responses = BTreeMap::new();
        responses.insert(
            "200".to_string(),
            OpenApiResponse {
                description: "OK".to_string(),
                content: Some(default_json_content()),
            },
        );
        Self {
            summary: None,
            description: None,
            operation_id: None,
            tags: None,
            request_body: None,
            responses,
        }
    }
}

impl OpenApiOperation {
    fn for_method(method: String) -> Self {
        let mut operation = Self::default();
        if method_supports_body(&method) {
            operation.request_body = Some(OpenApiRequestBody::json_payload());
        }
        operation
    }

    fn set_request_body(&mut self, schema: OpenApiSchema, example: Option<Value>) {
        let request_body = self
            .request_body
            .get_or_insert_with(OpenApiRequestBody::json_payload);
        request_body.content = json_content(schema, example);
    }

    fn set_response_body(&mut self, schema: OpenApiSchema, example: Option<Value>) {
        let response = self
            .responses
            .entry("200".to_string())
            .or_insert_with(|| OpenApiResponse {
                description: "OK".to_string(),
                content: None,
            });
        response.content = Some(json_content(schema, example));
    }
}

#[derive(Clone, Debug, Serialize)]
struct OpenApiResponse {
    description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<BTreeMap<String, OpenApiMediaType>>,
}

#[derive(Clone, Debug, Serialize)]
struct OpenApiRequestBody {
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    content: BTreeMap<String, OpenApiMediaType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    required: Option<bool>,
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

#[derive(Clone, Debug, Serialize)]
struct OpenApiMediaType {
    schema: OpenApiSchema,
    #[serde(skip_serializing_if = "Option::is_none")]
    example: Option<Value>,
}

/// Represents Open API schema
#[derive(Clone, Debug, Serialize)]
pub struct OpenApiSchema {
    #[serde(rename = "type")]
    schema_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    properties: Option<BTreeMap<String, OpenApiSchema>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    required: Option<Vec<String>>,
    #[serde(rename = "additionalProperties", skip_serializing_if = "Option::is_none")]
    additional_properties: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    items: Option<Box<OpenApiSchema>>,
}

impl OpenApiSchema {
    /// Creates an object schema
    pub fn object() -> Self {
        Self {
            schema_type: "object".to_string(),
            properties: None,
            required: None,
            additional_properties: Some(true),
            items: None,
        }
    }

    /// Creates a string schema
    pub fn string() -> Self {
        Self {
            schema_type: "string".to_string(),
            properties: None,
            required: None,
            additional_properties: None,
            items: None,
        }
    }

    /// Creates an integer schema
    pub fn integer() -> Self {
        Self {
            schema_type: "integer".to_string(),
            properties: None,
            required: None,
            additional_properties: None,
            items: None,
        }
    }

    /// Creates a number schema
    pub fn number() -> Self {
        Self {
            schema_type: "number".to_string(),
            properties: None,
            required: None,
            additional_properties: None,
            items: None,
        }
    }

    /// Creates a boolean schema
    pub fn boolean() -> Self {
        Self {
            schema_type: "boolean".to_string(),
            properties: None,
            required: None,
            additional_properties: None,
            items: None,
        }
    }

    /// Creates an array schema
    pub fn array(items: OpenApiSchema) -> Self {
        Self {
            schema_type: "array".to_string(),
            properties: None,
            required: None,
            additional_properties: None,
            items: Some(Box::new(items)),
        }
    }

    /// Creates a schema from [`serde_json::Value`]
    pub fn from_example(example: &Value) -> Self {
        match example {
            Value::Null => OpenApiSchema::object(),
            Value::Bool(_) => OpenApiSchema::boolean(),
            Value::Number(number) => {
                if number.is_i64() || number.is_u64() {
                    OpenApiSchema::integer()
                } else {
                    OpenApiSchema::number()
                }
            }
            Value::String(_) => OpenApiSchema::string(),
            Value::Array(items) => {
                let item_schema = items
                    .first()
                    .map(OpenApiSchema::from_example)
                    .unwrap_or_else(OpenApiSchema::object);
                OpenApiSchema::array(item_schema)
            }
            Value::Object(map) => {
                let mut schema = OpenApiSchema::object();
                let mut required = Vec::new();
                for (key, value) in map {
                    schema = schema.with_property(key.clone(), OpenApiSchema::from_example(value));
                    required.push(key.clone());
                }
                schema.with_required(required)
            }
        }
    }

    /// Sets the property schema
    pub fn with_property(mut self, name: impl Into<String>, schema: OpenApiSchema) -> Self {
        self.properties
            .get_or_insert_with(BTreeMap::new)
            .insert(name.into(), schema);
        self
    }

    /// Sets required properties
    pub fn with_required<I, T>(mut self, required: I) -> Self
    where
        I: IntoIterator<Item = T>,
        T: Into<String>,
    {
        self.required = Some(required.into_iter().map(Into::into).collect());
        self
    }
}

impl Default for OpenApiSchema {
    fn default() -> Self {
        Self::object()
    }
}

fn default_example() -> Value {
    json!({})
}

fn json_content(schema: OpenApiSchema, example: Option<Value>) -> BTreeMap<String, OpenApiMediaType> {
    let mut content = BTreeMap::new();
    content.insert(
        "application/json".to_string(),
        OpenApiMediaType {
            schema,
            example,
        },
    );
    content
}

fn default_json_content() -> BTreeMap<String, OpenApiMediaType> {
    json_content(OpenApiSchema::object(), Some(default_example()))
}

fn method_supports_body(method: &str) -> bool {
    matches!(method, "post" | "put" | "patch")
}

pub(crate) fn swagger_ui_html(spec_path: &str) -> String {
    format!(
        r##"<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>{DEFAULT_UI_TITLE}</title>
    <link rel="stylesheet" href="https://unpkg.com/swagger-ui-dist@5/swagger-ui.css" />
  </head>
  <body>
    <div id="swagger-ui"></div>
    <script src="https://unpkg.com/swagger-ui-dist@5/swagger-ui-bundle.js"></script>
    <script>
      window.onload = function() {{
        SwaggerUIBundle({{
          url: "{spec_path}",
          dom_id: "#swagger-ui",
        }});
      }};
    </script>
  </body>
</html>"##
    )
}

impl App {
    /// Configures OpenAPI registry with custom settings.
    ///
    /// # Example
    /// ```no_run
    /// use volga::App;
    ///
    /// let app = App::new()
    ///     .with_open_api(|config| config
    ///         .with_title("Example API")
    ///         .with_version("1.0.0"));
    /// ```
    pub fn with_open_api<T>(mut self, config: T) -> Self
    where
        T: FnOnce(OpenApiConfig) -> OpenApiConfig,
    {
        let config = config(self.openapi_config.unwrap_or_default());
        let registry = OpenApiRegistry::new(config.clone());

        self.openapi_config = Some(config);
        self.openapi = Some(registry);
        self
    }

    /// Sets OpenAPI registry with the provided configuration.
    pub fn set_open_api(mut self, config: OpenApiConfig) -> Self {
        self.openapi = Some(OpenApiRegistry::new(config.clone()));
        self.openapi_config = Some(config);
        self
    }

    /// Registers the OpenAPI JSON endpoint.
    pub fn use_open_api(&mut self) -> &mut Self {
        if self.openapi.is_none() {
            let config = self.openapi_config.clone().unwrap_or_default();
            self.openapi = Some(OpenApiRegistry::new(config.clone()));
            self.openapi_config = Some(config);
        }

        let Some(registry) = self.openapi.clone() else {
            return self;
        };

        let spec_path = self
            .openapi_config
            .as_ref()
            .map(|config| config.spec_path().to_string())
            .unwrap_or_else(|| OpenApiConfig::default().spec_path().to_string());

        self.map_get(&spec_path, move || {
            let registry = registry.clone();
            async move { crate::Json(registry.document()) }
        });

        if self
            .openapi_config
            .as_ref()
            .is_some_and(|config| config.ui_enabled())
        {
            let ui_path = self
                .openapi_config
                .as_ref()
                .map(|config| config.ui_path().to_string())
                .unwrap_or_else(|| OpenApiConfig::default().ui_path().to_string());

            self.map_get(&ui_path, move || {
                let spec_path = spec_path.clone();
                async move {
                    crate::HttpResponse::builder()
                        .header(ContentType::html_utf_8())
                        .body(swagger_ui_html(&spec_path).into())
                }
            });
        }

        self
    }
}

#[cfg(test)]
mod tests {

}