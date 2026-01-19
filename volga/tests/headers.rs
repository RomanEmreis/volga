#![allow(missing_docs)]
#![cfg(feature = "test")]

use volga::ok;
use volga::headers::{Header, HttpHeaders, ContentType};
use volga::test::TestServer;

#[tokio::test]
async fn it_reads_headers() {
    let server = TestServer::spawn(|app| {
        app.map_get("/test", |headers: HttpHeaders| async move {
            ok!("{}", headers.get_raw("x-api-key").unwrap().to_str().unwrap())
        });
    }).await;

    let response = server.client()
        .get(server.url("/test"))
        .header("x-api-key", "some-api-key")
        .send()
        .await
        .unwrap();

    assert!(response.status().is_success());
    assert_eq!(response.text().await.unwrap(), "some-api-key");
    
    server.shutdown().await;
}

#[tokio::test]
async fn it_reads_specific_header() {
    let server = TestServer::spawn(|app| {
        app.map_get("/test", |content_type: Header<ContentType>| async move {
            ok!("{content_type}")
        });
    }).await;
    
    let response = server.client()
        .get(server.url("/test"))
        .header("Content-Type", "text/plain")
        .send()
        .await
        .unwrap();

    assert!(response.status().is_success());
    assert_eq!(response.text().await.unwrap(), "content-type: text/plain");
    
    server.shutdown().await;
}

#[tokio::test]
async fn it_writes_headers() {
    let server = TestServer::spawn(|app| {
        app.map_get("/test", || async move {
            ok!("ok!", [
                ("x-api-key", "some-api-key")
            ])
        });
    }).await;

    let response = server.client()
        .get(server.url("/test"))
        .send()
        .await
        .unwrap();  

    assert!(response.status().is_success());
    assert_eq!(response.headers().get("x-api-key").unwrap(), "some-api-key");
    assert_eq!(response.text().await.unwrap(), "\"ok!\"");
    
    server.shutdown().await;
}