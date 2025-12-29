//! Run with:
//!
//! ```no_rust
//! cargo run -p rate_limiting
//! ```

use volga::{App, rate_limiting::{FixedWindow, SlidingWindow, Key}};
use std::time::Duration;

fn main() {
    let mut app = App::new()
        .with_fixed_window(FixedWindow::new(10, Duration::from_secs(30)))
        .with_sliding_window(SlidingWindow::new(10, Duration::from_secs(30)));

    app.map_get("/fixed", async || "Hello from fixed window!")
        .fixed_window(Key::Ip);

    app.map_get("/sliding", async || "Hello from sliding window!")
        .sliding_window(Key::Ip);

    app.run_blocking();
}