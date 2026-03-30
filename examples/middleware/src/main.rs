//! Run with:
//!
//! ```no_rust
//! cargo run -p middleware
//! ```

use std::time::Duration;
use volga::headers::{Accept, Header};
use volga::middleware::{AttachHandler, HttpContext, NextFn};
use volga::{App, CancellationToken, HttpResult, ok, status};

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

    app.attach(Timeout {
        duration: Duration::from_secs(1),
    });

    // Example of middleware
    app.with(
        |user_agent: Header<Accept>, token: CancellationToken, next| async move {
            if !token.is_cancelled() && user_agent.as_ref() == "*/*" {
                next.await
            } else {
                status!(406)
            }
        },
    );

    // Request handler
    app.map_get("/hello", || async { ok!("Hello World!") });

    app.run().await
}

struct Timeout {
    duration: Duration,
}

impl AttachHandler for Timeout {
    fn call(&self, ctx: HttpContext, next: NextFn) -> impl Future<Output = HttpResult> + 'static {
        let duration = self.duration;
        async move {
            tokio::time::sleep(duration).await;
            next(ctx).await
        }
    }
}
