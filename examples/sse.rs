//! Run with:
//!
//! ```no_rust
//! cargo run --example sse
//! ```

use volga::{App, http::sse::Event, sse};
use volga::error::Error;
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
            Event::from(resp).into()
        };
        let stream = repeat_with(repeater)
            .map(Ok::<_, Error>)
            .throttle(Duration::from_secs(1));
        
        sse!(stream)
    });

    app.run().await
}