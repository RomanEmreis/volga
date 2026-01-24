#![allow(missing_docs)]
#![cfg(all(feature = "test", feature = "decompression-full"))]

use http_body_util::BodyExt;
use volga::{Json, ok, HttpRequest, Limit};
use serde_json::{Value, json};

use async_compression::tokio::write::{
    BrotliEncoder, 
    GzipEncoder, 
    ZlibEncoder, 
    ZstdEncoder
};
use tokio::io::AsyncWriteExt;
use volga::error::Error;
use volga::test::TestServer;
use volga::middleware::decompress::ExpansionRatio;

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

#[tokio::test]
async fn it_tests_max_compressed_limit() {
    let server = TestServer::builder()
        .configure(|app| app
            .without_body_limit()
            .with_decompression_limits(|limits| {
                limits.with_max_compressed(Limit::Limited(1))
            })
        )
        .setup(|app| {
            app.use_decompression();
            app.map_post("/decompress", async |req: HttpRequest| {
                let body = req.into_body();
                let _bytes = body.collect().await?;
                Ok::<_, Error>(())
            });
        })
        .build()
        .await;

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

    assert_eq!(response.status(), 413);
    assert_eq!(
        response.text().await.unwrap(),
        "Decompression error: CompressedBodyTooLarge"
    );

    server.shutdown().await;
}

#[tokio::test]
async fn it_tests_max_decompressed_limit() {
    let server = TestServer::builder()
        .configure(|app| app
            .without_body_limit()
            .with_decompression_limits(|limits| {
                limits.with_max_decompressed(Limit::Limited(1))
            })
        )
        .setup(|app| {
            app.use_decompression();
            app.map_post("/decompress", async |req: HttpRequest| {
                let body = req.into_body();
                let _bytes = body.collect().await?;
                Ok::<_, Error>(())
            });
        })
        .build()
        .await;

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

    assert_eq!(response.status(), 413);
    assert_eq!(
        response.text().await.unwrap(),
        "Decompression error: DecompressedBodyTooLarge"
    );

    server.shutdown().await;
}

#[tokio::test]
async fn it_tests_expansion_ratio_exceeded() {
    let server = TestServer::builder()
        .configure(|app| app
            .without_body_limit()
            .with_decompression_limits(|limits| {
                limits.with_max_expansion_ratio(ExpansionRatio::new(1, 0))
            })
        )
        .setup(|app| {
            app.use_decompression();
            app.map_post("/decompress", async |req: HttpRequest| {
                let body = req.into_body();
                let _bytes = body.collect().await?;
                Ok::<_, Error>(())
            });
        })
        .build()
        .await;

    let data = vec![b'a'; 64 * 1024];

    let mut encoder = ZstdEncoder::new(Vec::new());
    encoder.write_all(&data).await.unwrap();
    encoder.shutdown().await.unwrap();
    let body = encoder.into_inner();

    let response = server.client()
        .post(server.url("/decompress"))
        .header("content-encoding", "zstd")
        .body(body)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 413);
    assert_eq!(
        response.text().await.unwrap(),
        "Decompression error: ExpansionRatioExceeded"
    );

    server.shutdown().await;
}

#[tokio::test]
async fn it_does_not_exceed_expansion_ratio_when_within_slack() {
    let server = TestServer::builder()
        .configure(|app| app
            .without_body_limit()
            .with_decompression_limits(|limits| {
                limits.with_max_expansion_ratio(ExpansionRatio::new(1, 1024 * 1024))
            })
        )
        .setup(|app| {
            app.use_decompression();
            app.map_post("/decompress", async |req: HttpRequest| {
                let body = req.into_body();
                let bytes = body.collect().await?.to_bytes();

                assert!(!bytes.is_empty());

                Ok::<_, Error>(())
            });
        })
        .build()
        .await;

    let data = vec![b'a'; 64 * 1024];

    let mut encoder = ZstdEncoder::new(Vec::new());
    encoder.write_all(&data).await.unwrap();
    encoder.shutdown().await.unwrap();
    let body = encoder.into_inner();

    let response = server.client()
        .post(server.url("/decompress"))
        .header("content-encoding", "zstd")
        .body(body)
        .send()
        .await
        .unwrap();

    assert!(response.status().is_success(), "status = {}", response.status());

    server.shutdown().await;
}
