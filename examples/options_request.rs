﻿//! Run with:
//!
//! ```no_rust
//! cargo run --example options_request
//! ```

use volga::{App, ok};

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let mut app = App::new();

    // OPTIONS /hello
    app.map_options("/hello", || async {
        ok!([
            ("Allow", "GET, OPTIONS")
        ])
    });

    // GET /hello
    app.map_get("/hello", || async {
        ok!("Hello World!")
    });

    app.run().await
}