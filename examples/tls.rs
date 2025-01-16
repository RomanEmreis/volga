use volga::{App, Json, tls::TlsConfig, ok};

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let mut app = App::new();
    
    app.use_tls(TlsConfig::from_pem("examples/tls")
        .with_https_redirection());

    app.map_group("/user")
        .map_get("/{name}", |name: String| async move {
            ok!("Hello {name}!")
        })
        .map_post("/create", |user: Json<serde_json::Value>| async move {
            user
        });

    app.run().await
}