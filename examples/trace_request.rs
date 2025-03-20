//! Run with:
//!
//! ```no_rust
//! cargo run --example trace_request --features tracing
//! ```

use volga::{App, HttpRequest, stream};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> std::io::Result<()> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .init();
    
    let mut app = App::new();

    // Example of TRACE handler
    app.map_trace("/", |req: HttpRequest| async move {
        let boxed_body = req.into_boxed_body();
        stream!(boxed_body, [
            ("content-type", "message/http")
        ])
    });

    app.run().await
}