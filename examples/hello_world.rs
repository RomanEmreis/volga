//! Run with:
//!
//! ```no_rust
//! cargo run --example hello_world
//! ```

use volga::App;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    // Start the server
    let mut app = App::new();
    
    // Example of asynchronous request handler
    app.map_get("/hello", || async { 
        "Hello World!"
    });

    app.run().await
}