#![allow(missing_docs)]
#![cfg(all(feature = "test", feature = "ws"))]

use volga::test::TestServer;

#[tokio::test]
async fn it_works() {
    let server = TestServer::spawn(|app| {
        app.map_msg("/ws", |msg: String| async { msg });
    }).await;

    let mut ws = server.ws("/ws").await;

    ws.send_text("Pass!").await;
    let response = ws.recv_text().await;

    assert_eq!(response, "Pass!");
    
    server.shutdown().await;
}

#[tokio::test]
async fn it_works_with_split() {
    use volga::ws::{WebSocket, WsEvent};

    let server = TestServer::spawn(|app| {
        app.map_ws("/ws", |ws: WebSocket| async move {
            let (mut write, mut read) = ws.split();
            while let Some(Ok(msg)) = read.recv::<String>().await {
                match msg { 
                    WsEvent::Data(msg) => write.send(msg).await.unwrap(),
                    WsEvent::Close(frame) => write.close(frame).await.unwrap()
                }
            }
        });
    }).await;

    let mut ws = server.ws("/ws").await;

    ws.send_text("Pass!").await;
    let response = ws.recv_text().await;

    assert_eq!(response, "Pass!");

    server.shutdown().await;
}

#[tokio::test]
async fn it_works_with_custom_protocol() {
    use volga::ws::{WebSocketConnection, WebSocket, WsEvent};

    let server = TestServer::spawn(|app| {
        app.map_conn("/ws", |conn: WebSocketConnection| async {
            conn.with_protocols(["foo-ws"]).on(|ws: WebSocket| async {
                let protocol = ws.protocol().unwrap().to_str().unwrap().to_string();
                let (mut write, mut read) = ws.split();
                while let Some(Ok(msg)) = read.recv::<String>().await {
                    match msg { 
                        WsEvent::Data(msg) => write.send(format!("[{protocol}]: {msg}")).await.unwrap(),
                        WsEvent::Close(frame) => write.close(frame).await.unwrap()
                    }
                }
            })
        });
    }).await;

    let mut ws = server.ws_with_protocols("/ws", ["foo-ws"]).await;

    ws.send_text("Pass!").await;
    let response = ws.recv_text().await;

    assert_eq!(response, "[foo-ws]: Pass!");
    
    server.shutdown().await;
}

#[tokio::test]
async fn it_allows_ws_sink_stream_into_inner() {
    use futures_util::{SinkExt, StreamExt};

    let server = TestServer::spawn(|app| {
        app.map_ws("/ws", |ws: volga::ws::WebSocket| async move {
            let (write, read) = ws.split();
            let mut inner_sink = write.into_inner();
            let mut inner_stream = read.into_inner();
            if let Some(Ok(msg)) = inner_stream.next().await {
                inner_sink.send(msg).await.unwrap();
            }
        });
    }).await;

    let mut ws = server.ws("/ws").await;

    ws.send_text("hello").await;
    let response = ws.recv_text().await;

    assert_eq!(response, "hello");

    server.shutdown().await;
}

