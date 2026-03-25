//! File-based application configuration.
//!
//! Enabled by the `config` feature (on by default).
//! See [`ConfigBuilder`] and [`Config`] for usage.

pub(crate) mod store;

pub use store::{ConfigStore, SectionKind};
