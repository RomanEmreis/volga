//! Run with:
//!
//! ```no_rust
//! cargo run --example sse
//! ```

use volga::{App, sse};
use futures_util::stream::{repeat_with};
use std::time::Duration;
use tokio_stream::StreamExt;
use volga::error::Error;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let mut app = App::new();

    app.map_get("/events", || async {
        let stream = repeat_with(|| "data: Hello, World!\n\n".into())
            .map(Ok::<_, Error>)
            .throttle(Duration::from_secs(1));
        
        sse!(stream)
    });

    app.run().await
}