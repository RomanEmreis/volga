use volga::{App, Json, ok};
use serde_json::{Value, json};

use async_compression::tokio::write::{
    BrotliEncoder, 
    GzipEncoder, 
    ZlibEncoder, 
    ZstdEncoder
};
use tokio::io::AsyncWriteExt;

#[tokio::test]
async fn it_decompress_brotli() {
    tokio::spawn(async {
        let mut app = App::new().bind("127.0.0.1:7916");
        app.use_decompression();
        app.map_post("/decompress", |Json(value): Json<Value>| async move {
            ok!(value)
        });
        app.run().await
    });

    let response = tokio::spawn(async {
        let client = if cfg!(all(feature = "http1", not(feature = "http2"))) {
            reqwest::Client::builder().http1_only().build().unwrap()
        } else {
            reqwest::Client::builder().http2_prior_knowledge().build().unwrap()
        };

        let data = b"{\"age\":33,\"name\":\"John\"}";
        let mut encoder = BrotliEncoder::new(Vec::new());

        encoder.write_all(data).await.unwrap();
        encoder.shutdown().await.unwrap();
        let body = encoder.into_inner();
        
        client
            .post("http://127.0.0.1:7916/decompress")
            .header("content-encoding", "br")
            .body(body)
            .send()
            .await.unwrap()
    }).await.unwrap();

    assert_eq!(response.json::<Value>().await.unwrap(), json!({ "name": "John", "age": 33 }));
}

#[tokio::test]
async fn it_decompress_gzip() {
    tokio::spawn(async {
        let mut app = App::new().bind("127.0.0.1:7917");
        app.use_decompression();
        app.map_post("/decompress", |Json(value): Json<Value>| async move {
            ok!(value)
        });
        app.run().await
    });

    let response = tokio::spawn(async {
        let client = if cfg!(all(feature = "http1", not(feature = "http2"))) {
            reqwest::Client::builder().http1_only().build().unwrap()
        } else {
            reqwest::Client::builder().http2_prior_knowledge().build().unwrap()
        };

        let data = b"{\"age\":33,\"name\":\"John\"}";
        let mut encoder = GzipEncoder::new(Vec::new());

        encoder.write_all(data).await.unwrap();
        encoder.shutdown().await.unwrap();
        let body = encoder.into_inner();

        client
            .post("http://127.0.0.1:7917/decompress")
            .header("content-encoding", "gzip")
            .body(body)
            .send()
            .await.unwrap()
    }).await.unwrap();

    assert_eq!(response.json::<Value>().await.unwrap(), json!({ "name": "John", "age": 33 }));
}

#[tokio::test]
async fn it_decompress_deflate() {
    tokio::spawn(async {
        let mut app = App::new().bind("127.0.0.1:7918");
        app.use_decompression();
        app.map_post("/decompress", |Json(value): Json<Value>| async move {
            ok!(value)
        });
        app.run().await
    });

    let response = tokio::spawn(async {
        let client = if cfg!(all(feature = "http1", not(feature = "http2"))) {
            reqwest::Client::builder().http1_only().build().unwrap()
        } else {
            reqwest::Client::builder().http2_prior_knowledge().build().unwrap()
        };

        let data = b"{\"age\":33,\"name\":\"John\"}";
        let mut encoder = ZlibEncoder::new(Vec::new());

        encoder.write_all(data).await.unwrap();
        encoder.shutdown().await.unwrap();
        let body = encoder.into_inner();

        client
            .post("http://127.0.0.1:7918/decompress")
            .header("content-encoding", "deflate")
            .body(body)
            .send()
            .await.unwrap()
    }).await.unwrap();

    assert_eq!(response.json::<Value>().await.unwrap(), json!({ "name": "John", "age": 33 }));
}

#[tokio::test]
async fn it_decompress_zstd() {
    tokio::spawn(async {
        let mut app = App::new().bind("127.0.0.1:7919");
        app.use_decompression();
        app.map_post("/decompress", |Json(value): Json<Value>| async move {
            ok!(value)
        });
        app.run().await
    });

    let response = tokio::spawn(async {
        let client = if cfg!(all(feature = "http1", not(feature = "http2"))) {
            reqwest::Client::builder().http1_only().build().unwrap()
        } else {
            reqwest::Client::builder().http2_prior_knowledge().build().unwrap()
        };

        let data = b"{\"age\":33,\"name\":\"John\"}";
        let mut encoder = ZstdEncoder::new(Vec::new());

        encoder.write_all(data).await.unwrap();
        encoder.shutdown().await.unwrap();
        let body = encoder.into_inner();

        client
            .post("http://127.0.0.1:7919/decompress")
            .header("content-encoding", "zstd")
            .body(body)
            .send()
            .await.unwrap()
    }).await.unwrap();

    assert_eq!(response.json::<Value>().await.unwrap(), json!({ "name": "John", "age": 33 }));
}

#[tokio::test]
async fn it_ignores_decompress() {
    tokio::spawn(async {
        let mut app = App::new().bind("127.0.0.1:7920");
        app.use_decompression();
        app.map_post("/decompress", |Json(value): Json<Value>| async move {
            ok!(value)
        });
        app.run().await
    });

    let response = tokio::spawn(async {
        let client = if cfg!(all(feature = "http1", not(feature = "http2"))) {
            reqwest::Client::builder().http1_only().build().unwrap()
        } else {
            reqwest::Client::builder().http2_prior_knowledge().build().unwrap()
        };

        let body = "{\"age\":33,\"name\":\"John\"}";

        client
            .post("http://127.0.0.1:7920/decompress")
            .body(body)
            .send()
            .await.unwrap()
    }).await.unwrap();

    assert_eq!(response.json::<Value>().await.unwrap(), json!({ "name": "John", "age": 33 }));
}
