//! # Volga Open API Integration
//!
//! OpenAPI 3.0 integration for the **Volga** web framework.
//!
//! `volga-open-api` generates OpenAPI specifications directly from your routes, extractors,
//! and responders - without macros, codegen, or runtime reflection.
//!
//! It is fully optional and designed to stay out of your way.

mod config;
mod doc;
mod op;
mod param;
mod registry;
mod route;
mod schema;
mod ui;

pub use {
    config::{OpenApiConfig, OpenApiSpec},
    doc::OpenApiDocument,
    registry::OpenApiRegistry,
    route::{IntoStatusCode, OpenApiRouteConfig},
    ui::ui_html,
};
