//! OpenAPI registry and configuration.

use std::{collections::BTreeMap, sync::{Arc, Mutex}};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{json, Value};
use crate::{App, http::Method, headers::ContentType};
use schema::Probe;
use mime::{
    TEXT_PLAIN_UTF_8, 
    APPLICATION_JSON, 
    APPLICATION_WWW_FORM_URLENCODED,
    MULTIPART_FORM_DATA,
    TEXT_EVENT_STREAM,
    APPLICATION_OCTET_STREAM
};

pub use schema::OpenApiSchema;

mod schema;

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
    /// 
    /// Default: `/openapi.json`
    pub fn with_spec_path(mut self, path: impl Into<String>) -> Self {
        self.spec_path = path.into();
        self
    }

    /// Enables or disables the OpenAPI UI.
    /// 
    /// Default: `false`
    pub fn with_ui(mut self) -> Self {
        self.ui_enabled = true;
        self
    }

    /// Sets the path where the OpenAPI UI is served.
    /// 
    /// Default: `/openapi`
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
    request_content_type: Option<String>,
    response_content_type: Option<String>,
    extra_parameters: Vec<OpenApiParameter>,
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

    /// Generates JSON request schema and example.
    pub fn consumes_json<T: DeserializeOwned>(self) -> Self {
        self.with_request_type_from_deserialize::<T>(APPLICATION_JSON.as_ref())
    }

    /// Generates form request schema and example.
    pub fn consumes_form<T: DeserializeOwned>(self) -> Self {
        self.with_request_type_from_deserialize::<T>(APPLICATION_WWW_FORM_URLENCODED.as_ref())
    }
    
    /// Generates multipart request schema.
    #[cfg(feature = "multipart")]
    pub fn consumes_multipart(self) -> Self {
        self.with_multipart_request()
    }

    /// Generates stream request schema.
    pub fn consumes_stream(self) -> Self {
        self.with_stream_request()
    }

    /// Generates JSON response schema and example from `T::default()`.
    pub fn produces_json<T: Serialize + Default>(self) -> Self {
        self.produces_json_example(T::default())
    }

    /// Generates form response schema and example from `T::default()`.
    pub fn produces_form<T: Serialize + Default>(self) -> Self {
        self.produces_form_example(T::default())
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

    /// Generates default JSON request schema
    #[allow(unused)]
    pub(crate) fn with_json_request(mut self) -> Self {
        self.request_schema
            .get_or_insert_with(OpenApiSchema::object);
        self.request_content_type = Some(APPLICATION_JSON.to_string());
        self
    }

    /// Generates default form request schema
    #[allow(unused)]
    pub(crate) fn with_form_request(mut self) -> Self {
        self.request_schema
            .get_or_insert_with(OpenApiSchema::object);
        self.request_content_type = Some(APPLICATION_WWW_FORM_URLENCODED.to_string());
        self
    }

    /// Generates default Multipart request schema
    #[cfg(feature = "multipart")]
    pub(crate) fn with_multipart_request(mut self) -> Self {
        self.request_schema
            .get_or_insert_with(OpenApiSchema::multipart);
        self.request_content_type = Some(MULTIPART_FORM_DATA.to_string());
        self
    }

    /// Generates default stream request schema
    pub(crate) fn with_stream_request(mut self) -> Self {
        self.request_schema
            .get_or_insert_with(OpenApiSchema::binary);
        self.request_content_type = Some(APPLICATION_OCTET_STREAM.to_string());
        self
    }

    /// Generates default JSON response schema
    pub(crate) fn with_json_response(mut self) -> Self {
        self.response_schema
            .get_or_insert_with(OpenApiSchema::object);
        self.response_content_type = Some(APPLICATION_JSON.to_string());
        self
    }

    /// Generates default form response schema
    pub(crate) fn with_form_response(mut self) -> Self {
        self.response_schema
            .get_or_insert_with(OpenApiSchema::object);
        self.response_content_type = Some(APPLICATION_WWW_FORM_URLENCODED.to_string());
        self
    }

    /// Generates default text response schema
    pub(crate) fn with_text_response(mut self) -> Self {
        self.response_schema
            .get_or_insert_with(OpenApiSchema::string);
        self.response_content_type = Some(TEXT_PLAIN_UTF_8.to_string());
        self
    }

    /// Generates SSE stream response schema
    pub(crate) fn with_sse_response(mut self) -> Self {
        self.response_schema
            .get_or_insert_with(|| OpenApiSchema::string().with_title("SSE stream"));
        self.response_content_type = Some(TEXT_EVENT_STREAM.to_string());
        self
    }

    /// Generates default stream response schema
    pub(crate) fn with_stream_response(mut self) -> Self {
        self.response_schema
            .get_or_insert_with(OpenApiSchema::binary);
        self.response_content_type = Some(APPLICATION_OCTET_STREAM.to_string());
        self
    }

    pub(crate) fn with_empty_response(mut self) -> Self {
        self.response_schema = None;
        self.response_content_type = None;
        self
    }

    pub(crate) fn with_query_parameter(mut self, name: String, schema: OpenApiSchema) -> Self {
        self.extra_parameters.push(OpenApiParameter {
            name,
            location: "query".to_string(),
            required: false,
            schema,
        });
        self
    }

    pub(crate) fn with_request_type_from_deserialize<T: DeserializeOwned>(
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

    pub(crate) fn with_query_parameters_from_deserialize<T: DeserializeOwned>(mut self) -> Self {
        if let Some((schema, _)) = schema_and_example_from_deserialize::<T>() 
            && let Some(properties) = schema.properties {
            for (name, property_schema) in properties {
                self = self.with_query_parameter(name, property_schema);
            }
        }
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
        if self.request_content_type.is_none() {
            self.request_content_type = other.request_content_type.clone();
        }
        if self.response_content_type.is_none() {
            self.response_content_type = other.response_content_type.clone();
        }
        if !other.extra_parameters.is_empty() {
            self.extra_parameters.extend(other.extra_parameters.clone());
        }
        self
    }

    fn apply_to_with_doc(
        &self,
        operation: &mut OpenApiOperation,
        schemas: &mut BTreeMap<String, OpenApiSchema>
    ) {
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
            components: OpenApiComponents {
                schemas: BTreeMap::new(),
            },
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
            .or_insert_with(|| OpenApiOperation::for_method(method, path));
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

        let OpenApiDocument { paths, components, .. } = &mut *doc;
        
        let method = method.as_str().to_ascii_lowercase();
        let entry = paths.entry(path.to_string()).or_default();
        
        let operation = entry
            .entry(method.clone())
            .or_insert_with(|| OpenApiOperation::for_method(method, path));

        config.apply_to_with_doc(operation, &mut components.schemas);
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

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct OpenApiDocument {
    openapi: String,
    info: OpenApiInfo,
    components: OpenApiComponents,
    paths: BTreeMap<String, BTreeMap<String, OpenApiOperation>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct OpenApiComponents {
    schemas: BTreeMap<String, OpenApiSchema>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct OpenApiInfo {
    title: String,
    version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct OpenApiOperation {
    #[serde(skip_serializing_if = "Option::is_none")]
    summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    operation_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tags: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    parameters: Option<Vec<OpenApiParameter>>,
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

impl OpenApiOperation {
    fn for_method(method: String, path: &str) -> Self {
        let mut operation = Self::default();
        if method_supports_body(&method) {
            operation.request_body = Some(OpenApiRequestBody::json_payload());
        }

        let parameters = parse_path_parameters(path);
        if !parameters.is_empty() {
            operation.parameters = Some(parameters);
        }

        operation
    }

    fn set_request_body(
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

    fn set_response_body(
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

#[derive(Clone, Debug, Serialize, Deserialize)]
struct OpenApiParameter {
    name: String,
    #[serde(rename = "in")]
    location: String,
    required: bool,
    schema: OpenApiSchema,
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

impl OpenApiRequestBody {
    fn json_payload() -> Self {
        Self {
            description: None,
            content: default_json_content(),
            required: Some(true),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct OpenApiMediaType {
    schema: OpenApiSchema,
    #[serde(skip_serializing_if = "Option::is_none")]
    example: Option<Value>,
}

fn parse_path_parameters(path: &str) -> Vec<OpenApiParameter> {
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

fn default_example() -> Value {
    json!({})
}

fn default_json_content() -> BTreeMap<String, OpenApiMediaType> {
    media_content(
        APPLICATION_JSON.as_ref(),
        OpenApiSchema::object(),
        Some(default_example()),
    )
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
    <script src="https://unpkg.com/swagger-ui-dist@5/swagger-ui-standalone-preset.js"></script>
    <script>
      window.onload = function() {{
        SwaggerUIBundle({{
          urls: [
            {{ url: "{spec_path}", name: "v1" }},
          ],
          dom_id: "#swagger-ui",
          presets: [
            SwaggerUIBundle.presets.apis,
            SwaggerUIStandalonePreset
          ],
          layout: "StandaloneLayout"
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
