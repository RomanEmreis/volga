//! Startup configuration processing: reads the file-based config and applies
//! built-in sections to `App`, then builds the `ConfigStore`.

use crate::{
    App,
    config::{ConfigBuilder, ConfigStore, builder::parse_config_file},
};
use serde_json::Value;
use std::{io, path::Path, sync::Arc};

impl App {
    /// Resolves file-based configuration at startup.
    ///
    /// Reads the config file once, applies built-in sections (server, tls, tracing,
    /// openapi, cors) to the `App` builder, and builds the `ConfigStore` from user
    /// bindings.
    ///
    /// Precedence:
    /// - Built-in section missing from file → no change to `App` fields.
    /// - Built-in section present and valid → applied (**overrides** prior builder calls).
    /// - Built-in section present but invalid → startup error.
    pub(crate) fn process_config(mut self, builder: ConfigBuilder) -> Result<Self, io::Error> {
        if builder.reload_interval.is_some() && builder.file_path.is_none() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "config: reload_on_change() requires from_file() to be called",
            ));
        }

        let file_path = builder.file_path.clone().unwrap_or_default();

        let full_value = load_value(&file_path)?;

        self = self.apply_server_section(&full_value)?;
        #[cfg(feature = "tls")]
        {
            self = self.apply_tls_section(&full_value)?;
        }
        #[cfg(feature = "tracing")]
        {
            self = self.apply_tracing_section(&full_value)?;
        }
        #[cfg(feature = "openapi")]
        {
            self = self.apply_openapi_section(&full_value)?;
        }
        #[cfg(feature = "middleware")]
        {
            self = self.apply_cors_section(&full_value)?;
        }
        #[cfg(feature = "oauth")]
        {
            self = self.apply_oauth_section(&full_value)?;
        }

        let store = builder
            .build_from_value(&full_value)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        self.config_store = Some(Arc::new(store));

        Ok(self)
    }

    /// Applies the `[server]` section from the parsed config value.
    fn apply_server_section(mut self, value: &Value) -> Result<Self, io::Error> {
        #[derive(serde::Deserialize)]
        struct ServerSection {
            host: Option<String>,
            port: Option<u16>,
            body_limit_bytes: Option<usize>,
            max_header_count: Option<usize>,
            max_connections: Option<usize>,
        }

        let Some(s) = parse_section::<ServerSection>(value, "server")? else {
            return Ok(self);
        };

        match (&s.host, s.port) {
            (Some(host), Some(port)) => {
                self = self.bind((parse_host(host)?, port));
            }
            (Some(host), None) => {
                let port = self.socket_addr().port();
                self = self.bind((parse_host(host)?, port));
            }
            (None, Some(port)) => {
                let ip = self.socket_addr().ip();
                self = self.bind((ip, port));
            }
            (None, None) => {}
        }
        if let Some(bytes) = s.body_limit_bytes {
            self = if bytes == 0 {
                self.without_body_limit()
            } else {
                self.with_body_limit(crate::Limit::Limited(bytes))
            };
        }
        if let Some(n) = s.max_header_count {
            self = self.with_max_header_count(crate::Limit::Limited(n));
        }
        if let Some(n) = s.max_connections {
            self = self.with_max_connections(if n == 0 {
                crate::Limit::Unlimited
            } else {
                crate::Limit::Limited(n)
            });
        }

        Ok(self)
    }

    /// Applies the `[tls]` section from the parsed config value.
    #[cfg(feature = "tls")]
    fn apply_tls_section(mut self, value: &Value) -> Result<Self, io::Error> {
        if let Some(tls_cfg) = parse_section::<crate::tls::TlsConfig>(value, "tls")? {
            self = self.set_tls(tls_cfg);
        }
        Ok(self)
    }

    /// Applies the `[tracing]` section from the parsed config value.
    #[cfg(feature = "tracing")]
    fn apply_tracing_section(mut self, value: &Value) -> Result<Self, io::Error> {
        if let Some(tr_cfg) = parse_section::<crate::tracing::TracingConfig>(value, "tracing")? {
            self = self.set_tracing(tr_cfg);
        }
        Ok(self)
    }

    /// Applies the `[openapi]` section from the parsed config value.
    #[cfg(feature = "openapi")]
    fn apply_openapi_section(mut self, value: &Value) -> Result<Self, io::Error> {
        if let Some(oa_cfg) = parse_section::<crate::openapi::OpenApiConfig>(value, "openapi")? {
            self = self.with_open_api(|existing| {
                // File config wins, but preserve the runtime-only `exposed` flag.
                let mut new = oa_cfg;
                new.exposed = existing.exposed;
                if new.specs().is_empty() {
                    new.with_specs(existing.specs().to_vec())
                } else {
                    new
                }
            });
        }
        Ok(self)
    }

    /// Applies the `[cors]` section from the parsed config value.
    #[cfg(feature = "middleware")]
    fn apply_cors_section(mut self, value: &Value) -> Result<Self, io::Error> {
        if let Some(cors_cfg) = parse_section::<crate::http::cors::CorsConfig>(value, "cors")? {
            self = self.set_cors(cors_cfg);
        }
        Ok(self)
    }

    /// Applies the `[oauth.resource]`, `[oauth.server]` and `[oauth.client]`
    /// sections from the parsed config value.
    #[cfg(feature = "oauth")]
    fn apply_oauth_section(mut self, value: &Value) -> Result<Self, io::Error> {
        use crate::auth::oauth::{AuthorizationServerMetadata, ProtectedResourceMetadata};

        let Some(section) = value.get("oauth") else {
            return Ok(self);
        };

        if let Some(resource) = section.get("resource") {
            let metadata: ProtectedResourceMetadata = parse_subsection(resource, "oauth.resource")?;
            self = self.set_oauth_resource_metadata(metadata);
        }
        if let Some(server) = section.get("server") {
            let mut server = server.clone();
            // Mirror the `AuthorizationServerMetadata::new()` prefills so a
            // minimal `[oauth.server]` section behaves like the builder DSL:
            // `response_types_supported` is REQUIRED per RFC 8414 §2, and
            // leaving `grant_types_supported` absent would make clients
            // assume the implicit grant is supported
            if let Some(obj) = server.as_object_mut() {
                obj.entry("response_types_supported")
                    .or_insert_with(|| serde_json::json!(["code"]));
                obj.entry("grant_types_supported")
                    .or_insert_with(|| serde_json::json!(["authorization_code"]));
            }
            let metadata: AuthorizationServerMetadata = parse_subsection(&server, "oauth.server")?;
            self = self.set_oauth_server_metadata(metadata);
        }
        #[cfg(feature = "oauth-client")]
        if let Some(client) = section.get("client") {
            self = self.apply_oauth_client_section(client)?;
        }
        Ok(self)
    }

    /// Applies the `[oauth.client]` section from the parsed config value.
    ///
    /// Present fields are merged into the [`OAuthConfig`](crate::auth::OAuthConfig)
    /// built so far, overriding prior builder calls; activation still requires
    /// an explicit [`App::use_oauth`] call in code.
    #[cfg(feature = "oauth-client")]
    fn apply_oauth_client_section(mut self, value: &Value) -> Result<Self, io::Error> {
        use std::time::Duration;

        // unknown keys are rejected: a silently ignored typo in a
        // security-relevant knob (`require_https`) must not go unnoticed
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct OAuthClientSection {
            issuer: Option<String>,
            refresh_cooldown_secs: Option<u64>,
            max_key_age_secs: Option<u64>,
            require_https: Option<bool>,
            timeout_secs: Option<u64>,
            max_redirects: Option<u8>,
        }

        let s: OAuthClientSection = parse_subsection(value, "oauth.client")?;

        let mut oauth = self.oauth_client_config.take().unwrap_or_default();
        if let Some(issuer) = s.issuer {
            oauth = oauth.with_issuer(issuer);
        }
        if let Some(secs) = s.refresh_cooldown_secs {
            oauth = oauth.with_refresh_cooldown(Duration::from_secs(secs));
        }
        if let Some(secs) = s.max_key_age_secs {
            oauth = oauth.with_max_key_age(Duration::from_secs(secs));
        }
        oauth = oauth.with_client_config(|mut client| {
            if let Some(required) = s.require_https {
                client = client.require_https(required);
            }
            if let Some(secs) = s.timeout_secs {
                client = client.with_timeout(Duration::from_secs(secs));
            }
            if let Some(limit) = s.max_redirects {
                client = client.with_max_redirects(limit);
            }
            client
        });
        self.oauth_client_config = Some(oauth);
        Ok(self)
    }
}

