//! Run with:
//!
//! ```no_rust
//! cargo run --example mcp --features mcp,tracing
//! ```

use volga::{App, ok};
use tracing_subscriber::prelude::*;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .init();

    let mut app = App::new();

    // Example of an addition tool
    app.map_tool("add", async |a: i32, b: i32| a + b);

    // Example of a dynamic greeting resource
    app.map_resource("greeting://{name}", async |name: String| {
        ok!("Hello, {name}!")
    });
    
    // Runs the server
    app.run().await
}