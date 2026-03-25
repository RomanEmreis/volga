//! Builder for the file-based configuration system.

use crate::config::store::{ConfigStore, SectionKind};
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::time::Duration;

const DEFAULT_RELOAD_INTERVAL: Duration = Duration::from_secs(5);

type RegisterFn = Box<dyn FnOnce(&mut ConfigStore, &Value) -> Result<(), String> + Send>;

/// Builds and validates the file-based configuration.
///
/// Created via [`App::with_config`] or [`App::with_default_config`].
///
/// # Example
/// ```no_run
/// use volga::App;
/// use serde::Deserialize;
/// #[derive(Deserialize)] struct Database { url: String }
///
/// App::new().with_config(|cfg| {
///     cfg.from_file("app_config.toml")
///        .bind_section::<Database>("database")
///        .reload_on_change()
/// });
/// ```
pub struct ConfigBuilder {
    /// Path to the config file (`None` until `from_file` is called).
    pub(crate) file_path: Option<String>,
    bindings: Vec<RegisterFn>,
    /// Interval for hot-reload polling (`None` means no hot-reload).
    pub(crate) reload_interval: Option<Duration>,
}

impl std::fmt::Debug for ConfigBuilder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConfigBuilder")
            .field("file_path", &self.file_path)
            .field("binding_count", &self.bindings.len())
            .field("reload_interval", &self.reload_interval)
            .finish()
    }
}

impl Default for ConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl ConfigBuilder {
    /// Creates an empty builder.
    pub fn new() -> Self {
        Self {
            file_path: None,
            bindings: Vec::new(),
            reload_interval: None,
        }
    }

    /// Sets the config file path.
    ///
    /// Supported formats: `.toml`, `.json` (detected by file extension).
    pub fn from_file(mut self, path: impl Into<String>) -> Self {
        self.file_path = Some(path.into());
        self
    }

    /// Binds a config section to type `T`.
    ///
    /// Produces a startup error if the section is absent or malformed.
    pub fn bind_section<T>(mut self, key: impl Into<String>) -> Self
    where
        T: DeserializeOwned + Send + Sync + 'static,
    {
        let key = key.into();
        self.bindings.push(Box::new(move |store, value| {
            store.register::<T>(&key, SectionKind::Required, value)
        }));
        self
    }

    /// Binds an optional config section to type `T`.
    ///
    /// If the section is absent, `Option<Config<T>>` extracts as `None`.
    pub fn bind_section_optional<T>(mut self, key: impl Into<String>) -> Self
    where
        T: DeserializeOwned + Send + Sync + 'static,
    {
        let key = key.into();
        self.bindings.push(Box::new(move |store, value| {
            store.register::<T>(&key, SectionKind::Optional, value)
        }));
        self
    }

    /// Enables hot-reload with the default 5-second poll interval.
    ///
    /// Requires `from_file()` to also be called; produces a startup error otherwise.
    pub fn reload_on_change(mut self) -> Self {
        self.reload_interval = Some(DEFAULT_RELOAD_INTERVAL);
        self
    }

    /// Reads and parses the configured file, returning its contents as `serde_json::Value`.
    ///
    /// Returns `Err` if no file path was configured, the file cannot be read, or parsing fails.
    pub fn load_file(&self) -> Result<Value, String> {
        let path = self.file_path.as_deref().ok_or_else(|| {
            "config: no file path configured; call from_file() before reload_on_change()".to_owned()
        })?;
        parse_config_file(path)
    }

    /// Builds a `ConfigStore` from an already-parsed `Value`.
    ///
    /// Preferred over re-reading the file when the caller already has the parsed value
    /// (e.g. from built-in section processing). Avoids double I/O.
    ///
    /// Returns `(ConfigStore, Option<reload_interval>)`.
    pub fn build_from_value(self, value: &Value) -> Result<(ConfigStore, Option<Duration>), String> {
        let mut store = ConfigStore::new();
        for register in self.bindings {
            register(&mut store, value)?;
        }
        Ok((store, self.reload_interval))
    }
}

/// Reads and parses a config file into `serde_json::Value`.
///
/// Supports `.toml` and `.json` by file extension.
pub(crate) fn parse_config_file(path: &str) -> Result<Value, String> {
    let contents = std::fs::read_to_string(path)
        .map_err(|e| format!("config: cannot read file '{path}': {e}"))?;

    if path.ends_with(".toml") {
        let table: toml::Value = contents
            .parse()
            .map_err(|e| format!("config: TOML parse error in '{path}': {e}"))?;
        serde_json::to_value(table)
            .map_err(|e| format!("config: TOML → JSON conversion error: {e}"))
    } else if path.ends_with(".json") {
        serde_json::from_str(&contents)
            .map_err(|e| format!("config: JSON parse error in '{path}': {e}"))
    } else {
        Err(format!(
            "config: unsupported file format for '{path}' (use .toml or .json)"
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;
    use std::io::Write;

    #[derive(Debug, Deserialize, PartialEq)]
    struct Db {
        url: String,
    }

    fn write_toml(content: &str) -> tempfile::NamedTempFile {
        let mut f = tempfile::NamedTempFile::with_suffix(".toml").unwrap();
        f.write_all(content.as_bytes()).unwrap();
        f
    }

    #[test]
    fn builder_from_toml_required_section() {
        let file = write_toml("[db]\nurl = \"postgres://localhost/test\"");
        let builder = ConfigBuilder::new()
            .from_file(file.path().to_str().unwrap())
            .bind_section::<Db>("db");
        let json = builder.load_file().unwrap();
        let (store, _) = builder.build_from_value(&json).unwrap();
        let arc = store.get::<Db>().unwrap();
        assert_eq!(arc.url, "postgres://localhost/test");
    }

    #[test]
    fn builder_from_json_required_section() {
        let mut f = tempfile::NamedTempFile::with_suffix(".json").unwrap();
        f.write_all(br#"{"db": {"url": "mysql://localhost/test"}}"#)
            .unwrap();
        let builder = ConfigBuilder::new()
            .from_file(f.path().to_str().unwrap())
            .bind_section::<Db>("db");
        let json = builder.load_file().unwrap();
        let (store, _) = builder.build_from_value(&json).unwrap();
        assert_eq!(store.get::<Db>().unwrap().url, "mysql://localhost/test");
    }

    #[test]
    fn builder_optional_section_missing_is_ok() {
        let file = write_toml("");
        let builder = ConfigBuilder::new()
            .from_file(file.path().to_str().unwrap())
            .bind_section_optional::<Db>("db");
        let json = builder.load_file().unwrap();
        let (store, _) = builder.build_from_value(&json).unwrap();
        assert!(store.get::<Db>().is_none());
    }

    #[test]
    fn builder_required_section_missing_errors() {
        let file = write_toml("");
        let builder = ConfigBuilder::new()
            .from_file(file.path().to_str().unwrap())
            .bind_section::<Db>("db");
        let json = builder.load_file().unwrap();
        let result = builder.build_from_value(&json);
        assert!(result.is_err());
    }

    #[test]
    fn reload_on_change_sets_interval() {
        let file = write_toml("");
        let builder = ConfigBuilder::new()
            .from_file(file.path().to_str().unwrap())
            .reload_on_change();
        let json = builder.load_file().unwrap();
        let (_, interval) = builder.build_from_value(&json).unwrap();
        assert!(interval.is_some());
    }

    #[test]
    fn reload_without_file_is_error() {
        let builder = ConfigBuilder::new().reload_on_change();
        assert!(builder.load_file().is_err());
    }
}