/// Deserializes a nested built-in section (e.g. `[oauth.server]`), reporting
/// the full dotted key in the startup error.
#[cfg(feature = "oauth")]
fn parse_subsection<T: serde::de::DeserializeOwned>(
    value: &Value,
    key: &str,
) -> Result<T, io::Error> {
    serde_json::from_value(value.clone()).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("config: [{key}] section is invalid: {e}"),
        )
    })
}

/// Spawns a background task that periodically reloads config from file.
///
/// Reads `store.reload` to determine whether hot-reload is configured.
/// If `store.reload` is `None`, it returns immediately without spawning anything.
pub(crate) fn spawn_reload(
    store: &Arc<ConfigStore>,
    shutdown: Arc<tokio::sync::watch::Sender<()>>,
) {
    let Some((interval, file_path)) = store.reload.as_ref().cloned() else {
        return;
    };
    let store = Arc::clone(store);
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = tokio::time::sleep(interval) => {}
                _ = shutdown.closed() => break,
            }
            match parse_config_file(&file_path) {
                Ok(value) => store.reload_sections(&value),
                #[cfg(feature = "tracing")]
                Err(_e) => tracing::error!("config reload: cannot read file: {_e:#}"),
                #[cfg(not(feature = "tracing"))]
                Err(_) => {}
            }
        }
    });
}

