use volga::{App, tracing::TracingConfig};
use tracing::trace;
use tracing_subscriber::prelude::*;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    // Configuring tracing output to the stdout
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .init();
    
    // Configure tracing parameters
    let tracing = TracingConfig::new()
        .with_header()
        .with_header_name("x-span-id");
    
    let mut app = App::new()
        .with_tracing(tracing);
    
    // Enable tracing middleware
    app.use_tracing();

    app.map_get("/tracing", || async {
        trace!("handling the request!");
        "Done!"
    });

    app.run().await
}