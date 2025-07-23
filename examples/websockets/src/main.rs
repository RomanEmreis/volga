//! Run with:
//!
//! ```no_rust
//! cargo run -p websockets
//! ```

use volga::{
    App,
    HttpResult,
    Json,
    di::Dc,
    ws::{WebSocketConnection, WebSocket}
};

use std::sync::{Arc, RwLock};
use tracing_subscriber::prelude::*;

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

    // Complete handler WebSockets with DI
    app.map_conn("/ws", handle_ws);

    // Simplified WebSockets handler with DI
    app.map_ws("/ws1", |ws: WebSocket, counter: Dc<Counter>| async move {
        // Split socket into sender and receiver that can be used separately
        let (mut sender, mut receiver) = ws.split();

        tokio::task::spawn(async move {
            let _ = sender.send("Hello from WebSockets server!").await;
        });

        tokio::task::spawn(async move {
            while let Some(Ok(msg)) = receiver.recv::<String>().await {
                let value = inc(&counter).await;
                tracing::info!("received: {msg}; msg #{value}")
            }
        });
    });

    // Simplified JSON message handler with DI
    app.map_msg("/ws2", |msg: Json<Msg>, counter: Dc<Counter>| async move {
        let value = inc(&counter).await;
        format!("received: {}; msg #{value}", msg.text)
    });

    // Handle errors globally
    app.map_err(|err| async move {
        tracing::error!("{:?}", err);
    });

    app.run().await
}

async fn handle_ws(conn: WebSocketConnection, counter: Dc<Counter>) -> HttpResult {
    // Here can be configured a connection and extracted something from DI or HTTP metadata
    conn.with_protocols(["foo-ws"])
        .on(|ws| handle(ws, counter))
}

async fn handle(mut ws: WebSocket, counter: Dc<Counter>) {
    ws.on_msg(move |msg: String| handle_message(msg, counter.clone())).await;
}

async fn handle_message(msg: String, counter: Dc<Counter>) -> String {
    let value = inc(&counter).await;
    format!("received: {msg}; msg #{value}")
}

async fn inc(counter: &Counter) -> i32 {
    let Ok(mut value) = counter.write() else { unreachable!() };
    *value += 1;
    *value
}