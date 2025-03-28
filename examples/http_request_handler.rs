﻿//! Run with:
//!
//! ```no_rust
//! cargo run --example http_request_handler
//! ```

use volga::{App, HttpRequest, ok};

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let mut app = App::new();

    // Example of request handler that takes the whole request
    app.map_get("/hello", |req: HttpRequest| async move {
        ok!("Request: {} {}", req.method(), req.uri())
    });

    app.run().await
}