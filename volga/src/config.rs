//! File-based application configuration.
//!
//! # Quick start
//! ```no_run
//! use volga::{App, Config};
//! use serde::Deserialize;
//!
//! #[derive(Deserialize)]
//! struct Database { url: String }
//!
//! #[tokio::main]
//! async fn main() -> std::io::Result<()> {
//!     let app = App::new()
//!         .with_config(|cfg| cfg.from_file("app_config.toml").bind_section::<Database>("database"));
//!     app.run().await
//! }
//! ```

pub(crate) mod builder;
pub(crate) mod extractor;
pub(crate) mod processing;
pub(crate) mod store;

pub use builder::ConfigBuilder;
pub use extractor::Config;
pub use store::{ConfigStore, SectionKind};

use crate::App;

impl App {
    /// Loads configuration from the default file (`app_config.toml` or `app_config.json`).
    ///
    /// Searches the current working directory in order: `app_config.toml`, then `app_config.json`.
    ///
    /// **Strict:** panics at startup if neither file exists or if config processing fails.
    /// If you want optional file-based config, use [`App::with_config`] directly.
    ///
    /// # Panics
    ///
    /// Panics if no default config file is found or if the config fails to load or parse.
    pub fn with_default_config(self) -> Self {
        use std::path::Path;
        let path = if Path::new("app_config.toml").exists() {
            "app_config.toml"
        } else if Path::new("app_config.json").exists() {
            "app_config.json"
        } else {
            panic!(
                "config: with_default_config() found neither app_config.toml nor app_config.json"
            );
        };
        self.process_config(ConfigBuilder::new().from_file(path))
            .unwrap_or_else(|e| panic!("config: {e}"))
    }

    /// Configures file-based configuration via a builder closure.
    ///
    /// # Example
    /// ```no_run
    /// use volga::App;
    /// use serde::Deserialize;
    /// #[derive(Deserialize)] struct Database { url: String }
    ///
    /// #[tokio::main]
    /// async fn main() -> std::io::Result<()> {
    ///     let app = App::new().with_config(|cfg| {
    ///         cfg.from_file("config/prod.toml")
    ///            .bind_section::<Database>("database")
    ///            .reload_on_change()
    ///     });
    ///     app.run().await
    /// }
    /// ```
    ///
    /// # Panics
    ///
    /// Panics if the config file cannot be read, parsed, or if any required section is missing.
    pub fn with_config<F>(self, f: F) -> Self
    where
        F: FnOnce(ConfigBuilder) -> ConfigBuilder,
    {
        self.process_config(f(ConfigBuilder::new()))
            .unwrap_or_else(|e| panic!("config: {e}"))
    }
}
