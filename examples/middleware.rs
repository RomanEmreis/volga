//! Run with:
//!
//! ```no_rust
//! cargo run --example middleware --features middleware
//! ```

use volga::{App, CancellationToken, ok, status};
use volga::headers::{Header, Accept};

#[tokio::main]
#[allow(clippy::all)]
async fn main() -> std::io::Result<()> {
    // Start the server
    let mut app = App::new();

    // Example of middleware
    app.with(|user_agent: Header<Accept>, token: CancellationToken, next| async move {
        if !token.is_cancelled() && *user_agent == "*/*" {
            next.await
        } else {
            status!(406)
        }
    });

    // Request handler
    app.map_get("/hello", || async {
        ok!("Hello World!")
    });

    app.run().await
}