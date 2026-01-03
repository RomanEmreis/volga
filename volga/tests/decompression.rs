#![allow(missing_docs)]

use volga::{Json, ok};
use serde_json::{Value, json};

use async_compression::tokio::write::{
    BrotliEncoder, 
    GzipEncoder, 
    ZlibEncoder, 
    ZstdEncoder
};
use tokio::io::AsyncWriteExt;

mod common;
use common::TestServer;

#[tokio::test]
async fn it_decompress_brotli() {
    let server = TestServer::spawn(|app| {
        app.use_decompression();
        app.map_post("/decompress", |Json(value): Json<Value>| async move {
            ok!(value)
        });
    }).await;

    let data = b"{\"age\":33,\"name\":\"John\"}";
    let mut encoder = BrotliEncoder::new(Vec::new());

    encoder.write_all(data).await.unwrap();
    encoder.shutdown().await.unwrap();
    let body = encoder.into_inner();
    
    let response = server.client()
        .post(server.url("/decompress"))
        .header("content-encoding", "br")
        .body(body)
        .send()
        .await
        .unwrap();

    assert_eq!(response.json::<Value>().await.unwrap(), json!({ "name": "John", "age": 33 }));
    
    server.shutdown().await;
}

#[tokio::test]
async fn it_decompress_brotli_for_route() {
    let server = TestServer::spawn(|app| {
        app.map_post("/decompress", |Json(value): Json<Value>| async move {
            ok!(value)
        })
        .with_decompression();
    }).await;

    let data = b"{\"age\":33,\"name\":\"John\"}";
    let mut encoder = BrotliEncoder::new(Vec::new());

    encoder.write_all(data).await.unwrap();
    encoder.shutdown().await.unwrap();
    let body = encoder.into_inner();

    let response = server.client()
        .post(server.url("/decompress"))
        .header("content-encoding", "br")
        .body(body)
        .send()
        .await
        .unwrap();

    assert_eq!(response.json::<Value>().await.unwrap(), json!({ "name": "John", "age": 33 }));
    
    server.shutdown().await;
}

#[tokio::test]
async fn it_decompress_brotli_for_group() {
    let server = TestServer::spawn(|app| {
        app.group("/tests", |api| {
            api.with_decompression();
            api.map_post("/decompress", |Json(value): Json<Value>| async move {
                ok!(value)
            });
        });
    }).await;

    let data = b"{\"age\":33,\"name\":\"John\"}";
    let mut encoder = BrotliEncoder::new(Vec::new());

    encoder.write_all(data).await.unwrap();
    encoder.shutdown().await.unwrap();
    let body = encoder.into_inner();

    let response = server.client()
        .post(server.url("/tests/decompress"))
        .header("content-encoding", "br")
        .body(body)
        .send()
        .await
        .unwrap();

    assert_eq!(response.json::<Value>().await.unwrap(), json!({ "name": "John", "age": 33 }));
    
    server.shutdown().await;
}

#[tokio::test]
async fn it_decompress_gzip() {
    let server = TestServer::spawn(|app| {
        app.use_decompression();
        app.map_post("/decompress", |Json(value): Json<Value>| async move {
            ok!(value)
        });
    }).await;

    let data = b"{\"age\":33,\"name\":\"John\"}";
    let mut encoder = GzipEncoder::new(Vec::new());

    encoder.write_all(data).await.unwrap();
    encoder.shutdown().await.unwrap();
    let body = encoder.into_inner();

    let response = server.client()
        .post(server.url("/decompress"))
        .header("content-encoding", "gzip")
        .body(body)
        .send()
        .await
        .unwrap();

    assert_eq!(response.json::<Value>().await.unwrap(), json!({ "name": "John", "age": 33 }));
    
    server.shutdown().await;
}

#[tokio::test]
async fn it_decompress_deflate() {
    let server = TestServer::spawn(|app| {
        app.use_decompression();
        app.map_post("/decompress", |Json(value): Json<Value>| async move {
            ok!(value)
        });   
    }).await;

    let data = b"{\"age\":33,\"name\":\"John\"}";
    let mut encoder = ZlibEncoder::new(Vec::new());

    encoder.write_all(data).await.unwrap();
    encoder.shutdown().await.unwrap();
    let body = encoder.into_inner();

    let response = server.client()
        .post(server.url("/decompress"))
        .header("content-encoding", "deflate")
        .body(body)
        .send()
        .await
        .unwrap();

    assert_eq!(response.json::<Value>().await.unwrap(), json!({ "name": "John", "age": 33 }));
    
    server.shutdown().await;
}

#[tokio::test]
async fn it_decompress_zstd() {
    let server = TestServer::spawn(|app| {
        app.use_decompression();
        app.map_post("/decompress", |Json(value): Json<Value>| async move {
            ok!(value)
        });   
    }).await;

    let data = b"{\"age\":33,\"name\":\"John\"}";
    let mut encoder = ZstdEncoder::new(Vec::new());

    encoder.write_all(data).await.unwrap();
    encoder.shutdown().await.unwrap();
    let body = encoder.into_inner();

    let response = server.client()
        .post(server.url("/decompress"))
        .header("content-encoding", "zstd")
        .body(body)
        .send()
        .await
        .unwrap();
    
    assert_eq!(response.json::<Value>().await.unwrap(), json!({ "name": "John", "age": 33 }));
    
    server.shutdown().await;
}

#[tokio::test]
async fn it_ignores_decompress() {
    let server = TestServer::spawn(|app| {
        app.use_decompression();
        app.map_post("/decompress", |Json(value): Json<Value>| async move {
            ok!(value)
        });   
    }).await;

    let body = "{\"age\":33,\"name\":\"John\"}";

    let response = server.client()
        .post(server.url("/decompress"))
        .body(body)
        .send()
        .await
        .unwrap();

    assert_eq!(response.json::<Value>().await.unwrap(), json!({ "name": "John", "age": 33 }));
    
    server.shutdown().await;
}
