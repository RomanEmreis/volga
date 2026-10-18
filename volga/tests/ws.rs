#![allow(missing_docs)]
#![cfg(all(feature = "test", feature = "ws"))]

use volga::test::TestServer;

#[tokio::test]
async fn it_works() {
    let server = TestServer::spawn(|app| {
        app.map_msg("/ws", |msg: String| async { msg });
    })
    .await;

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
                    WsEvent::Close(_frame) => write.close().await.unwrap(),
                    _ => unreachable!(),
                }
            }
        });
    })
    .await;

    let mut ws = server.ws("/ws").await;

    ws.send_text("Pass!").await;
    let response = ws.recv_text().await;

    assert_eq!(response, "Pass!");

    server.shutdown().await;
}

#[tokio::test]
async fn it_works_with_custom_protocol() {
    use volga::ws::{WebSocket, WebSocketConnection, WsEvent};

    let server = TestServer::spawn(|app| {
        app.map_conn("/ws", |conn: WebSocketConnection| async {
            conn.with_protocols(["foo-ws"]).on(|ws: WebSocket| async {
                let protocol = ws.protocol().unwrap().to_str().unwrap().to_string();
                let (mut write, mut read) = ws.split();
                while let Some(Ok(msg)) = read.recv::<String>().await {
                    match msg {
                        WsEvent::Data(msg) => {
                            write.send(format!("[{protocol}]: {msg}")).await.unwrap()
                        }
                        WsEvent::Close(_frame) => write.close().await.unwrap(),
                        _ => unreachable!(),
                    }
                }
            })
        });
    })
    .await;

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
    })
    .await;

    let mut ws = server.ws("/ws").await;

    ws.send_text("hello").await;
    let response = ws.recv_text().await;

    assert_eq!(response, "hello");

    server.shutdown().await;
}

#[test]
#[should_panic(
    expected = "`map_ws` at `/ws`: the handler reads 1 path parameter, but the route declares 0"
)]
fn it_panics_where_a_ws_handler_reads_a_path_param_its_route_does_not_declare() {
    let mut app = volga::App::new();

    app.map_ws("/ws", |_ws: volga::ws::WebSocket, _room: String| async {});
}

#[test]
#[should_panic(
    expected = "`map_msg` at `/ws`: the handler reads 1 path parameter, but the route declares 0"
)]
fn it_panics_where_a_msg_handler_reads_a_path_param_its_route_does_not_declare() {
    let mut app = volga::App::new();

    app.map_msg("/ws", |msg: String, room: String| async move {
        format!("{room}: {msg}")
    });
}

#[tokio::test]
async fn it_reads_a_path_param_in_a_ws_handler() {
    use volga::ws::WebSocket;

    let server = TestServer::spawn(|app| {
        app.map_ws("/ws/{room}", |mut ws: WebSocket, room: String| async move {
            ws.send(format!("welcome to {room}")).await.unwrap();
        });
    })
    .await;

    let mut ws = server.ws("/ws/lobby").await;

    assert_eq!(ws.recv_text().await, "welcome to lobby");

    server.shutdown().await;
}

#[tokio::test]
async fn it_reads_a_path_param_in_a_msg_handler() {
    let server = TestServer::spawn(|app| {
        app.map_msg("/ws/{room}", |msg: String, room: String| async move {
            format!("{room}: {msg}")
        });
    })
    .await;

    let mut ws = server.ws("/ws/lobby").await;

    ws.send_text("hi").await;
    let response = ws.recv_text().await;

    assert_eq!(response, "lobby: hi");

    server.shutdown().await;
}
