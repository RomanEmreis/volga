//! Run with:
//!
//! ```no_rust
//! cargo run -p sse
//! ```

use volga::{App, error::Error, http::sse::Message, sse};
use futures_util::stream::{repeat_with};
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
        let repeater = || {
            let resp = SseResponse { data: "Hello, world!".into() };
            Message::new()
                .comment("A greeting message")
                .event("greet")
                .json(resp)
                .retry(Duration::from_secs(3))
        };
        let stream = repeat_with(repeater)
            .map(Ok::<_, Error>)
            .throttle(Duration::from_secs(1));

        sse!(stream)
    });

    app.run().await
}