//! Run with:
//!
//! ```no_rust
//! cargo run -p static_files
//! ```

use volga::App;
use tracing_subscriber::prelude::*;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .init();

    let mut app = App::new()
        .with_host_env(|env| env
            .with_content_root("static")
            .with_fallback_file("404.html")
            .with_files_listing());

    // Configures static web server 
    // - redirects from "/" -> "/index.html" if presents
    // - redirects from "/{file_name}" -> "/file-name.ext"
    // - redirects to 404.html if an unspecified route is requested
    app.use_static_files();

    app.run().await
}