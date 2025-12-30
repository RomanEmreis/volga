//! Run with:
//!
//! ```no_rust
//! cargo run -p rate_limiting
//! ```

use volga::{App, rate_limiting::{FixedWindow, SlidingWindow, by}};
use std::time::Duration;

fn main() {
    let mut app = App::new()
        .with_fixed_window(FixedWindow::new(10, Duration::from_secs(30)))
        .with_sliding_window(SlidingWindow::new(10, Duration::from_secs(30)));

    app.map_get("/fixed", async || "Hello from fixed window!")
        .fixed_window(by::ip());

    app.map_get("/sliding", async || "Hello from sliding window!")
        .sliding_window(by::ip());

    app.run_blocking();
}