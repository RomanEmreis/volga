use volga::{App, problem};
use std::io::{Error, ErrorKind};
use tracing_subscriber::prelude::*;
use volga::tracing::TracingConfig;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .init();
    
    let mut app = App::new()
        .with_tracing(TracingConfig::new().with_header());
    
    app.use_tracing();
    
    app.map_get("/error", || async {
        tracing::trace!("producing error");
        Error::new(ErrorKind::Other, "some error")
    });

    // Enabling global error handler
    app.map_err(|error| async move {
        tracing::error!("{:?}", error);
        problem! {
            "status": 500,
            "detail": (error.to_string()),
            "prop": "some val"
        }
    });

    app.run().await
}