//! Run with:
//!
//! ```no_rust
//! cargo run -p tracing_example
//! ```

use tracing::{error, info, trace};
use tracing_subscriber::prelude::*;
use volga::{
    App,
    error::Error,
    middleware::{HttpContext, NextFn},
    status,
};

#[tokio::main]
async fn main() -> std::io::Result<()> {
    // Configuring tracing output to the stdout
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .init();

    let mut app = App::new()
        // Configuring tracing
        .with_tracing(|tracing| tracing.with_header().with_header_name("x-span-id"));

    // Global error handler will be in the request span scope
    app.map_err(|err| async move {
        error!("{err}");
        status!(500)
    });

    // Middleware will be in the request span scope
    app.wrap(|ctx: HttpContext, next: NextFn| async move {
        trace!("trace middleware");
        next(ctx).await
    });

    // Request handlers will be in the request span scope

    app.map_get("/error", || async {
        Err::<(), _>(Error::client_error("Hello from always failing handler"))
    });

    app.map_get("/tracing", || async {
        info!("handling the request!");
        "Done!"
    });

    app.run().await
}
