//! Run with:
//!
//! ```no_rust
//! cargo run -p global_error_handler
//! ```

use volga::{App, status};
use std::io::Error;
use tracing_subscriber::prelude::*;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .init();

    let mut app = App::new()
        .with_tracing(|tracing| tracing.with_header());

    app.use_tracing();

    app.map_get("/error", || async {
        tracing::trace!("producing error");
        Error::other("some error")
    });

    // Enabling global error handler
    app.map_err(|error: volga::error::Error| async move {
        tracing::error!("{:?}", error);
        status!(error.status.as_u16(), "{error}")
    });   

    app.run().await
}