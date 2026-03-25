//! # Volga
//!
//! > Fast, Easy, and very flexible Web Framework for Rust based on [Tokio](https://tokio.rs/) runtime and [hyper](https://hyper.rs/) for fun and painless microservices crafting.
//!
//! ## Features
//! * Supports HTTP/1 and HTTP/2
//! * Robust routing
//! * Custom middlewares
//! * Dependency Injection
//! * WebSockets and WebSocket-over-HTTP/2
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

pub mod app;
#[cfg(any(feature = "basic-auth", feature = "jwt-auth"))]
pub mod auth;
#[cfg(feature = "config")]
pub mod config;

#[cfg(feature = "di")]
pub mod di;
pub mod error;
pub mod fs;
#[cfg(feature = "__fuzzing")]
#[doc(hidden)]
pub mod fuzzing;
pub mod headers;
pub mod http;
pub mod json;
pub mod limits;
#[cfg(feature = "middleware")]
pub mod middleware;
#[cfg(feature = "openapi")]
pub mod openapi;
#[cfg(feature = "rate-limiting")]
pub mod rate_limiting;
#[cfg(any(test, feature = "test"))]
pub mod test;
#[cfg(feature = "tls")]
pub mod tls;
#[cfg(feature = "tracing")]
pub mod tracing;
pub mod utils;
#[cfg(feature = "ws")]
pub mod ws;

pub use crate::app::App;
pub use crate::http::{
    BoxBody, HttpBody, HttpRequest, HttpResponse, HttpResult, UnsyncBoxBody,
    endpoints::args::{
        byte_stream::ByteStream,
        cancellation_token::CancellationToken,
        client_ip::ClientIp,
        file::File,
        form::Form,
        json::Json,
        path::{NamedPath, Path},
        query::Query,
    },
    response::builder::{RESPONSE_ERROR, SERVER_NAME},
};

#[cfg(feature = "middleware")]
pub use http::HttpRequestMut;

pub use limits::Limit;

#[cfg(feature = "multipart")]
pub use crate::http::endpoints::args::multipart::Multipart;

/// Route mapping helpers
pub mod routing {
    pub use crate::app::router::{Route, RouteGroup};
}

#[doc(hidden)]
pub use async_stream as __async_stream;
