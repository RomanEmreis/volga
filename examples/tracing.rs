//! Run with:
//!
//! ```no_rust
//! cargo run --example tracing --features tracing
//! ```

use volga::App;
use tracing::trace;
use tracing_subscriber::prelude::*;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    // Configuring tracing output to the stdout
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .init();
    
    let mut app = App::new()
        .with_tracing(|tracing| tracing
            .with_header()
            .with_header_name("x-span-id"));

    // this middleware won't be in the request span scope 
    // since it's defined above the tracing middleware
    app.wrap(|ctx, next| async move {
        trace!("inner middleware");
        next(ctx).await
    });
    
    // Enable tracing middleware
    app.use_tracing();

    // this middleware will be in the request span scope 
    // since it's defined below the tracing middleware
    app.wrap(|ctx, next| async move {
        trace!("inner middleware");
        next(ctx).await
    });
    
    app.map_get("/tracing", || async {
        trace!("handling the request!");
        "Done!"
    });

    app.run().await
}