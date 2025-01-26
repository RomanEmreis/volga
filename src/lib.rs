//! # Volga
//!
//! > Fast, Easy, and very flexible Web Framework for Rust based on [Tokio](https://tokio.rs/) runtime and [hyper](https://hyper.rs/) for fun and painless microservices crafting.
//!
//! ## Features
//! * Supports HTTP/1 and HTTP/2
//! * Robust routing
//! * Custom middlewares
//! * Full [Tokio](https://tokio.rs/) compatibility
//! * Runs on stable Rust 1.80+
//! 
//! ## Example
//! ```toml
//! [dependencies]
//! volga = "0.4.10"
//! tokio = { version = "1", features = ["full"] }
//! ```
//! ```no_run
//! use volga::*;
//! 
//! #[tokio::main]
//! async fn main() -> std::io::Result<()> {
//!     // Start the server
//!     let mut app = App::new();
//! 
//!     // Example of request handler
//!     app.map_get("/hello/{name}", |name: String| async move {
//!          ok!("Hello {name}!")
//!     });
//!     
//!     app.run().await
//! }
//! ```

#![forbid(unsafe_code)]
#![deny(unreachable_pub)]

mod server;

pub mod app;
pub mod http;
pub mod headers;
pub mod json;
pub mod error;
#[cfg(feature = "di")]
pub mod di;
#[cfg(feature = "middleware")]
pub mod middleware;
#[cfg(feature = "tls")]
pub mod tls;
#[cfg(feature = "tracing")]
pub mod tracing;
#[cfg(test)]
pub mod test_utils;

pub use crate::app::App;
pub use crate::http::{
    response::builder::{RESPONSE_ERROR, SERVER_NAME},
    endpoints::args::{
        cancellation_token::CancellationToken,
        file::File,
        json::Json,
        path::Path,
        query::Query,
        form::Form,
    },
    BoxBody,
    UnsyncBoxBody,
    HttpBody,
    HttpRequest,
    HttpResponse,
    HttpResult,
    HttpHeaders,
    ResponseContext,
    Results
};

#[cfg(feature = "multipart")]
pub use crate::http::endpoints::args::multipart::Multipart;

pub mod routing {
    pub use crate::app::router::RouteGroup;
}


