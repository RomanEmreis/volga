//! Startup configuration processing: reads the file-based config and applies
//! built-in sections to `App`, then builds the `ConfigStore`.

use crate::{
    App,
    config::{ConfigBuilder, ConfigStore, builder::parse_config_file},
};
use serde_json::Value;
use std::{io, sync::Arc, time::Duration};

/// Reload info returned by [`App::process_config`]: `(store, interval, file_path)`.
pub(crate) type ReloadInfo = (Arc<ConfigStore>, Duration, String);

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
    pub(crate) fn process_config(mut self) -> Result<(Self, Option<ReloadInfo>), io::Error> {
        let Some(builder) = self.resolve_config_builder()? else {
            return Ok((self, None));
        };

        if builder.reload_interval.is_some() && builder.file_path.is_none() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "config: reload_on_change() requires from_file() to be called",
            ));
        }

        let file_path = builder.file_path.clone().unwrap_or_default();
        let reload_interval = builder.reload_interval;

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

        let (store, _) = builder
            .build_from_value(&full_value)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        let store = Arc::new(store);
        self.config_store = Some(Arc::clone(&store));

        let reload = reload_interval.map(|interval| (store, interval, file_path));
        Ok((self, reload))
    }

    /// Resolves the [`ConfigBuilder`] from the app state.
    ///
    /// Returns `None` when neither `with_config` nor `with_default_config` was called.
    fn resolve_config_builder(&mut self) -> Result<Option<ConfigBuilder>, io::Error> {
        if self.use_default_config && self.config_builder.is_none() {
            use std::path::Path;
            let path = if Path::new("app_config.toml").exists() {
                "app_config.toml"
            } else if Path::new("app_config.json").exists() {
                "app_config.json"
            } else {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    "config: with_default_config() found neither app_config.toml nor app_config.json",
                ));
            };
            Ok(Some(ConfigBuilder::new().from_file(path)))
        } else {
            Ok(self.config_builder.take())
        }
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

        if let (Some(host), Some(port)) = (&s.host, s.port) {
            self = self.bind(format!("{host}:{port}").as_str());
        } else if let Some(port) = s.port {
            let ip = self.socket_addr().ip();
            self = self.bind(format!("{ip}:{port}").as_str());
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
