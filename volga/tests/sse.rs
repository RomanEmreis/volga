#![allow(missing_docs)]
#![cfg(feature = "test")]

use futures_util::stream::{repeat_with};
use tokio_stream::StreamExt;
use volga::sse;
use volga::error::Error;
use volga::test::TestServer;

#[tokio::test]
async fn it_adds_access_control_allow_origin_header() {
    let server = TestServer::spawn(|app| {
        app.map_get("/events", || async {
            let stream = repeat_with(|| "data: Pass!\n\n")
                .map(Ok::<&str, Error>)
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