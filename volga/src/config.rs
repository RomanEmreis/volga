//! File-based application configuration.
//!
//! Enable the `config` feature (on by default) to use this module.
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
pub(crate) mod sections;
pub(crate) mod store;

pub use builder::ConfigBuilder;
pub use extractor::Config;
pub use store::{ConfigStore, SectionKind};