/// Parses a host string as a [`std::net::IpAddr`], returning a descriptive startup error
/// if the value is not a valid IP address (hostname, unbracketed IPv6, etc.).
fn parse_host(host: &str) -> Result<std::net::IpAddr, io::Error> {
    host.parse().map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("config: [server] invalid host address '{host}' (must be a valid IP address)"),
        )
    })
}

/// Parses a config file into a `serde_json::Value`, or returns an empty object
/// when the path is empty (no file configured).
fn load_value(file_path: &Path) -> Result<Value, io::Error> {
    if file_path.as_os_str().is_empty() {
        return Ok(Value::Object(Default::default()));
    }
    parse_config_file(file_path).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

/// Parses a built-in section from the full config value.
///
/// Returns `Ok(None)` if the key is absent, `Ok(Some(T))` if present and valid,
/// or `Err` if the section is present but fails to deserialize.
fn parse_section<T: serde::de::DeserializeOwned>(
    full_value: &Value,
    key: &str,
) -> Result<Option<T>, io::Error> {
    match full_value.get(key) {
        None => Ok(None),
        Some(v) => serde_json::from_value::<T>(v.clone())
            .map(Some)
            .map_err(|e| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("config: [{key}] section is invalid: {e}"),
                )
            }),
    }
}

#[cfg(test)]
mod tests {
    use crate::App;
    use std::io::Write;

    fn write_toml(content: &str) -> tempfile::NamedTempFile {
        let mut f = tempfile::NamedTempFile::with_suffix(".toml").unwrap();
        f.write_all(content.as_bytes()).unwrap();
        f
    }

    #[test]
    fn host_only_section_preserves_port() {
        let file = write_toml("[server]\nhost = \"0.0.0.0\"\n");
        let path = file.path().to_str().unwrap().to_owned();

        let app = App::new()
            .bind("127.0.0.1:7878")
            .with_config(|cfg| cfg.with_file(&path));

        let addr = app.socket_addr();
        assert_eq!(
            addr.port(),
            7878,
            "port must be preserved when only host is set"
        );
        assert_eq!(
            addr.ip().to_string(),
            "0.0.0.0",
            "host must be updated from config"
        );
    }

    #[test]
    fn port_only_section_preserves_host() {
        let file = write_toml("[server]\nport = 9090\n");
        let path = file.path().to_str().unwrap().to_owned();

        let app = App::new()
            .bind("127.0.0.1:7878")
            .with_config(|cfg| cfg.with_file(&path));

        let addr = app.socket_addr();
        assert_eq!(addr.port(), 9090, "port must be updated from config");
        assert_eq!(
            addr.ip().to_string(),
            "127.0.0.1",
            "host must be preserved when only port is set"
        );
    }

    #[test]
    fn host_and_port_section_overrides_both() {
        let file = write_toml("[server]\nhost = \"0.0.0.0\"\nport = 9090\n");
        let path = file.path().to_str().unwrap().to_owned();

        let app = App::new()
            .bind("127.0.0.1:7878")
            .with_config(|cfg| cfg.with_file(&path));

        let addr = app.socket_addr();
        assert_eq!(addr.port(), 9090);
        assert_eq!(addr.ip().to_string(), "0.0.0.0");
    }

    #[test]
    #[should_panic(expected = "config:")]
    fn invalid_host_panics_at_startup() {
        let file = write_toml("[server]\nhost = \"localhost\"\n");
        let path = file.path().to_str().unwrap().to_owned();
        App::new().with_config(|cfg| cfg.with_file(&path));
    }

