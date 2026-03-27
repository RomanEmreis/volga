//! Run with:
//!
//! ```no_rust
//! cargo run -p config
//! ```

use volga::{App, Config, ok};

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let mut app = App::new().with_config(|cfg| {
        cfg.with_file("examples/config/app_config.toml")
            .bind_section::<HandlerConfig>("handler")
    });

    app.map_get(
        "/hello",
        async |cfg: Config<HandlerConfig>| ok!(fmt: "Hello, {}!", cfg.msg),
    );

    app.run().await
}

#[derive(serde::Deserialize)]
struct HandlerConfig {
    msg: String,
}
