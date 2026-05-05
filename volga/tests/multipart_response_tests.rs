//! Integration tests for outgoing `Multipart` responses.

#![cfg(all(feature = "multipart", feature = "test"))]

use bytes::Bytes;
use futures_util::stream;
use volga::http::endpoints::args::multipart::{Multipart, MultipartSubtype, Part};
use volga::test::TestServer;

#[tokio::test]
async fn end_to_end_form_data_response() {
    let server = TestServer::builder()
        .setup(|app| {
            app.map_get("/mp", || async {
                Multipart::from_parts(vec![
                    Part::text("greeting", "hello"),
                    Part::file("logo", "logo.bin", Bytes::from_static(b"\x01\x02\x03")),
                ])
            });
        })
        .build()
        .await;

    let res = server.client().get(server.url("/mp")).send().await.unwrap();
    let ct = res
        .headers()
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    assert!(ct.starts_with("multipart/form-data; boundary="));
    let body = res.bytes().await.unwrap();

    let boundary = multer::parse_boundary(&ct).unwrap();
    let mut mp =
        multer::Multipart::new(stream::iter(vec![Ok::<_, std::io::Error>(body)]), boundary);

    let f1 = mp.next_field().await.unwrap().unwrap();
    assert_eq!(f1.name(), Some("greeting"));
    assert_eq!(f1.text().await.unwrap(), "hello");

    let f2 = mp.next_field().await.unwrap().unwrap();
    assert_eq!(f2.name(), Some("logo"));
    assert_eq!(f2.file_name(), Some("logo.bin"));
    assert_eq!(
        f2.bytes().await.unwrap(),
        Bytes::from_static(b"\x01\x02\x03")
    );

    server.shutdown().await;
}

#[tokio::test]
async fn end_to_end_streaming_response() {
    let server = TestServer::builder()
        .setup(|app| {
            app.map_get("/stream", || async {
                let chunks = stream::iter(vec![
                    Ok::<_, volga::error::Error>(Bytes::from_static(b"alpha-")),
                    Ok(Bytes::from_static(b"beta-")),
                    Ok(Bytes::from_static(b"gamma")),
                ]);
                let part = Part::stream(
                    "log",
                    "log.txt",
                    volga::headers::ContentType::text_utf_8(),
                    chunks,
                );
                Multipart::from_parts(vec![part])
            });
        })
        .build()
        .await;

    let res = server
        .client()
        .get(server.url("/stream"))
        .send()
        .await
        .unwrap();
    let ct = res
        .headers()
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    let body = res.bytes().await.unwrap();
    let boundary = multer::parse_boundary(&ct).unwrap();
    let mut mp =
        multer::Multipart::new(stream::iter(vec![Ok::<_, std::io::Error>(body)]), boundary);
    let f = mp.next_field().await.unwrap().unwrap();
    assert_eq!(f.text().await.unwrap(), "alpha-beta-gamma");

    server.shutdown().await;
}

#[tokio::test]
async fn end_to_end_byteranges_subtype() {
    let server = TestServer::builder()
        .setup(|app| {
            app.map_get("/ranges", || async {
                let part1 = Part::new(b"first" as &[u8])
                    .with_content_type(volga::headers::ContentType::text_utf_8())
                    .with_header_raw(
                        volga::headers::HeaderName::from_static("content-range"),
                        volga::headers::HeaderValue::from_static("bytes 0-4/10"),
                    );
                let part2 = Part::new(b"five!" as &[u8])
                    .with_content_type(volga::headers::ContentType::text_utf_8())
                    .with_header_raw(
                        volga::headers::HeaderName::from_static("content-range"),
                        volga::headers::HeaderValue::from_static("bytes 5-9/10"),
                    );
                Multipart::from_parts(vec![part1, part2]).with_subtype(MultipartSubtype::ByteRanges)
            });
        })
        .build()
        .await;

    let res = server
        .client()
        .get(server.url("/ranges"))
        .send()
        .await
        .unwrap();
    let ct = res
        .headers()
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    assert!(ct.starts_with("multipart/byteranges; boundary="));

    server.shutdown().await;
}

#[tokio::test]
async fn end_to_end_proxy_via_into_outgoing() {
    let server = TestServer::builder()
        .setup(|app| {
            app.map_post("/echo", |mp: Multipart| async move { mp.into_outgoing() });
        })
        .build()
        .await;

    // Build a multipart request body manually
    let body = b"--CLI-BDY\r\n\
Content-Disposition: form-data; name=\"x\"\r\n\
\r\n\
hello-world\r\n\
--CLI-BDY--\r\n";
    let res = server
        .client()
        .post(server.url("/echo"))
        .header("content-type", "multipart/form-data; boundary=CLI-BDY")
        .body(body.to_vec())
        .send()
        .await
        .unwrap();
    let ct = res
        .headers()
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    let bytes = res.bytes().await.unwrap();

    let boundary = multer::parse_boundary(&ct).unwrap();
    let mut mp =
        multer::Multipart::new(stream::iter(vec![Ok::<_, std::io::Error>(bytes)]), boundary);
    let f = mp.next_field().await.unwrap().unwrap();
    assert_eq!(f.name(), Some("x"));
    assert_eq!(f.text().await.unwrap(), "hello-world");

    server.shutdown().await;
}