    #[test]
    #[should_panic(expected = "config:")]
    fn invalid_host_with_port_panics_at_startup() {
        let file = write_toml("[server]\nhost = \"not-an-ip\"\nport = 8080\n");
        let path = file.path().to_str().unwrap().to_owned();
        App::new().with_config(|cfg| cfg.with_file(&path));
    }

    #[test]
    fn server_section_body_limit_zero_removes_limit() {
        let file = write_toml("[server]\nbody_limit_bytes = 0\n");
        let path = file.path().to_str().unwrap().to_owned();
        // should not panic — body_limit = 0 → Unlimited
        App::new().with_config(|cfg| cfg.with_file(&path));
    }

    #[test]
    fn server_section_all_fields() {
        let file = write_toml(
            "[server]\nhost = \"127.0.0.1\"\nport = 8181\nbody_limit_bytes = 1024\nmax_header_count = 50\nmax_connections = 100\n",
        );
        let path = file.path().to_str().unwrap().to_owned();

        let app = App::new().with_config(|cfg| cfg.with_file(&path));

        let addr = app.socket_addr();
        assert_eq!(addr.port(), 8181);
        assert_eq!(addr.ip().to_string(), "127.0.0.1");
    }

    #[test]
    #[should_panic(expected = "config:")]
    fn reload_on_change_without_file_panics() {
        // ConfigBuilder with reload but no file path must fail at process_config time.
        App::new().with_config(|cfg| cfg.reload_on_change());
    }

    #[test]
    fn server_section_max_connections_zero_is_unlimited() {
        let file = write_toml("[server]\nmax_connections = 0\n");
        let path = file.path().to_str().unwrap().to_owned();
        // max_connections = 0 → Unlimited; must not panic.
        App::new().with_config(|cfg| cfg.with_file(&path));
    }

    #[test]
    fn no_file_configured_uses_empty_config() {
        // with_config without from_file: no file → empty object → no panic.
        App::new().with_config(|cfg| cfg);
    }

    #[cfg(feature = "tracing")]
    #[test]
    fn tracing_section_applied_from_config() {
        let file = write_toml("[tracing]\ninclude_header = true\n");
        let path = file.path().to_str().unwrap().to_owned();
        let app = App::new().with_config(|cfg| cfg.with_file(&path));
        assert!(app.tracing_config.is_some());
    }

    #[cfg(feature = "tracing")]
    #[test]
    #[should_panic(expected = "config:")]
    fn invalid_tracing_section_panics_at_startup() {
        // include_header must be bool, not a string → parse_section must return Err
        let file = write_toml("[tracing]\ninclude_header = \"yes\"\n");
        let path = file.path().to_str().unwrap().to_owned();
        App::new().with_config(|cfg| cfg.with_file(&path));
    }

    #[cfg(feature = "openapi")]
    #[test]
    fn openapi_section_applied_from_config() {
        let file = write_toml("[openapi]\ntitle = \"My API\"\n");
        let path = file.path().to_str().unwrap().to_owned();
        App::new().with_config(|cfg| cfg.with_file(&path));
    }

    #[cfg(feature = "middleware")]
    #[test]
    fn cors_section_applied_from_config() {
        let file = write_toml("[cors]\n");
        let path = file.path().to_str().unwrap().to_owned();
        App::new().with_config(|cfg| cfg.with_file(&path));
    }

    #[cfg(feature = "oauth")]
    #[test]
    fn oauth_section_applied_from_config() {
        let file = write_toml(
            "[oauth.resource]\n\
             resource = \"https://api.example.com\"\n\
             authorization_servers = [\"https://auth.example.com\"]\n\
             scopes_supported = [\"read\"]\n\
             \n\
             [oauth.server]\n\
             issuer = \"https://auth.example.com\"\n\
             subject_types_supported = [\"public\"]\n",
        );
        let path = file.path().to_str().unwrap().to_owned();

        let app = App::new().with_config(|cfg| cfg.with_file(&path));

        let resource = app.oauth_resource_metadata.as_ref().unwrap();
        assert_eq!(resource.resource, "https://api.example.com");
        assert_eq!(resource.authorization_servers, ["https://auth.example.com"]);
        assert_eq!(resource.scopes_supported, ["read"]);

        let server = app.oauth_server_metadata.as_ref().unwrap();
        assert_eq!(server.issuer, "https://auth.example.com");
        // a minimal section gets the `new()` prefills
        assert_eq!(server.response_types_supported, ["code"]);
        assert_eq!(server.grant_types_supported, ["authorization_code"]);
        // unknown keys land in `additional_fields` (flatten)
        assert_eq!(
            server.additional_fields["subject_types_supported"],
            serde_json::json!(["public"])
        );
    }

