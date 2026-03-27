//! Startup configuration processing: reads the file-based config and applies
//! built-in sections to `App`, then builds the `ConfigStore`.

use crate::{
    App,
    config::{ConfigBuilder, ConfigStore, builder::parse_config_file},
};
use serde_json::Value;
use std::{io, sync::Arc};

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
                new
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
fn load_value(file_path: &str) -> Result<Value, io::Error> {
    if file_path.is_empty() {
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
}
