//! # Volga Open API Integration
//! 
//! OpenAPI 3.0 integration for the **Volga** web framework.
//! 
//! `volga-open-api` generates OpenAPI specifications directly from your routes, extractors, 
//! and responders - without macros, codegen, or runtime reflection.
//! 
//! It is fully optional and designed to stay out of your way.

mod schema;
mod config;
mod route;
mod param;
mod op;
mod registry;
mod doc;
mod ui;

pub use {
    config::{OpenApiConfig, OpenApiSpec},
    route::{OpenApiRouteConfig, IntoStatusCode},
    registry::OpenApiRegistry,
    doc::OpenApiDocument,
    ui::ui_html,
};