    #[cfg(feature = "oauth")]
    #[test]
    fn oauth_section_overrides_builder_calls() {
        let file = write_toml("[oauth.server]\nissuer = \"https://file.example.com\"\n");
        let path = file.path().to_str().unwrap().to_owned();

        let app = App::new()
            .set_oauth_server_metadata("https://builder.example.com")
            .with_config(|cfg| cfg.with_file(&path));

        let server = app.oauth_server_metadata.as_ref().unwrap();
        assert_eq!(server.issuer, "https://file.example.com");
    }

    #[cfg(feature = "oauth")]
    #[test]
    #[should_panic(expected = "config: [oauth.resource] section is invalid")]
    fn invalid_oauth_section_panics_at_startup() {
        // `resource` is required for the resource metadata document
        let file = write_toml("[oauth.resource]\nscopes_supported = [\"read\"]\n");
        let path = file.path().to_str().unwrap().to_owned();
        App::new().with_config(|cfg| cfg.with_file(&path));
    }

    #[cfg(feature = "oauth-client")]
    #[test]
    fn oauth_client_section_applied_from_config() {
        use std::time::Duration;

        let file = write_toml(
            "[oauth.client]\n\
             issuer = \"https://auth.example.com\"\n\
             refresh_cooldown_secs = 5\n\
             max_key_age_secs = 120\n\
             require_https = false\n\
             timeout_secs = 10\n\
             max_redirects = 2\n",
        );
        let path = file.path().to_str().unwrap().to_owned();

        let app = App::new().with_config(|cfg| cfg.with_file(&path));

        let oauth = app.oauth_client_config.as_ref().unwrap();
        assert_eq!(oauth.issuer.as_deref(), Some("https://auth.example.com"));
        assert_eq!(oauth.refresh_cooldown(), Duration::from_secs(5));
        assert_eq!(oauth.max_key_age(), Duration::from_secs(120));
        assert!(!oauth.client_config().enforce_https());
        assert_eq!(oauth.client_config().timeout(), Duration::from_secs(10));
        assert_eq!(oauth.client_config().max_redirects(), 2);
        // the file only describes the issuer — activation stays in code
        assert!(!app.oauth_client_enabled);
    }

    #[cfg(feature = "oauth-client")]
    #[test]
    fn oauth_client_section_merges_with_builder_calls() {
        use std::time::Duration;

        // the file sets only the cooldown; the builder-set issuer survives
        let file = write_toml("[oauth.client]\nrefresh_cooldown_secs = 5\n");
        let path = file.path().to_str().unwrap().to_owned();

        let app = App::new()
            .with_oauth(|oauth| oauth.with_issuer("https://builder.example.com"))
            .with_config(|cfg| cfg.with_file(&path));

        let oauth = app.oauth_client_config.as_ref().unwrap();
        assert_eq!(oauth.issuer.as_deref(), Some("https://builder.example.com"));
        assert_eq!(oauth.refresh_cooldown(), Duration::from_secs(5));
    }

    #[cfg(feature = "oauth-client")]
    #[test]
    fn oauth_client_section_overrides_builder_fields_it_names() {
        let file = write_toml("[oauth.client]\nissuer = \"https://file.example.com\"\n");
        let path = file.path().to_str().unwrap().to_owned();

        let app = App::new()
            .with_oauth(|oauth| oauth.with_issuer("https://builder.example.com"))
            .with_config(|cfg| cfg.with_file(&path));

        let oauth = app.oauth_client_config.as_ref().unwrap();
        assert_eq!(oauth.issuer.as_deref(), Some("https://file.example.com"));
    }

    #[cfg(feature = "oauth-client")]
    #[test]
    #[should_panic(expected = "config: [oauth.client] section is invalid")]
    fn unknown_oauth_client_key_panics_at_startup() {
        // a typo in a security-relevant knob must not be silently ignored
        let file = write_toml("[oauth.client]\nrequire_http = false\n");
        let path = file.path().to_str().unwrap().to_owned();
        App::new().with_config(|cfg| cfg.with_file(&path));
    }

    #[cfg(feature = "oauth-client")]
    #[test]
    fn oauth_client_section_enables_use_oauth() {
        let file = write_toml("[oauth.client]\nissuer = \"https://auth.example.com\"\n");
        let path = file.path().to_str().unwrap().to_owned();

        let mut app = App::new().with_config(|cfg| cfg.with_file(&path));
        app.use_oauth();
        assert!(app.oauth_client_enabled);
    }
}
