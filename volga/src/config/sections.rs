//! Deserializable structs for built-in config file sections.
//!
//! Each struct maps to a top-level section key in the config file.
//! Fields are `Option<T>` so partial overrides work — only present fields
//! are applied to the `App` builder.

use serde::Deserialize;

/// `[server]` section — controls TCP binding and request limits.
#[derive(Debug, Deserialize)]
pub(crate) struct ServerSection {
    /// Bind address (e.g. `"0.0.0.0"`)
    pub host: Option<String>,
    /// Bind port (e.g. `8080`)
    pub port: Option<u16>,
    /// Request body limit in bytes. `0` means unlimited.
    pub body_limit_bytes: Option<usize>,
    /// Max number of HTTP request headers.
    pub max_header_count: Option<usize>,
    /// Max number of concurrent TCP connections. `0` means unlimited.
    pub max_connections: Option<usize>,
}

/// `[tls]` section — TLS certificate paths and HTTPS redirection options.
#[cfg(feature = "tls")]
#[derive(Debug, Deserialize)]
pub(crate) struct TlsSection {
    /// Path to the PEM certificate file.
    pub cert: String,
    /// Path to the PEM private key file.
    pub key: String,
    /// Whether to enable HTTPS → HTTP redirection listener.
    pub https_redirection: Option<bool>,
    /// HTTP port for the redirection listener (default 7879).
    pub http_port: Option<u16>,
}

/// `[tracing]` section — request tracing options.
#[cfg(feature = "tracing")]
#[derive(Debug, Deserialize)]
pub(crate) struct TracingSection {
    /// Whether to include a span-id header in responses.
    ///
    /// Note: `header_name` cannot be set via config file because it requires a `'static` str.
    /// Use `App::with_tracing()` for custom header names.
    #[serde(default)]
    pub include_header: bool,
}

/// `[openapi]` section — OpenAPI document metadata.
#[cfg(feature = "openapi")]
#[derive(Debug, Deserialize)]
pub(crate) struct OpenApiSection {
    /// OpenAPI document title.
    pub title: Option<String>,
    /// OpenAPI document version string.
    pub version: Option<String>,
    /// Optional description.
    pub description: Option<String>,
}

/// `[cors]` section — cross-origin resource sharing settings.
///
/// Presence of this section in the config automatically enables CORS.
/// Only active when the `middleware` feature is enabled.
#[cfg(feature = "middleware")]
#[derive(Debug, Deserialize)]
pub(crate) struct CorsSection {
    /// Allowed origin patterns (e.g. `["https://example.com"]`).
    pub allowed_origins: Option<Vec<String>>,
    /// Allowed HTTP methods (e.g. `["GET", "POST"]`).
    pub allowed_methods: Option<Vec<String>>,
    /// Allowed request headers.
    pub allowed_headers: Option<Vec<String>>,
    /// Whether to allow credentials.
    pub allow_credentials: Option<bool>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_section_parses() {
        let toml = r#"
            host = "0.0.0.0"
            port = 9090
            body_limit_bytes = 10485760
        "#;
        let s: ServerSection = toml::from_str(toml).unwrap();
        assert_eq!(s.host.as_deref(), Some("0.0.0.0"));
        assert_eq!(s.port, Some(9090));
        assert_eq!(s.body_limit_bytes, Some(10485760));
    }

    #[cfg(feature = "tls")]
    #[test]
    fn tls_section_parses() {
        let toml = r#"
            cert = "certs/cert.pem"
            key = "certs/key.pem"
        "#;
        let s: TlsSection = toml::from_str(toml).unwrap();
        assert_eq!(s.cert, "certs/cert.pem");
        assert_eq!(s.key, "certs/key.pem");
    }

    #[cfg(feature = "tracing")]
    #[test]
    fn tracing_section_parses() {
        let toml = r#"include_header = true"#;
        let s: TracingSection = toml::from_str(toml).unwrap();
        assert!(s.include_header);
    }

    #[cfg(feature = "openapi")]
    #[test]
    fn openapi_section_parses() {
        let toml = r#"
            title = "My API"
            version = "2.0"
        "#;
        let s: OpenApiSection = toml::from_str(toml).unwrap();
        assert_eq!(s.title.as_deref(), Some("My API"));
    }
}
