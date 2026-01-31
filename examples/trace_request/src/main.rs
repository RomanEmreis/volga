//! Run with:
//!
//! ```no_rust
//! cargo run -p trace_request
//! ```

use volga::{App, http::HttpBodyStream, stream};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> std::io::Result<()> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .init();

    let mut app = App::new();

    // Example of TRACE handler
    app.map_trace("/", |body_stream: HttpBodyStream| async move {
        stream!(body_stream; [
            ("content-type", "message/http")
        ])
    });

    app.run().await
}