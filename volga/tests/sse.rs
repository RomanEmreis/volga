#![allow(missing_docs)]
#![cfg(feature = "test")]

use futures_util::stream::{repeat_with, StreamExt};
use volga::{sse, sse_stream};
use volga::test::TestServer;
use volga::http::sse::Message;

#[tokio::test]
async fn it_tests_sse_text_stream() {
    let server = TestServer::spawn(|app| {
        app.map_get("/events", || async {
            let stream = repeat_with(|| "data: Pass!\n\n".into())
                .take(2);
            sse!(stream)
        });
    }).await;

    let response = server.client()
        .get(server.url("/events"))
        .send()
        .await
        .unwrap();

    assert!(response.status().is_success());
    assert_eq!(response.text().await.unwrap(), "data: Pass!\n\ndata: Pass!\n\n");
    
    server.shutdown().await;
}

#[tokio::test]
async fn it_tests_sse_message_stream() {
    let server = TestServer::spawn(|app| {
        app.map_get("/events", || async {
            let stream = Message::new()
                .comment("test")
                .data("Pass!")
                .repeat()
                .take(2);
            sse!(stream)
        });
    }).await;

    let response = server.client()
        .get(server.url("/events"))
        .send()
        .await
        .unwrap();

    assert!(response.status().is_success());
    assert_eq!(response.text().await.unwrap(), ": test\ndata: Pass!\n\n: test\ndata: Pass!\n\n");

    server.shutdown().await;
}

#[tokio::test]
async fn it_tests_sse_stream_response() {
    let server = TestServer::spawn(|app| {
        app.map_get("/events", async || sse_stream! {
            for _ in 0..2 {
                yield Message::new().data("Pass!");
            }
        });
    }).await;

    let response = server.client()
        .get(server.url("/events"))
        .send()
        .await
        .unwrap();

    assert!(response.status().is_success());
    assert_eq!(response.text().await.unwrap(), "data: Pass!\n\ndata: Pass!\n\n");

    server.shutdown().await;
}