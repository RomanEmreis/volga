#![allow(missing_docs)]

use volga::App;
use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::{
    Message,
    Utf8Bytes
};

#[tokio::test]
#[cfg(all(feature = "http1", not(feature = "http2")))]
async fn it_works_with_http1() {
    use hyper::Uri;
    use tokio_tungstenite::tungstenite::ClientRequestBuilder;

    tokio::spawn(async {
        let mut app = App::new().bind("127.0.0.1:7928");
        app.map_msg("/ws", |msg: String| async { msg });
        app.run().await
    });
    
    let response = tokio::spawn(async {
        let req = ClientRequestBuilder::new(Uri::from_static("ws://127.0.0.1:7928/ws"));
        let (mut ws, _) = tokio_tungstenite::connect_async(req)
            .await
            .unwrap();

        let input = Message::Text(Utf8Bytes::from_static("Pass!"));
        ws.send(input.clone()).await.unwrap();
        ws.next().await.unwrap().unwrap()
    }).await.unwrap();

    assert_eq!(response, Message::Text(Utf8Bytes::from_static("Pass!")));
}

#[tokio::test]
#[cfg(feature = "http2")]
async fn it_works_with_http2() {
    use hyper::{Request, Method};
    use hyper_util::rt::{TokioExecutor, TokioIo};
    use tokio::net::TcpStream;
    use tokio_tungstenite::{WebSocketStream, tungstenite::protocol};
    use volga::HttpBody;

    tokio::spawn(async {
        let mut app = App::new()
            .bind("127.0.0.1:7929");
        app.map_msg("/ws", |msg: String| async { msg });
        app.run().await
    });
    
    let response = tokio::spawn(async {
        let io = TokioIo::new(TcpStream::connect("127.0.0.1:7929").await.unwrap());
        let (mut send_request, conn) =
            hyper::client::conn::http2::Builder::new(TokioExecutor::new())
                .handshake(io)
                .await
                .unwrap();

        tokio::spawn(async {
            conn.await.unwrap();
        });

        let req = Request::builder()
            .method(Method::CONNECT)
            .extension(hyper::ext::Protocol::from_static("websocket"))
            .uri("/ws")
            .body(HttpBody::empty())
            .unwrap();

        let mut response = send_request.send_request(req).await.unwrap();
        let upgraded = hyper::upgrade::on(&mut response).await.unwrap();
        let upgraded = TokioIo::new(upgraded);
        let mut ws = WebSocketStream::from_raw_socket(upgraded, protocol::Role::Client, None).await;

        let input = Message::Text(Utf8Bytes::from_static("Pass!"));
        ws.send(input.clone()).await.unwrap();
        ws.next().await.unwrap().unwrap()
    }).await.unwrap();

    assert_eq!(response, Message::Text(Utf8Bytes::from_static("Pass!")));
}

#[tokio::test]
#[cfg(feature = "http2")]
async fn it_works_with_split_with_http2() {
    use hyper::{Request, Method};
    use hyper_util::rt::{TokioExecutor, TokioIo};
    use tokio::net::TcpStream;
    use tokio_tungstenite::{WebSocketStream, tungstenite::protocol};
    use volga::HttpBody;
    use volga::ws::WebSocket;

    tokio::spawn(async {
        let mut app = App::new()
            .bind("127.0.0.1:7934");
        app.map_ws("/ws", |ws: WebSocket| async move {
            let (mut write, mut read) = ws.split();
            while let Some(Ok(msg)) = read.recv::<String>().await {
                write.send(msg).await.unwrap();
            }
        });
        app.run().await
    });

    let response = tokio::spawn(async {
        let io = TokioIo::new(TcpStream::connect("127.0.0.1:7934").await.unwrap());
        let (mut send_request, conn) =
            hyper::client::conn::http2::Builder::new(TokioExecutor::new())
                .handshake(io)
                .await
                .unwrap();

        tokio::spawn(async {
            conn.await.unwrap();
        });

        let req = Request::builder()
            .method(Method::CONNECT)
            .extension(hyper::ext::Protocol::from_static("websocket"))
            .uri("/ws")
            .body(HttpBody::empty())
            .unwrap();

        let mut response = send_request.send_request(req).await.unwrap();
        let upgraded = hyper::upgrade::on(&mut response).await.unwrap();
        let upgraded = TokioIo::new(upgraded);
        let mut ws = WebSocketStream::from_raw_socket(upgraded, protocol::Role::Client, None).await;

        let input = Message::Text(Utf8Bytes::from_static("Pass!"));
        ws.send(input.clone()).await.unwrap();
        ws.next().await.unwrap().unwrap()
    }).await.unwrap();

    assert_eq!(response, Message::Text(Utf8Bytes::from_static("Pass!")));
}

