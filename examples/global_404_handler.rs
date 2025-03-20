//! Run with:
//!
//! ```no_rust
//! cargo run --example global_404_handler --features tracing
//! ```

use volga::{App, http::{Method, Uri}, html};
use tracing_subscriber::prelude::*;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .init();

    let mut app = App::new();

    app.map_get("/", || async {});

    // Enabling global 404 handler
    app.map_fallback(|uri: Uri, method: Method| async move {
        tracing::debug!("route not found {} {}", method, uri);
        html!(r#"
            <!doctype html>
            <html>
                <head>Not Found!</head>
            </html>
            "#)
    });

    app.run().await
}