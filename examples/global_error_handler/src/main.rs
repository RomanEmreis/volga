//! Run with:
//!
//! ```no_rust
//! cargo run -p global_error_handler
//! ```

use volga::{App, problem};
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
        let (status, instance, err) = error.into_parts();
        problem! {
            "status": status.as_u16(),
            "detail": (err.to_string()),
            "instance": instance,
        }
    });

    app.run().await
}