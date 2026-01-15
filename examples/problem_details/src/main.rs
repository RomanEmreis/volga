//! Run with:
//!
//! ```no_rust
//! cargo run -p problem_details
//! ```

use volga::{App, error::Problem};
use std::io::Error;
use serde::Serialize;
use tracing_subscriber::prelude::*;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .init();

    let mut app = App::new()
        .with_tracing(|tracing| tracing.with_header());

    // Enabling global error handler that produces
    // error responses in Problem details format
    app.use_problem_details();  

    app.map_get("/error", || async {
        tracing::trace!("producing error");
        Error::other("some error")
    });

    app.map_get("/problem", || async {
        // Always producing a problem

        Problem::new(400)
            .with_detail("Missing Parameter")
            .with_instance("/problem")
            .with_extensions(ValidationError {
                invalid_params: vec![InvalidParam { 
                    name: "id".into(), 
                    reason: "The ID must be provided".into()
                }]
            })
    });

    app.run().await
}

#[derive(Default, Serialize)]
struct ValidationError {
    #[serde(rename = "invalid-params")]
    invalid_params: Vec<InvalidParam>,
}

#[derive(Default, Serialize)]
struct InvalidParam {
    name: String,
    reason: String,
}