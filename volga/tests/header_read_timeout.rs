#![allow(missing_docs)]
#![cfg(all(feature = "test", any(feature = "http1", feature = "tls")))]

//! A client has a limited time to send the head of a request (#313).
//!
//! hyper has a header read timeout of its own, but it only runs with a timer set on the
//! connection builder, which volga did not set: a client sending its headers a byte at a time
//! held its connection for as long as it liked. The timer runs whenever the server waits for
//! a request, so it also closes an idle keep-alive connection, and never while a handler works.
//! A server with both protocols, or with WebSockets, reads the first bytes of a connection to
//! tell HTTP/1 from HTTP/2 before that timer runs: those are bounded by the same timeout.

use std::time::{Duration, Instant};
#[cfg(feature = "http1")]
use tokio::io::AsyncWriteExt;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::net::TcpStream;
use volga::test::TestServer;
use volga::{Limit, ok};

/// The header read timeout the tests run with
const TIMEOUT: Duration = Duration::from_millis(300);

/// How long a test waits for the server to close a connection it should close
const CLOSE_WITHIN: Duration = Duration::from_secs(5);

#[cfg(feature = "http1")]
async fn server(timeout: Limit<Duration>) -> TestServer {
    TestServer::builder()
        .configure(move |app| app.with_header_read_timeout(timeout))
        .setup(|app| {
            app.map_get("/ping", || async { ok!("pong") });
            app.map_get("/slow", || async {
                tokio::time::sleep(TIMEOUT * 3).await;
                ok!("done")
            });
        })
        .build()
        .await
}

async fn connect(server: &TestServer) -> TcpStream {
    let addr = server
        .url("")
        .split_once("://")
        .map(|(_, addr)| addr.to_owned())
        .unwrap();
    TcpStream::connect(addr).await.unwrap()
}

/// Reads until the server closes the connection or `within` passes, and returns what was read
/// and whether the connection was closed
async fn read_until_closed(tcp: &mut (impl AsyncRead + Unpin), within: Duration) -> (String, bool) {
    let mut received = Vec::new();
    let mut buf = [0u8; 1024];
    let deadline = tokio::time::Instant::now() + within;

    loop {
        match tokio::time::timeout_at(deadline, tcp.read(&mut buf)).await {
            Ok(Ok(0)) | Ok(Err(_)) => return (String::from_utf8_lossy(&received).into(), true),
            Ok(Ok(n)) => received.extend_from_slice(&buf[..n]),
            Err(_) => return (String::from_utf8_lossy(&received).into(), false),
        }
    }
}

/// Reads one response with a `Content-Length` body
#[cfg(feature = "http1")]
async fn read_response(tcp: &mut TcpStream) -> String {
    let mut received = Vec::new();
    let mut buf = [0u8; 1024];

    loop {
        let n = tokio::time::timeout(CLOSE_WITHIN, tcp.read(&mut buf))
            .await
            .expect("no response")
            .unwrap();
        assert!(n > 0, "connection closed before the response");
        received.extend_from_slice(&buf[..n]);

        let text = String::from_utf8_lossy(&received);
        if let Some((head, body)) = text.split_once("\r\n\r\n") {
            let length = head
                .lines()
                .find_map(|line| line.strip_prefix("content-length: "))
                .and_then(|len| len.trim().parse::<usize>().ok())
                .unwrap_or(0);
            if body.len() >= length {
                return text.into_owned();
            }
        }
    }
}

#[cfg(feature = "http1")]
#[tokio::test]
async fn it_closes_a_connection_whose_request_head_does_not_arrive_in_time() {
    let server = server(Limit::Limited(TIMEOUT)).await;
    let mut tcp = connect(&server).await;

    let started = Instant::now();
    tcp.write_all(b"GET /ping HTTP/1.1\r\nHost: localhost\r\n")
        .await
        .unwrap();
    let (received, closed) = read_until_closed(&mut tcp, CLOSE_WITHIN).await;

    assert!(closed, "the connection is still open");
    assert!(started.elapsed() >= TIMEOUT, "closed before the timeout");
    assert!(!received.contains("200 OK"), "served: {received}");

    server.shutdown().await;
}

#[cfg(feature = "http1")]
#[tokio::test]
async fn it_closes_an_idle_keep_alive_connection() {
    let server = server(Limit::Limited(TIMEOUT)).await;
    let mut tcp = connect(&server).await;

    tcp.write_all(b"GET /ping HTTP/1.1\r\nHost: localhost\r\n\r\n")
        .await
        .unwrap();
    let response = read_response(&mut tcp).await;
    assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");

    let (received, closed) = read_until_closed(&mut tcp, CLOSE_WITHIN).await;

    assert!(closed, "the idle connection is still open");
    assert!(received.is_empty(), "unexpected bytes: {received}");

    server.shutdown().await;
}

#[cfg(feature = "http1")]
#[tokio::test]
async fn it_does_not_time_out_a_handler() {
    let server = server(Limit::Limited(TIMEOUT)).await;
    let mut tcp = connect(&server).await;

    tcp.write_all(b"GET /slow HTTP/1.1\r\nHost: localhost\r\n\r\n")
        .await
        .unwrap();
    let response = read_response(&mut tcp).await;

    assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
    assert!(response.ends_with("done"), "{response}");

    server.shutdown().await;
}

