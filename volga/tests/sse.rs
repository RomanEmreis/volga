#![allow(missing_docs)]
#![cfg(feature = "test")]

use futures_util::stream::{repeat_with, StreamExt};
use volga::{sse, sse_stream};
use volga::error::Error;
use volga::test::TestServer;
use volga::http::sse::Message;

#[tokio::test]
async fn it_tests_sse_text_stream() {
    let server = TestServer::spawn(|app| {
        app.map_get("/events", || async {
            let stream = repeat_with(|| "data: Pass!\n\n")
                .map(Ok::<_, Error>)
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
async fn it_tests_sse_message_response() {
    let server = TestServer::spawn(|app| {
        app.map_get("/events", || async {
            Message::new()
                .comment("test")
                .data("Pass!")
                .once()
        });
    }).await;

    let response = server.client()
        .get(server.url("/events"))
        .send()
        .await
        .unwrap();

    assert!(response.status().is_success());
    assert_eq!(response.text().await.unwrap(), ": test\ndata: Pass!\n\n");

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

#[tokio::test]
async fn it_tests_sse_stream_result_response() {
    let server = TestServer::spawn(|app| {
        app.map_get("/events", async || sse_stream! {
            for _ in 0..2 {
                get_result()?;
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

#[tokio::test]
async fn it_tests_sse_stream_error_response() {
    let server = TestServer::spawn(|app| {
        app.map_get("/events", async || sse_stream! {
            for _ in 0..2 {
                get_error()?;
                yield Message::new().data("Pass!");
            }
        });
    }).await;

    let response = server.client()
        .get(server.url("/events"))
        .send()
        .await;

    assert!(response.is_err());

    server.shutdown().await;
}

#[tokio::test]
#[allow(clippy::never_loop)]
async fn it_tests_sse_stream_return_err_response() {
    let server = TestServer::spawn(|app| {
        app.map_get("/events", async || sse_stream! {
            loop {
                get_result()?;
                Err(Error::client_error("test error"))?;
                
                yield Message::new().data("Pass!");
            }
        });
    }).await;

    let response = server.client()
        .get(server.url("/events"))
        .send()
        .await;

    assert!(response.is_err());

    server.shutdown().await;
}

fn get_result() -> Result<(), Error> { 
    Ok(())
}

fn get_error() -> Result<(), Error> { 
    Err(Error::client_error("test error"))
}