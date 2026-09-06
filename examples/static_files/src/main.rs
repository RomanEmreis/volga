//! Run with:
//!
//! ```no_rust
//! cargo run -p static_files
//! ```

use tracing_subscriber::prelude::*;
use volga::App;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .init();

    let mut app = App::new().with_host_env(|env| {
        env.with_content_root("examples/static_files/static")
            .with_fallback_file("404.html")
            .with_files_listing()
    });

    // Configures static web server
    // - answers "/" with the index file, or with a listing of the content root
    // - answers "/{path}" with the file of that name, at any depth
    // - falls back to 404.html for anything neither a file nor a route answers
    app.use_static_files();

    app.run().await
}
