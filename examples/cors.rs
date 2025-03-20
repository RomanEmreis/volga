//! Run with:
//! 
//! ```no_rust
//! cargo run --example cors --features middleware,static-files,tracing
//! ```

use volga::{App, Form};
use volga::http::Method;
use tracing_subscriber::prelude::*;

#[derive(serde::Deserialize)]
struct User {
    name: String,
    email: String,
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .init();
    
    let mut app = App::new()
        .bind("127.0.0.1:7878")
        .with_host_env(|env| env
            .with_content_root("examples/cors"))
        .with_cors(|cors| cors
            .with_origins(["http://127.0.0.1:7878"])
            .with_any_header()
            .with_methods([Method::GET, Method::POST]));
    
    app.use_cors();
    app.use_static_files();

    app.map_post("/", |user: Form<User>| async move { 
        tracing::info!("submitted: {}: {}", user.name, user.email);
    });

    app.run().await
}