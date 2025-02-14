﻿use std::time::Duration;
use volga::{App, Json, ok};

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let mut app = App::new()
        .bind("127.0.0.1:7878")
        .with_tls_from_pem("examples/tls")
        .with_https_redirection()
        .with_http_port(7879)
        .with_hsts_max_age(Duration::from_secs(30))
        .with_hsts_exclude_hosts(&["example.com", "example.net"]);

    app.use_hsts();
    
    app.map_group("/user")
        .map_get("/{name}", |name: String| async move {
            ok!("Hello {name}!")
        })
        .map_post("/create", |user: Json<serde_json::Value>| async move {
            user
        });

    app.run().await
}