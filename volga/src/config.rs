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
    /// **Strict:** produces a startup error if neither file exists. If you want optional
    /// file-based config (file may or may not exist), use [`App::with_config`] directly.
    pub fn with_default_config(mut self) -> Self {
        self.use_default_config = true;
        self
    }

    /// Configures file-based configuration via a builder closure.
    ///
    /// # Example
    /// ```no_run
    /// use volga::App;
    /// use serde::Deserialize;
    /// #[derive(Deserialize)] struct Database { url: String }
    ///
    /// App::new().with_config(|cfg| {
    ///     cfg.from_file("config/prod.toml")
    ///        .bind_section::<Database>("database")
    ///        .reload_on_change()
    /// });
    /// ```
    pub fn with_config<F>(mut self, f: F) -> Self
    where
        F: FnOnce(ConfigBuilder) -> ConfigBuilder,
    {
        self.config_builder = Some(f(ConfigBuilder::new()));
        self
    }
}