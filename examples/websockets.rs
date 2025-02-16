use volga::{App, Json};
use tracing_subscriber::prelude::*;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .init();

    let mut app = App::new();

    app.map_message("/ws", handle_msg);

    app.run().await
}

async fn handle_msg(msg: Json<Msg>) -> Json<Msg> {
    Json(Msg { text: format!("Hello, {}!", msg.text) })
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
struct Msg {  
    text: String
}