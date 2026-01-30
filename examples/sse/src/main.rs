//! Run with:
//!
//! ```no_rust
//! cargo run -p sse
//! ```

use volga::{App, http::sse, sse};
use std::time::Duration;
use serde::Serialize;
use tokio_stream::StreamExt;

#[derive(Serialize)]
struct SseResponse {
    data: String,
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let mut app = App::new();

    app.map_get("/events", || async {
        let resp = SseResponse { data: "Hello, world!".into() };
        let stream = sse::Message::new()
            .comment("A greeting message")
            .event("greet")
            .json(resp)
            .retry(Duration::from_secs(3))
            .repeat()
            .throttle(Duration::from_secs(1));
        
        sse!(stream)
    });

    app.run().await
}