/// The server reads the first bytes of a connection to tell HTTP/1 from HTTP/2 before either
/// protocol's own handling starts: a client sending nothing is bounded all the same
#[tokio::test]
#[cfg(feature = "http1")]
async fn it_closes_a_connection_that_sends_nothing() {
    let server = server(Limit::Limited(TIMEOUT)).await;
    let mut tcp = connect(&server).await;

    let started = Instant::now();
    let (received, closed) = read_until_closed(&mut tcp, CLOSE_WITHIN).await;

    assert!(closed, "the connection is still open");
    assert!(started.elapsed() >= TIMEOUT, "closed before the timeout");
    assert!(received.is_empty(), "unexpected bytes: {received}");

    server.shutdown().await;
}

/// A start that could still be the HTTP/2 preface keeps the protocol undecided where the
/// server speaks both. A server of HTTP/1 alone refuses it right away, which is fine too
#[tokio::test]
#[cfg(feature = "http1")]
async fn it_closes_a_connection_that_stops_inside_the_http2_preface() {
    let server = server(Limit::Limited(TIMEOUT)).await;
    let mut tcp = connect(&server).await;

    tcp.write_all(b"PRI * HTTP/2.0\r\n").await.unwrap();
    let (_, closed) = read_until_closed(&mut tcp, CLOSE_WITHIN).await;

    assert!(closed, "the connection is still open");

    server.shutdown().await;
}

#[cfg(feature = "http1")]
#[tokio::test]
async fn it_keeps_a_connection_without_a_timeout() {
    let server = server(Limit::Unlimited).await;
    let mut tcp = connect(&server).await;

    tcp.write_all(b"GET /ping HTTP/1.1\r\nHost: localhost\r\n")
        .await
        .unwrap();
    let (_, closed) = read_until_closed(&mut tcp, TIMEOUT * 4).await;
    assert!(!closed, "the connection was closed");

    tcp.write_all(b"\r\n").await.unwrap();
    let response = read_response(&mut tcp).await;
    assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");

    server.shutdown().await;
}

#[cfg(feature = "tls")]
mod tls {
    use super::*;
    use reqwest::Certificate;
    use std::sync::Once;
    use volga::tls::TlsConfig;

    static INIT: Once = Once::new();

    fn init_crypto() {
        INIT.call_once(|| {
            tokio_rustls::rustls::crypto::aws_lc_rs::default_provider()
                .install_default()
                .expect("Failed to install crypto provider");
        });
    }

    async fn https_server() -> TestServer {
        init_crypto();

        TestServer::builder()
            .with_https()
            .configure(|app| {
                app.with_header_read_timeout(Limit::Limited(TIMEOUT))
                    .set_tls(TlsConfig::from_pem_files(
                        "tests/tls/server.pem",
                        "tests/tls/server.key",
                    ))
            })
            .setup(|app| {
                app.map_get("/ping", || async { ok!("pong") });
            })
            .build()
            .await
    }

    #[tokio::test]
    async fn it_closes_a_connection_whose_tls_handshake_does_not_finish_in_time() {
        let server = https_server().await;
        let mut tcp = connect(&server).await;

        let started = Instant::now();
        let (_, closed) = read_until_closed(&mut tcp, CLOSE_WITHIN).await;

        assert!(closed, "the connection is still open");
        assert!(started.elapsed() >= TIMEOUT, "closed before the timeout");

        server.shutdown().await;
    }

    /// A client that completes the handshake and then sends nothing
    #[tokio::test]
    async fn it_closes_a_tls_connection_that_sends_nothing() {
        use std::sync::Arc;
        use tokio_rustls::TlsConnector;
        use tokio_rustls::rustls::pki_types::{CertificateDer, ServerName, pem::PemObject};
        use tokio_rustls::rustls::{ClientConfig, RootCertStore};

        let server = https_server().await;

        let mut roots = RootCertStore::empty();
        roots
            .add(CertificateDer::from_pem_slice(include_bytes!("tls/ca.pem")).unwrap())
            .unwrap();
        let config = ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth();
        let mut tls = TlsConnector::from(Arc::new(config))
            .connect(
                ServerName::try_from("127.0.0.1").unwrap(),
                connect(&server).await,
            )
            .await
            .unwrap();

        let started = Instant::now();
        let (_, closed) = read_until_closed(&mut tls, CLOSE_WITHIN).await;

        assert!(closed, "the connection is still open");
        assert!(started.elapsed() >= TIMEOUT, "closed before the timeout");

        server.shutdown().await;
    }

    #[tokio::test]
    async fn it_serves_https_with_a_header_read_timeout() {
        let server = https_server().await;
        let ca_certificate = Certificate::from_pem(include_bytes!("tls/ca.pem")).unwrap();

        let response = server
            .client_builder()
            .add_root_certificate(ca_certificate)
            .build()
            .unwrap()
            .get(server.url("/ping"))
            .send()
            .await
            .unwrap();

        assert_eq!(response.text().await.unwrap(), "pong");

        server.shutdown().await;
    }
}