#[tokio::test]
#[cfg(all(feature = "http1", not(feature = "http2")))]
async fn it_works_with_split_with_http1() {
    use hyper::Uri;
    use tokio_tungstenite::tungstenite::ClientRequestBuilder;
    use volga::ws::WebSocket;

    tokio::spawn(async {
        let mut app = App::new().bind("127.0.0.1:7935");
        app.map_ws("/ws", |ws: WebSocket| async move {
            let (mut write, mut read) = ws.split();
            while let Some(Ok(msg)) = read.recv::<String>().await {
                write.send(msg).await.unwrap();
            }
        });
        app.run().await
    });

    let response = tokio::spawn(async {
        let req = ClientRequestBuilder::new(Uri::from_static("ws://127.0.0.1:7935/ws"));
        let (mut ws, _) = tokio_tungstenite::connect_async(req)
            .await
            .unwrap();

        let input = Message::Text(Utf8Bytes::from_static("Pass!"));
        ws.send(input.clone()).await.unwrap();
        ws.next().await.unwrap().unwrap()
    }).await.unwrap();

    assert_eq!(response, Message::Text(Utf8Bytes::from_static("Pass!")));
}

#[tokio::test]
#[cfg(feature = "http2")]
async fn it_works_with_custom_protocol_with_http2() {
    use hyper::{Request, Method};
    use hyper_util::rt::{TokioExecutor, TokioIo};
    use tokio::net::TcpStream;
    use tokio_tungstenite::{WebSocketStream, tungstenite::protocol};
    use volga::HttpBody;
    use volga::ws::{WebSocketConnection, WebSocket};
    use volga::headers::SEC_WEBSOCKET_PROTOCOL;

    tokio::spawn(async {
        let mut app = App::new()
            .bind("127.0.0.1:7936");
        app.map_conn("/ws", |conn: WebSocketConnection| async { 
            conn.with_protocols(["foo-ws"]).on(|ws: WebSocket| async {
                let protocol = ws.protocol().unwrap().to_str().unwrap().to_string();
                let (mut write, mut read) = ws.split();
                while let Some(Ok(msg)) = read.recv::<String>().await {
                    write.send(format!("[{protocol}]: {msg}")).await.unwrap();
                }
            })
        });
        app.run().await
    });

    let response = tokio::spawn(async {
        let io = TokioIo::new(TcpStream::connect("127.0.0.1:7936").await.unwrap());
        let (mut send_request, conn) =
            hyper::client::conn::http2::Builder::new(TokioExecutor::new())
                .handshake(io)
                .await
                .unwrap();

        tokio::spawn(async {
            conn.await.unwrap();
        });

        let req = Request::builder()
            .method(Method::CONNECT)
            .extension(hyper::ext::Protocol::from_static("websocket"))
            .header(SEC_WEBSOCKET_PROTOCOL, "foo-ws")
            .uri("/ws")
            .body(HttpBody::empty())
            .unwrap();

        let mut response = send_request.send_request(req).await.unwrap();
        let upgraded = hyper::upgrade::on(&mut response).await.unwrap();
        let upgraded = TokioIo::new(upgraded);
        let mut ws = WebSocketStream::from_raw_socket(upgraded, protocol::Role::Client, None).await;

        let input = Message::Text(Utf8Bytes::from_static("Pass!"));
        ws.send(input.clone()).await.unwrap();
        ws.next().await.unwrap().unwrap()
    }).await.unwrap();

    assert_eq!(response, Message::Text(Utf8Bytes::from_static("[foo-ws]: Pass!")));
}

#[tokio::test]
#[cfg(all(feature = "http1", not(feature = "http2")))]
async fn it_works_with_custom_protocol_with_http1() {
    use hyper::Uri;
    use tokio_tungstenite::tungstenite::ClientRequestBuilder;
    use volga::ws::{WebSocketConnection, WebSocket};

    tokio::spawn(async {
        let mut app = App::new().bind("127.0.0.1:7937");
        app.map_conn("/ws", |conn: WebSocketConnection| async {
            conn.with_protocols(["foo-ws"]).on(|ws: WebSocket| async {
                let protocol = ws.protocol().unwrap().to_str().unwrap().to_string();
                let (mut write, mut read) = ws.split();
                while let Some(Ok(msg)) = read.recv::<String>().await {
                    write.send(format!("[{protocol}]: {msg}")).await.unwrap();
                }
            })
        });
        app.run().await
    });

    let response = tokio::spawn(async {
        let req = ClientRequestBuilder::new(Uri::from_static("ws://127.0.0.1:7937/ws"))
            .with_header("Sec-WebSocket-Protocol", "foo-ws");
        let (mut ws, _) = tokio_tungstenite::connect_async(req)
            .await
            .unwrap();

        let input = Message::Text(Utf8Bytes::from_static("Pass!"));
        ws.send(input.clone()).await.unwrap();
        ws.next().await.unwrap().unwrap()
    }).await.unwrap();

    assert_eq!(response, Message::Text(Utf8Bytes::from_static("[foo-ws]: Pass!")));
}