#![allow(missing_docs)]
#![cfg(all(feature = "test", feature = "tracing"))]

use volga::test::TestServer;
use tracing_subscriber::util::SubscriberInitExt;

#[tokio::test]
async fn it_adds_request_id() {
    let server = TestServer::builder()
        .configure(|app| {
            tracing_subscriber::registry().init();
            app.with_tracing(|tracing| tracing.with_header())
        })
        .setup(|app| {
            app.map_get("/test", async || "Pass!");
        })
        .build()
        .await;
    
    let response = server.client()
        .get(server.url("/test"))
        .send()
        .await
        .unwrap();

    assert!(response.status().is_success());
    assert!(response.headers().get("request-id").is_some());
    assert_eq!(response.text().await.unwrap(), "Pass!");
    
    server.shutdown().await;
}