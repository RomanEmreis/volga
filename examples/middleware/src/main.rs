//! Run with:
//!
//! ```no_rust
//! cargo run -p middleware
//! ```

use volga::{App, CancellationToken, ok, status};
use volga::headers::{Header, Accept};

#[tokio::main]
#[allow(clippy::all)]
async fn main() -> std::io::Result<()> {
    // Start the server
    let mut app = App::new();

    // Example of middleware
    app.wrap(|ctx, next| async move {
        // do something with the request
        let resp = next(ctx).await;
        // do something with response
        resp
    });

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