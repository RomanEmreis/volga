//! Run with:
//!
//! ```no_rust
//! cargo run -p sse
//! ```

use serde::Serialize;
use std::time::Duration;
use volga::{App, http::sse, sse_stream};

#[derive(Serialize)]
struct SseResponse {
    data: String,
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let mut app = App::new();

    app.map_get("/events", async || {
        sse_stream! {
            loop {
                yield sse::Message::new()
                    .comment("A greeting message")
                    .event("greet")
                    .json(SseResponse { data: "Hello world!".to_string() })
                    .retry(Duration::from_secs(3));

                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }
    });

    app.run().await
}
