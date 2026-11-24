#![allow(missing_docs)]
#![cfg(all(feature = "test", feature = "decompression-full"))]

use http_body_util::BodyExt;
use serde_json::{Value, json};
use volga::{HttpRequest, Json, Limit, ok};

use async_compression::tokio::write::{BrotliEncoder, GzipEncoder, ZlibEncoder, ZstdEncoder};
use tokio::io::{AsyncWrite, AsyncWriteExt};
use volga::error::Error;
use volga::middleware::decompress::ExpansionRatio;
use volga::test::TestServer;

#[tokio::test]
async fn it_decompress_brotli() {
    let server = TestServer::spawn(|app| {
        app.use_decompression();
        app.map_post("/decompress", |Json(value): Json<Value>| async move {
            ok!(value)
        });
    })
    .await;

    let data = b"{\"age\":33,\"name\":\"John\"}";
    let mut encoder = BrotliEncoder::new(Vec::new());

    encoder.write_all(data).await.unwrap();
    encoder.shutdown().await.unwrap();
    let body = encoder.into_inner();

    let response = server
        .client()
        .post(server.url("/decompress"))
        .header("content-encoding", "br")
        .body(body)
        .send()
        .await
        .unwrap();

    assert_eq!(
        response.json::<Value>().await.unwrap(),
        json!({ "name": "John", "age": 33 })
    );

    server.shutdown().await;
}

#[tokio::test]
async fn it_decompress_brotli_for_route() {
    let server = TestServer::spawn(|app| {
        app.map_post("/decompress", |Json(value): Json<Value>| async move {
            ok!(value)
        })
        .with_decompression();
    })
    .await;

    let data = b"{\"age\":33,\"name\":\"John\"}";
    let mut encoder = BrotliEncoder::new(Vec::new());

    encoder.write_all(data).await.unwrap();
    encoder.shutdown().await.unwrap();
    let body = encoder.into_inner();

    let response = server
        .client()
        .post(server.url("/decompress"))
        .header("content-encoding", "br")
        .body(body)
        .send()
        .await
        .unwrap();

    assert_eq!(
        response.json::<Value>().await.unwrap(),
        json!({ "name": "John", "age": 33 })
    );

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
    })
    .await;

    let data = b"{\"age\":33,\"name\":\"John\"}";
    let mut encoder = BrotliEncoder::new(Vec::new());

    encoder.write_all(data).await.unwrap();
    encoder.shutdown().await.unwrap();
    let body = encoder.into_inner();

    let response = server
        .client()
        .post(server.url("/tests/decompress"))
        .header("content-encoding", "br")
        .body(body)
        .send()
        .await
        .unwrap();

    assert_eq!(
        response.json::<Value>().await.unwrap(),
        json!({ "name": "John", "age": 33 })
    );

    server.shutdown().await;
}

const ENCODINGS: [&str; 4] = ["br", "gzip", "deflate", "zstd"];

const INVALID_COMPRESSED_BODY: &str = "Decompression error: InvalidCompressedBody";

/// Encodes `data` as the `Content-Encoding` `encoding` names
async fn encode(encoding: &str, data: &[u8]) -> Vec<u8> {
    async fn encoded<E: AsyncWrite + Unpin>(mut encoder: E, data: &[u8]) -> E {
        encoder.write_all(data).await.unwrap();
        encoder.shutdown().await.unwrap();
        encoder
    }

    match encoding {
        "br" => encoded(BrotliEncoder::new(Vec::new()), data)
            .await
            .into_inner(),
        "gzip" => encoded(GzipEncoder::new(Vec::new()), data)
            .await
            .into_inner(),
        "deflate" => encoded(ZlibEncoder::new(Vec::new()), data)
            .await
            .into_inner(),
        "zstd" => encoded(ZstdEncoder::new(Vec::new()), data)
            .await
            .into_inner(),
        _ => unreachable!("no encoder for {encoding}"),
    }
}

/// Posts `body` with `Content-Encoding: encoding` to a route that reads the whole body,
/// and returns the status and the text of the response
async fn post_compressed(encoding: &str, body: Vec<u8>) -> (u16, String) {
    let server = TestServer::spawn(|app| {
        app.use_decompression();
        app.map_post("/decompress", async |req: HttpRequest| {
            let _bytes = req.into_body().collect().await?;
            Ok::<_, Error>(())
        });
    })
    .await;

    let response = server
        .client()
        .post(server.url("/decompress"))
        .header("content-encoding", encoding)
        .body(body)
        .send()
        .await
        .unwrap();

    let status = response.status().as_u16();
    let text = response.text().await.unwrap();

    server.shutdown().await;
    (status, text)
}

/// A large-window stream may ask for a 1 GiB window, allocated before any output exists for
/// the decompression limits to count. This one, the marker `0x11` and a window of 2^30 with an
/// empty last meta-block, decodes to an empty body where the extension is accepted.
#[tokio::test]
async fn it_rejects_large_window_brotli() {
    let (status, text) = post_compressed("br", vec![0x11, 0xde]).await;

    assert_eq!(status, 400);
    assert_eq!(text, INVALID_COMPRESSED_BODY);
}

#[tokio::test]
async fn it_rejects_a_body_that_is_not_a_stream_of_its_encoding() {
    for encoding in ENCODINGS {
        let (status, text) = post_compressed(encoding, b"not a compressed stream".to_vec()).await;

        assert_eq!(status, 400, "{encoding}");
        assert_eq!(text, INVALID_COMPRESSED_BODY, "{encoding}");
    }
}

