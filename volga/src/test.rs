//! Test utilities for building and running Volga applications in integration tests.
//!
//! This module provides helpers for spinning up a fully functional Volga server
//! in an isolated environment and interacting with it using an HTTP client.
//!
//! # Purpose
//!
//! The primary goal of this module is to make integration testing of Volga
//! applications simple, reliable, and deterministic.
//!
//! Unlike unit tests, integration tests often need to verify behavior that
//! spans multiple layers, such as:
//!
//! - middleware execution order
//! - request / response headers
//! - authentication and authorization
//! - CORS configuration
//! - routing and method handling
//!
//! Spawning a real HTTP server for each test is often the simplest and most
//! reliable way to test these scenarios.
//!
//! # Design
//!
//! - Each [`TestServer`] instance binds to a randomly assigned free port.
//! - The server is started in the background and shut down gracefully.
//! - No global state is shared between tests.
//!
//! # Feature flag
//!
//! This module is available behind the `test` feature and is intended to be used
//! from integration and end-to-end tests:
//!
//! ```toml
//! [dev-dependencies]
//! volga = { version = "...", features = ["test"] }
//! ```
//!
//! # Example
//!
//! ```no_run
//! use volga::test::TestServer;
//!
//! #[tokio::test]
//! async fn health_check() {
//!     let server = TestServer::builder()
//!         .setup(|app| {
//!             app.map_get("/health", || async { "ok" });
//!         })
//!         .build()
//!         .await;
//!
//!     let response = server
//!         .client()
//!         .get(server.url("/health"))
//!         .send()
//!         .await
//!         .unwrap();
//!
//!     assert!(response.status().is_success());
//!
//!     server.shutdown().await;
//! }
//! ```
//! ## File system utilities
//!
//! The module also provides helpers for working with temporary files,
//! such as [`TempFile`], which is useful when testing file uploads
//! or filesystem-backed APIs.
//!
//! [`TestServer`]: crate::test::TestServer


pub use server::{TestServer, TestServerBuilder};
pub use fs::TempFile;

#[cfg(feature = "ws")]
pub use ws::TestWebSocket;

pub mod server;
pub mod fs;
#[cfg(feature = "ws")]
pub mod ws;