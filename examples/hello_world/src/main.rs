//! Run with:
//!
//! ```no_rust
//! cargo run -p hello_world
//! ```

use volga::{App, ok};

#[tokio::main]
async fn main() -> std::io::Result<()> {
    // Start the server
    let mut app = App::new();

    // Example of an asynchronous request handler
    app.map_get("/hello", async || "Hello World!");

    // Example of a synchronous request handler: nothing to await, so no future to build
    app.map_get("/hello/{name}", |name: String| ok!("Hello {name}!"));

    app.run().await
}