#[tokio::test]
async fn it_rejects_a_stream_cut_short() {
    for encoding in ENCODINGS {
        let mut body = encode(encoding, b"{\"age\":33,\"name\":\"John\"}").await;
        body.truncate(body.len() / 2);

        let (status, text) = post_compressed(encoding, body).await;

        assert_eq!(status, 400, "{encoding}");
        assert_eq!(text, INVALID_COMPRESSED_BODY, "{encoding}");
    }
}

/// The gzip header passes and the deflate stream after it does not: flate2 reports that as
/// `ErrorKind::Other`, where a broken header is `InvalidData`
#[tokio::test]
async fn it_rejects_gzip_with_a_corrupt_deflate_stream() {
    let mut body = encode("gzip", b"{\"age\":33,\"name\":\"John\"}").await;
    // Past the 10-byte header: a final block of the reserved type 3
    body[10] = 0xff;

    let (status, text) = post_compressed("gzip", body).await;

    assert_eq!(status, 400);
    assert_eq!(text, INVALID_COMPRESSED_BODY);
}

#[tokio::test]
async fn it_decompress_gzip() {
    let server = TestServer::spawn(|app| {
        app.use_decompression();
        app.map_post("/decompress", |Json(value): Json<Value>| async move {
            ok!(value)
        });
    })
    .await;

    let data = b"{\"age\":33,\"name\":\"John\"}";
    let mut encoder = GzipEncoder::new(Vec::new());

    encoder.write_all(data).await.unwrap();
    encoder.shutdown().await.unwrap();
    let body = encoder.into_inner();

    let response = server
        .client()
        .post(server.url("/decompress"))
        .header("content-encoding", "gzip")
        .body(body)
        .send()
        .await
        .unwrap();

    assert_eq!(
        response.json::<Value>().await.unwrap(),
        json!({ "name": "John", "age": 33 })
    );

    server.shutdown().await;
}

#[tokio::test]
async fn it_decompress_deflate() {
    let server = TestServer::spawn(|app| {
        app.use_decompression();
        app.map_post("/decompress", |Json(value): Json<Value>| async move {
            ok!(value)
        });
    })
    .await;

    let data = b"{\"age\":33,\"name\":\"John\"}";
    let mut encoder = ZlibEncoder::new(Vec::new());

    encoder.write_all(data).await.unwrap();
    encoder.shutdown().await.unwrap();
    let body = encoder.into_inner();

    let response = server
        .client()
        .post(server.url("/decompress"))
        .header("content-encoding", "deflate")
        .body(body)
        .send()
        .await
        .unwrap();

    assert_eq!(
        response.json::<Value>().await.unwrap(),
        json!({ "name": "John", "age": 33 })
    );

    server.shutdown().await;
}

#[tokio::test]
async fn it_decompress_zstd() {
    let server = TestServer::spawn(|app| {
        app.use_decompression();
        app.map_post("/decompress", |Json(value): Json<Value>| async move {
            ok!(value)
        });
    })
    .await;

    let data = b"{\"age\":33,\"name\":\"John\"}";
    let mut encoder = ZstdEncoder::new(Vec::new());

    encoder.write_all(data).await.unwrap();
    encoder.shutdown().await.unwrap();
    let body = encoder.into_inner();

    let response = server
        .client()
        .post(server.url("/decompress"))
        .header("content-encoding", "zstd")
        .body(body)
        .send()
        .await
        .unwrap();

    assert_eq!(
        response.json::<Value>().await.unwrap(),
        json!({ "name": "John", "age": 33 })
    );

    server.shutdown().await;
}

#[tokio::test]
async fn it_ignores_decompress() {
    let server = TestServer::spawn(|app| {
        app.use_decompression();
        app.map_post("/decompress", |Json(value): Json<Value>| async move {
            ok!(value)
        });
    })
    .await;

    let body = "{\"age\":33,\"name\":\"John\"}";

    let response = server
        .client()
        .post(server.url("/decompress"))
        .body(body)
        .send()
        .await
        .unwrap();

    assert_eq!(
        response.json::<Value>().await.unwrap(),
        json!({ "name": "John", "age": 33 })
    );

    server.shutdown().await;
}

/// The request body limit wraps the decompressed body in a limit of its own, and a
/// decompression limit tripping underneath it used to come out as that limit's `400`
#[tokio::test]
async fn it_keeps_the_413_of_a_decompression_limit_under_the_body_limit() {
    let server = TestServer::builder()
        .configure(|app| {
            app.with_decompression_limits(|limits| limits.with_max_compressed(Limit::Limited(1)))
        })
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

    let response = server
        .client()
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
async fn it_tests_max_compressed_limit() {
    let server = TestServer::builder()
        .configure(|app| {
            app.without_body_limit()
                .with_decompression_limits(|limits| limits.with_max_compressed(Limit::Limited(1)))
        })
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

    let response = server
        .client()
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
        .configure(|app| {
            app.without_body_limit()
                .with_decompression_limits(|limits| limits.with_max_decompressed(Limit::Limited(1)))
        })
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

    let response = server
        .client()
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
        .configure(|app| {
            app.without_body_limit()
                .with_decompression_limits(|limits| {
                    limits.with_max_expansion_ratio(ExpansionRatio::new(1, 0))
                })
        })
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

    let response = server
        .client()
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
        .configure(|app| {
            app.without_body_limit()
                .with_decompression_limits(|limits| {
                    limits.with_max_expansion_ratio(ExpansionRatio::new(1, 1024 * 1024))
                })
        })
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

    let response = server
        .client()
        .post(server.url("/decompress"))
        .header("content-encoding", "zstd")
        .body(body)
        .send()
        .await
        .unwrap();

    assert!(
        response.status().is_success(),
        "status = {}",
        response.status()
    );

    server.shutdown().await;
}
