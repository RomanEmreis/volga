//! # Volga
//!
//! > Fast, Easy, and very flexible Web Framework for Rust based on [Tokio](https://tokio.rs/) runtime and [hyper](https://hyper.rs/) for fun and painless microservices crafting.
//!
//! ## Features
//! * Supports HTTP/1 and HTTP/2
//! * Robust routing
//! * Custom middlewares
//! * Dependency Injection
//! * WebSockets and WebTransport
//! * Full [Tokio](https://tokio.rs/) compatibility
//! * Runs on stable Rust 1.80+
//! 
//! ## Example
//! ```no_run
//! use volga::*;
//! 
//! #[tokio::main]
//! async fn main() -> std::io::Result<()> {
//!     // Start the server
//!     let mut app = App::new();
//! 
//!     // Example of a request handler
//!     app.map_get("/hello/{name}", async |name: String| {
//!          ok!("Hello {name}!")
//!     });
//!     
//!     app.run().await
//! }
//! ```

mod server;
pub(crate) mod utils;

pub mod app;
pub mod http;
pub mod headers;
pub mod json;
pub mod error;
pub mod fs;
#[cfg(feature = "di")]
pub mod di;
#[cfg(feature = "middleware")]
pub mod middleware;
#[cfg(feature = "tls")]
pub mod tls;
#[cfg(feature = "tracing")]
pub mod tracing;
#[cfg(feature = "ws")]
pub mod ws;
#[cfg(any(feature = "basic-auth", feature = "jwt-auth"))]
pub mod auth;
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
    ResponseContext,
    Results
};

#[cfg(feature = "multipart")]
pub use crate::http::endpoints::args::multipart::Multipart;

/// Route mapping helpers
pub mod routing {
    pub use crate::app::router::{RouteGroup, Route};
}


