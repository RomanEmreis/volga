#![allow(missing_docs)]
#![cfg(feature = "test")]

//! A response sent before the request body has been read must reach the client.
//!
//! An HTTP/1 connection with part of a request body still unread cannot be kept alive, so the
//! server closes it once the response is out. Closing a socket with data still arriving makes
//! the kernel answer with a TCP reset, and a reset can overtake the response: the client sees
//! "connection reset" instead of the `413` or `401` it was sent. The server lingers on such a
//! connection instead - it keeps reading, and throws away, whatever the client is still
//! sending until the client closes too.
//!
//! HTTP/2 ends the stream alone, with `RST_STREAM(NO_ERROR)`, and keeps the connection, so the
//! same requests over it get their response without any lingering.

use volga::test::TestServer;
use volga::{HttpRequest, Limit, ok, status};

/// How many requests each test sends: on loopback, about one in ten of them is reset without
/// a lingering close, so this many would all get through by chance about once in 200 runs
const REQUESTS: usize = 50;

/// A body big enough that the client is still sending it when the response comes back
const BODY: usize = 1024 * 1024;

async fn read_body(req: HttpRequest) -> volga::HttpResult {
    use http_body_util::BodyExt;

    let bytes = req.into_body().collect().await?.to_bytes();
    ok!(bytes.len())
}

/// The protocol a test talks to the server in
#[derive(Debug, Clone, Copy)]
enum Protocol {
    #[cfg(feature = "http1")]
    Http1,
    #[cfg(feature = "http2")]
    Http2,
}

impl Protocol {
    fn client(self) -> reqwest::Client {
        let builder = reqwest::Client::builder();
        let builder = match self {
            #[cfg(feature = "http1")]
            Self::Http1 => builder.http1_only(),
            #[cfg(feature = "http2")]
            Self::Http2 => builder.http2_prior_knowledge(),
        };
        builder.build().unwrap()
    }
}

/// Sends `REQUESTS` POSTs of `BODY` bytes to `path` in `protocol`, each on a connection of
/// its own, and returns the status of each, or the error it failed with
async fn post_all(server: &TestServer, path: &str, protocol: Protocol) -> Vec<Result<u16, String>> {
    let mut results = Vec::with_capacity(REQUESTS);

    for _ in 0..REQUESTS {
        let result = protocol
            .client()
            .post(server.url(path))
            .body(vec![b'x'; BODY])
            .send()
            .await
            .map(|response| response.status().as_u16())
            .map_err(|err| format!("{err:?}"));
        results.push(result);
    }

    results
}

/// `REQUESTS` POSTs of a body over the limit in `protocol` all get their `413`
async fn assert_413_delivered(protocol: Protocol) {
    let server = TestServer::builder()
        .configure(|app| app.with_body_limit(Limit::Limited(16)))
        .setup(|app| {
            app.map_post("/upload", read_body);
        })
        .build()
        .await;

    let results = post_all(&server, "/upload", protocol).await;
    let failed = results.iter().filter(|r| *r != &Ok(413)).count();

    assert_eq!(
        failed, 0,
        "{failed} of {REQUESTS} did not get 413 over {protocol:?}: {results:?}"
    );

    server.shutdown().await;
}

/// `REQUESTS` POSTs in `protocol` to a handler that answers without reading the body all get
/// its `401`
async fn assert_unread_body_response_delivered(protocol: Protocol) {
    let server = TestServer::builder()
        .setup(|app| {
            app.map_post("/upload", || async { status!(401) });
        })
        .build()
        .await;

    let results = post_all(&server, "/upload", protocol).await;
    let failed = results.iter().filter(|r| *r != &Ok(401)).count();

    assert_eq!(
        failed, 0,
        "{failed} of {REQUESTS} did not get 401 over {protocol:?}: {results:?}"
    );

    server.shutdown().await;
}

#[cfg(feature = "http1")]
#[tokio::test]
async fn it_delivers_a_413_for_a_body_over_the_limit_over_http1() {
    assert_413_delivered(Protocol::Http1).await;
}

#[cfg(feature = "http1")]
#[tokio::test]
async fn it_delivers_a_response_sent_without_reading_the_body_over_http1() {
    assert_unread_body_response_delivered(Protocol::Http1).await;
}

#[cfg(feature = "http2")]
#[tokio::test]
async fn it_delivers_a_413_for_a_body_over_the_limit_over_http2() {
    assert_413_delivered(Protocol::Http2).await;
}

#[cfg(feature = "http2")]
#[tokio::test]
async fn it_delivers_a_response_sent_without_reading_the_body_over_http2() {
    assert_unread_body_response_delivered(Protocol::Http2).await;
}
