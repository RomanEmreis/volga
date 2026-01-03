#![allow(missing_docs)]

use volga::ok;

mod common;
use common::TestServer;

#[tokio::test]
async fn it_returns_brotli_compressed() {
    let server = TestServer::spawn(|app| {
        app.use_compression();
        app.map_get("/compressed", || async {
            let values= get_test_data();
            ok!(values)
        });
    }).await;

    let response = server.client()
        .get(server.url("/compressed"))
        .header("accept-encoding", "br")
        .send()
        .await
        .unwrap();

    assert_eq!(response.headers().get("vary").unwrap(), "accept-encoding");
    assert_eq!(response.json::<Vec<serde_json::Value>>().await.unwrap(), get_test_data());

    server.shutdown().await;
}

#[tokio::test]
async fn it_returns_brotli_compressed_for_route() {
    let server = TestServer::spawn(|app| {
        app.map_get("/compressed", || async {
            let values= get_test_data();
            ok!(values)
        }).with_compression();
    }).await;

    let response = server.client()
        .get(server.url("/compressed"))
        .header("accept-encoding", "br")
        .send()
        .await
        .unwrap();

    assert_eq!(response.headers().get("vary").unwrap(), "accept-encoding");
    assert_eq!(response.json::<Vec<serde_json::Value>>().await.unwrap(), get_test_data());

    server.shutdown().await;
}

#[tokio::test]
async fn it_returns_brotli_compressed_for_group() {
    let server = TestServer::spawn(|app| { 
        app.group("/tests", |api| {
            api.with_compression();
            
            api.map_get("/compressed", || async {
                let values= get_test_data();
                ok!(values)
            });
        });
    }).await;

    let response = server.client()
        .get(server.url("/tests/compressed"))
        .header("accept-encoding", "br")
        .send()
        .await
        .unwrap();

    assert_eq!(response.headers().get("vary").unwrap(), "accept-encoding");
    assert_eq!(response.json::<Vec<serde_json::Value>>().await.unwrap(), get_test_data());
    
    server.shutdown().await;
}

#[tokio::test]
async fn it_returns_gzip_compressed() {
    let server = TestServer::spawn(|app| {
        app.use_compression();
        app.map_get("/compressed", || async {
            let values= get_test_data();
            ok!(values)
        });
    }).await;

    let response = server.client()
        .get(server.url("/compressed"))
        .header("accept-encoding", "gzip")
        .send()
        .await
        .unwrap();

    assert_eq!(response.headers().get("vary").unwrap(), "accept-encoding");
    assert_eq!(response.json::<Vec<serde_json::Value>>().await.unwrap(), get_test_data());
    
    server.shutdown().await;
}

#[tokio::test]
async fn it_returns_deflate_compressed() {
    let server = TestServer::spawn(|app| {
        app.use_compression();
        app.map_get("/compressed", || async {
            let values= get_test_data();
            ok!(values)
        });
    }).await;

    let response = server.client()
        .get(server.url("/compressed"))
        .header("accept-encoding", "deflate")
        .send()
        .await
        .unwrap();

    assert_eq!(response.headers().get("vary").unwrap(), "accept-encoding");
    assert_eq!(response.json::<Vec<serde_json::Value>>().await.unwrap(), get_test_data());
    
    server.shutdown().await;
}

#[tokio::test]
async fn it_returns_zstd_compressed() {
    let server = TestServer::spawn(|app| {
        app.use_compression();
        app.map_get("/compressed", || async {
            let values= get_test_data();
            ok!(values)
        });   
    }).await;

    let response = server.client()
        .get(server.url("/compressed"))
        .header("accept-encoding", "zstd")
        .send()
        .await
        .unwrap();

    assert_eq!(response.headers().get("vary").unwrap(), "accept-encoding");
    assert_eq!(response.json::<Vec<serde_json::Value>>().await.unwrap(), get_test_data());
    
    server.shutdown().await;
}

#[tokio::test]
async fn it_returns_multiple_default_quality_compressed() {
    let server = TestServer::spawn(|app| {
        app.use_compression();
        app.map_get("/compressed", || async {
            let values= get_test_data();
            ok!(values)
        });
    }).await;

    let response = server.client()
        .get(server.url("/compressed"))
        .header("accept-encoding", "br, gzip, zstd")
        .send()
        .await
        .unwrap();

    assert_eq!(response.headers().get("vary").unwrap(), "accept-encoding");
    assert_eq!(response.json::<Vec<serde_json::Value>>().await.unwrap(), get_test_data());
    
    server.shutdown().await;
}

#[tokio::test]
async fn it_returns_multiple_different_quality_compressed() {
    let server = TestServer::spawn(|app| {
        app.use_compression();
        app.map_get("/compressed", || async {
            let values= get_test_data();
            ok!(values)
        });
    }).await;

    let response = server.client()
        .get(server.url("/compressed"))
        .header("accept-encoding", "br;q=0.9, gzip;q=1, zstd;q=0.8")
        .send()
        .await
        .unwrap();

    assert_eq!(response.headers().get("vary").unwrap(), "accept-encoding");
    assert_eq!(response.json::<Vec<serde_json::Value>>().await.unwrap(), get_test_data());
    
    server.shutdown().await;
}

#[tokio::test]
async fn it_returns_uncompressed() {
    let server = TestServer::spawn(|app| {
        app.use_compression();
        app.map_get("/compressed", || async {
            let values= get_test_data();
            ok!(values)
        });   
    }).await;

    let response = server.client()
        .get(server.url("/compressed"))
        .header("accept-encoding", "identity")
        .send()
        .await
        .unwrap();

    assert_eq!(response.headers().get("vary").unwrap(), "accept-encoding");
    assert_eq!(response.json::<Vec<serde_json::Value>>().await.unwrap(), get_test_data());
    
    server.shutdown().await;
}

#[tokio::test]
async fn it_returns_default_brotli_compressed() {
    let server = TestServer::spawn(|app| {
        app.use_compression();
        app.map_get("/compressed", || async {
            let values= get_test_data();
            ok!(values)
        });   
    }).await;

    let response = server.client()
        .get(server.url("/compressed"))
        .header("accept-encoding", "*")
        .send()
        .await
        .unwrap();

    assert_eq!(response.headers().get("vary").unwrap(), "accept-encoding");
    assert_eq!(response.json::<Vec<serde_json::Value>>().await.unwrap(), get_test_data());
    
    server.shutdown().await;
}

fn get_test_data() -> Vec<serde_json::Value> {
    let mut values: Vec<serde_json::Value> = Vec::new();
    for i in 0..10000 {
        values.push(serde_json::json!({ "age": i, "name": i.to_string() }));
    }
    values
}