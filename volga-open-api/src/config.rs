//! Types and utils for OpenAPI configuration.

pub(super) const DEFAULT_SPEC_PATH: &str = "/openapi.json";
const DEFAULT_SPEC_NAME: &str = "v1";
const DEFAULT_UI_PATH: &str = "/openapi";

/// OpenAPI runtime configuration.
#[derive(Clone, Debug)]
pub struct OpenApiConfig {
    /// Returns `true` if the OpenAPI have been exposed.
    pub exposed: bool,
    
    pub(super) title: String,
    pub(super) version: String,
    pub(super) description: Option<String>,
    pub(super) specs: Vec<OpenApiSpec>,
    pub(super) ui_enabled: bool,
    pub(super) ui_path: String,
}

/// OpenAPI spec
#[derive(Clone, Debug)]
pub struct OpenApiSpec {
    /// Spec name. Used to distinguish between multiple OpenAPI specs.
    pub name: String,

    /// Path to OpenAPI spec JSON.
    pub spec_path: String,
}

impl Default for OpenApiConfig {
    fn default() -> Self {
        Self {
            exposed: false,
            title: "Volga API".to_string(),
            version: "0.1.0".to_string(),
            description: None,
            specs: vec![OpenApiSpec::default()],
            ui_enabled: false,
            ui_path: DEFAULT_UI_PATH.to_string(),
        }
    }
}

impl Default for OpenApiSpec {
    fn default() -> Self {
        Self {
            name: DEFAULT_SPEC_NAME.to_string(),
            spec_path: DEFAULT_SPEC_PATH.to_string(),
        }
    }
}

impl<T: Into<String>> From<T> for OpenApiSpec {
    #[inline]
    fn from(value: T) -> Self {
        Self::new(value)
    }
}

impl OpenApiSpec {
    /// Creates a new OpenAPI spec with the given name.
    #[inline]
    pub fn new(name: impl Into<String>) -> Self {
        let name = name.into();
        Self {
            spec_path: format!("/{name}{DEFAULT_SPEC_PATH}"),
            name,
        }
    }

    /// Sets OpenAPI spec path.
    pub fn with_path(mut self, path: impl Into<String>) -> Self {
        self.spec_path = path.into();
        self
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

    /// Sets the OpenAPI spec.
    ///
    /// Default: `/openapi.json`
    pub fn with_spec(mut self, spec: OpenApiSpec) -> Self {
        self.specs = vec![spec];
        self
    }

    /// Sets the OpenAPI specs.
    ///
    /// Default: `/openapi.json`
    pub fn with_specs<I, S>(mut self, specs: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<OpenApiSpec>
    {
        let specs: Vec<_> = specs.into_iter().map(Into::into).collect();
        self.specs = if specs.is_empty() {
            vec![OpenApiSpec::default()]
        } else {
            specs
        };
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
    
    /// Returns the title.
    pub fn title(&self) -> &str {
        &self.title
    }

    /// Returns a list of registered OpenAPI specs.
    pub fn specs(&self) -> &[OpenApiSpec] {
        &self.specs
    }

    /// Returns `true` if UI is enabled.
    pub fn ui_enabled(&self) -> bool {
        self.ui_enabled
    }

    /// Returns UI URL path.
    pub fn ui_path(&self) -> &str {
        &self.ui_path
    }
}

#[cfg(test)]
mod tests {
    use super::{DEFAULT_SPEC_PATH, OpenApiConfig, OpenApiSpec};

    #[test]
    fn default_config_contains_single_default_spec_and_ui_disabled() {
        let config = OpenApiConfig::default();

        assert_eq!(config.title(), "Volga API");
        assert_eq!(config.specs().len(), 1);
        assert_eq!(config.specs()[0].name, "v1");
        assert_eq!(config.specs()[0].spec_path, DEFAULT_SPEC_PATH);
        assert!(!config.ui_enabled());
        assert_eq!(config.ui_path(), "/openapi");
    }

    #[test]
    fn spec_new_builds_path_from_spec_name() {
        let spec = OpenApiSpec::new("admin");
        assert_eq!(spec.name, "admin");
        assert_eq!(spec.spec_path, "/admin/openapi.json");
    }

    #[test]
    fn with_specs_accepts_mixed_inputs() {
        let config = OpenApiConfig::new().with_specs([
            OpenApiSpec::new("v1").with_path("/docs/v1.json"),
            OpenApiSpec::from("v2"),
        ]);

        assert_eq!(config.specs().len(), 2);
        assert_eq!(config.specs()[0].spec_path, "/docs/v1.json");
        assert_eq!(config.specs()[1].name, "v2");
        assert_eq!(config.specs()[1].spec_path, "/v2/openapi.json");
    }

    #[test]
    fn with_spec_replaces_existing_specs() {
        let config = OpenApiConfig::new().with_spec(OpenApiSpec::new("admin"));

        assert_eq!(config.specs().len(), 1);
        assert_eq!(config.specs()[0].name, "admin");
        assert_eq!(config.specs()[0].spec_path, "/admin/openapi.json");
    }

    #[test]
    fn with_title_version_and_description_override_defaults() {
        let config = OpenApiConfig::new()
            .with_title("Custom API")
            .with_version("2.5.1")
            .with_description("custom description");

        assert_eq!(config.title(), "Custom API");
        assert_eq!(config.version, "2.5.1");
        assert_eq!(config.description.as_deref(), Some("custom description"));
    }

    #[test]
    fn with_specs_restores_default_when_given_empty_list() {
        let config = OpenApiConfig::new().with_specs(Vec::<OpenApiSpec>::new());

        assert_eq!(config.specs().len(), 1);
        assert_eq!(config.specs()[0].name, "v1");
        assert_eq!(config.specs()[0].spec_path, DEFAULT_SPEC_PATH);
    }
}