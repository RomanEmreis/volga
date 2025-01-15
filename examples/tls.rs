use volga::{App, tls::TlsConfig};

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let mut app = App::new()
        .bind_tls(TlsConfig::from_pem("examples/tls"));

    app.map_get("/hello", || async {
        "Hello World!"
    });

    app.run().await
}