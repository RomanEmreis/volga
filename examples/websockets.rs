use volga::{App, HttpResult, Json, di::Dc};
use std::sync::{Arc, RwLock};
use tracing_subscriber::prelude::*;
use volga::ws::{WebSocketConnection, WebSocket};

type Counter = Arc<RwLock<i32>>;

#[derive(serde::Deserialize, serde::Serialize)]
struct Msg {
    text: String
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .init();

    let mut app = App::new();
    
    app.add_singleton(Counter::default());
    app.map_ws("/ws", handle_ws);
    
    app.run().await
}

async fn handle_ws(conn: WebSocketConnection, counter: Dc<Counter>) -> HttpResult {
    conn.on(|ws| handle(ws, counter))
}

async fn handle(mut ws: WebSocket, counter: Dc<Counter>) {
    ws.on_msg(move |msg: Json<Msg>| handle_message(msg, counter.clone())).await;
}

async fn handle_message(msg: Json<Msg>, counter: Dc<Counter>) -> Json<Msg> {
    let Ok(mut value) = counter.write() else {
        return Json(Msg { text: "error".into() })
    };
    *value += 1;
    Json(Msg { text: format!("Hello, {}! Number is: {}", msg.text, value) })
}