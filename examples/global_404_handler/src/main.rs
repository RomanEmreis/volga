//! Run with:
//!
//! ```no_rust
//! cargo run -p global_404_handler
//! ```

use std::sync::{Arc, RwLock};
use tracing_subscriber::prelude::*;
use volga::{App, http::{Method, Uri}, di::Dc, html};

#[derive(Default, Clone, Debug)]
struct Counter(Arc<RwLock<usize>>);

#[tokio::main]
async fn main() -> std::io::Result<()> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .init();

    let mut app = App::new();

    app.add_singleton(Counter::default());
    app.map_get("/", || async {});

    // Enabling global 404 handler
    app.map_fallback(|uri: Uri, method: Method, cnt: Dc<Counter>| async move {
        *cnt.0.write().unwrap() += 1;
        tracing::debug!("route not found {} {}; attempt: #{}", method, uri, cnt.0.read().unwrap());
        html!(r#"
            <!doctype html>
            <html>
                <head>Not Found!</head>
            </html>
            "#)
    });

    app.run().await
}