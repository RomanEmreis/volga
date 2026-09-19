#![allow(missing_docs)]
#![cfg(all(feature = "test", feature = "tls"))]

use reqwest::{Certificate, Identity, Version, redirect::Policy};
use std::{
    sync::Once,
    time::{Duration, Instant},
};
use volga::headers::{LOCATION, STRICT_TRANSPORT_SECURITY};
use volga::http::StatusCode;
use volga::test::TestServer;
use volga::tls::TlsConfig;

static INIT: Once = Once::new();

fn init_crypto() {
    INIT.call_once(|| {
        tokio_rustls::rustls::crypto::aws_lc_rs::default_provider()
            .install_default()
            .expect("Failed to install crypto provider");
    });
}

#[tokio::test]
async fn it_works_with_tls_with_no_auth() {
    init_crypto();

    let server = TestServer::builder()
        .with_https()
        .configure(|app| {
            app.set_tls(TlsConfig::from_pem_files(
                "tests/tls/server.pem",
                "tests/tls/server.key",
            ))
        })
        .setup(|app| {
            app.map_get("/tls", || async { "Pass!" });
        })
        .build()
        .await;

    let ca_cert = include_bytes!("tls/ca.pem");
    let ca_certificate = Certificate::from_pem(ca_cert).unwrap();

    let response = server
        .client_builder()
        .add_root_certificate(ca_certificate)
        .build()
        .unwrap()
        .get(server.url("/tls"))
        .send()
        .await
        .unwrap();

    assert_eq!(response.text().await.unwrap(), "Pass!");

    server.shutdown().await;
}

#[tokio::test]
async fn it_works_with_tls_with_required_auth_authenticated() {
    init_crypto();

    let server = TestServer::builder()
        .with_https()
        .configure(|app| {
            app.with_tls(|tls| {
                tls.with_cert_path("tests/tls/server.pem")
                    .with_key_path("tests/tls/server.key")
                    .with_required_client_auth("tests/tls/ca.pem")
            })
        })
        .setup(|app| {
            app.map_get("/tls", || async { "Pass!" });
        })
        .build()
        .await;

    let cert = std::fs::read_to_string("tests/tls/client.pem").unwrap();
    let key = std::fs::read_to_string("tests/tls/client.key").unwrap();
    let combined = format!("{}\n{}", cert, key);

    let identity = Identity::from_pem(combined.as_bytes()).unwrap();

    let ca_cert = include_bytes!("tls/ca.pem");
    let ca_certificate = Certificate::from_pem(ca_cert).unwrap();

    let response = server
        .client_builder()
        .add_root_certificate(ca_certificate)
        .identity(identity)
        .build()
        .unwrap()
        .get(server.url("/tls"))
        .send()
        .await
        .unwrap();

    assert_eq!(response.text().await.unwrap(), "Pass!");

    server.shutdown().await;
}

#[tokio::test]
async fn it_works_with_tls_with_required_auth_unauthenticated() {
    init_crypto();

    let server = TestServer::builder()
        .with_https()
        .configure(|app| {
            app.with_tls(|tls| {
                tls.with_cert_path("tests/tls/server.pem")
                    .with_key_path("tests/tls/server.key")
                    .with_required_client_auth("tests/tls/ca.pem")
            })
        })
        .setup(|app| {
            app.map_get("/tls", || async { "Pass!" });
        })
        .build()
        .await;

    let ca_cert = include_bytes!("tls/ca.pem");
    let ca_certificate = Certificate::from_pem(ca_cert).unwrap();

    let response = server
        .client_builder()
        .add_root_certificate(ca_certificate)
        .build()
        .unwrap()
        .get(server.url("/tls"))
        .send()
        .await;

    assert!(response.is_err());

    server.shutdown().await;
}

#[tokio::test]
async fn it_works_with_tls_with_optional_auth_authenticated() {
    init_crypto();

    let server = TestServer::builder()
        .with_https()
        .configure(|app| {
            app.with_tls(|tls| {
                tls.with_cert_path("tests/tls/server.pem")
                    .with_key_path("tests/tls/server.key")
                    .with_optional_client_auth("tests/tls/ca.pem")
            })
        })
        .setup(|app| {
            app.map_get("/tls", || async { "Pass!" });
        })
        .build()
        .await;

    let cert = std::fs::read_to_string("tests/tls/client.pem").unwrap();
    let key = std::fs::read_to_string("tests/tls/client.key").unwrap();
    let combined = format!("{}\n{}", cert, key);

    let identity = Identity::from_pem(combined.as_bytes()).unwrap();

    let ca_cert = include_bytes!("tls/ca.pem");
    let ca_certificate = Certificate::from_pem(ca_cert).unwrap();

    let response = server
        .client_builder()
        .add_root_certificate(ca_certificate)
        .identity(identity)
        .build()
        .unwrap()
        .get(server.url("/tls"))
        .send()
        .await
        .unwrap();

    assert_eq!(response.text().await.unwrap(), "Pass!");

    server.shutdown().await;
}

#[tokio::test]
async fn it_works_with_tls_with_optional_auth_unauthenticated() {
    init_crypto();

    let server = TestServer::builder()
        .with_https()
        .configure(|app| {
            app.with_tls(|tls| {
                tls.with_cert_path("tests/tls/server.pem")
                    .with_key_path("tests/tls/server.key")
                    .with_optional_client_auth("tests/tls/ca.pem")
            })
        })
        .setup(|app| {
            app.map_get("/tls", || async { "Pass!" });
        })
        .build()
        .await;

    let ca_cert = include_bytes!("tls/ca.pem");
    let ca_certificate = Certificate::from_pem(ca_cert).unwrap();

    let response = server
        .client_builder()
        .add_root_certificate(ca_certificate)
        .build()
        .unwrap()
        .get(server.url("/tls"))
        .send()
        .await
        .unwrap();

    assert_eq!(response.text().await.unwrap(), "Pass!");

    server.shutdown().await;
}

#[tokio::test]
async fn it_works_with_tls_with_required_auth_authenticated_and_https_redirection() {
    init_crypto();

    let http_port = TestServer::get_free_port();
    let server = TestServer::builder()
        .configure(move |app| {
            app.set_tls(TlsConfig::from_pem_files(
                "tests/tls/server.pem",
                "tests/tls/server.key",
            ))
            .with_tls(|tls| {
                tls.with_required_client_auth("tests/tls/ca.pem")
                    .with_https_redirection()
                    .with_http_port(http_port)
            })
            .with_hsts(|hsts| {
                hsts.without_preload()
                    .with_sub_domains()
                    .with_max_age(Duration::from_secs(60))
                    .with_exclude_hosts(["example.com", "example.net"])
            })
        })
        .setup(|app| {
            app.map_get("/tls", || async { "Pass!" });
        })
        .build()
        .await;

    let cert = std::fs::read_to_string("tests/tls/client.pem").unwrap();
    let key = std::fs::read_to_string("tests/tls/client.key").unwrap();
    let combined = format!("{}\n{}", cert, key);

    let identity = Identity::from_pem(combined.as_bytes()).unwrap();

    let ca_cert = include_bytes!("tls/ca.pem");
    let ca_certificate = Certificate::from_pem(ca_cert).unwrap();

    let response = server
        .client_builder()
        .add_root_certificate(ca_certificate)
        .identity(identity)
        .build()
        .unwrap()
        .get(format!("http://localhost:{http_port}/tls"))
        .header("host", "localhost")
        .send()
        .await
        .unwrap();

    assert_eq!(
        response.headers().get(STRICT_TRANSPORT_SECURITY).unwrap(),
        "max-age=60; includeSubDomains"
    );
    assert_eq!(response.text().await.unwrap(), "Pass!");

    server.shutdown().await;
}

#[tokio::test]
async fn it_works_with_tls_with_https_redirection() {
    init_crypto();

    let http_port = TestServer::get_free_port();
    let server = TestServer::builder()
        .configure(move |app| {
            app.set_tls(TlsConfig::from_pem_files(
                "tests/tls/server.pem",
                "tests/tls/server.key",
            ))
            .with_tls(|tls| tls.with_https_redirection().with_http_port(http_port))
            .with_hsts(|hsts| {
                hsts.without_preload()
                    .with_sub_domains()
                    .with_max_age(Duration::from_secs(60))
                    .with_exclude_hosts(["example.com", "example.net"])
            })
        })
        .setup(|app| {
            app.map_get("/tls", || async { "Pass!" });
        })
        .build()
        .await;

    let ca_cert = include_bytes!("tls/ca.pem");
    let ca_certificate = Certificate::from_pem(ca_cert).unwrap();

    let response = server
        .client_builder()
        .add_root_certificate(ca_certificate)
        .redirect(Policy::none())
        .build()
        .unwrap()
        .get(format!("http://localhost:{http_port}/tls"))
        .header("host", "localhost")
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::TEMPORARY_REDIRECT);
    assert_eq!(
        response.headers().get(&LOCATION).unwrap(),
        format!("https://localhost:{}/tls", server.port).as_str()
    );

    server.shutdown().await;
}

/// A TLS server redirecting plain HTTP from a port of its own, and that port
async fn redirecting_server() -> (TestServer, u16) {
    init_crypto();

    let http_port = TestServer::get_free_port();
    let server = TestServer::builder()
        .configure(move |app| {
            app.set_tls(TlsConfig::from_pem_files(
                "tests/tls/server.pem",
                "tests/tls/server.key",
            ))
            .with_tls(|tls| tls.with_https_redirection().with_http_port(http_port))
        })
        .setup(|app| {
            app.map_get("/tls", || async { "Pass!" });
        })
        .build()
        .await;

    // The redirection listener binds in a task of its own, after the server's port is taken
    drop(connect(http_port).await);

    (server, http_port)
}

/// Connects to a local port, waiting for something to listen on it
async fn connect(port: u16) -> tokio::net::TcpStream {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match tokio::net::TcpStream::connect(("127.0.0.1", port)).await {
            Ok(stream) => return stream,
            Err(_) if Instant::now() < deadline => {
                tokio::time::sleep(Duration::from_millis(20)).await
            }
            Err(err) => panic!("nothing started listening on port {port}: {err}"),
        }
    }
}

fn ca_certificate() -> Certificate {
    Certificate::from_pem(include_bytes!("tls/ca.pem")).unwrap()
}

/// A browser arrives at a plain-HTTP port with HTTP/1.1, and has to be redirected whether or
/// not HTTP/2 is enabled beside it
#[tokio::test]
#[cfg(feature = "http1")]
async fn it_redirects_an_http1_request_to_https() {
    let (server, http_port) = redirecting_server().await;

    let response = reqwest::Client::builder()
        .no_proxy()
        .http1_only()
        .redirect(Policy::none())
        .build()
        .unwrap()
        .get(format!("http://localhost:{http_port}/tls?a=b"))
        .send()
        .await
        .unwrap();

    assert_eq!(response.version(), Version::HTTP_11);
    assert_eq!(response.status(), StatusCode::TEMPORARY_REDIRECT);
    assert_eq!(
        response.headers().get(&LOCATION).unwrap(),
        format!("https://localhost:{}/tls?a=b", server.port).as_str()
    );

    server.shutdown().await;
}

/// HTTP/2 sends the host as `:authority`, never as a `Host` header
#[tokio::test]
#[cfg(feature = "http2")]
async fn it_redirects_an_http2_request_to_https_by_its_authority() {
    let (server, http_port) = redirecting_server().await;

    let response = reqwest::Client::builder()
        .no_proxy()
        .http2_prior_knowledge()
        .redirect(Policy::none())
        .build()
        .unwrap()
        .get(format!("http://localhost:{http_port}/tls"))
        .send()
        .await
        .unwrap();

    assert_eq!(response.version(), Version::HTTP_2);
    assert_eq!(response.status(), StatusCode::TEMPORARY_REDIRECT);
    assert_eq!(
        response.headers().get(&LOCATION).unwrap(),
        format!("https://localhost:{}/tls", server.port).as_str()
    );

    server.shutdown().await;
}

/// RFC 9112 Section 3.2: an HTTP/1.1 request without `Host` is answered `400`. Every client
/// library adds the header, so the request is written by hand
#[tokio::test]
#[cfg(feature = "http1")]
async fn it_answers_400_to_a_request_with_no_host() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let (server, http_port) = redirecting_server().await;

    let mut stream = connect(http_port).await;
    stream
        .write_all(b"GET /tls HTTP/1.1\r\nConnection: close\r\n\r\n")
        .await
        .unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).await.unwrap();

    assert!(
        response.starts_with("HTTP/1.1 400 "),
        "unexpected response: {response}"
    );

    server.shutdown().await;
}

/// Excludes `localhost` from HSTS - by name, with no port - and asserts the header goes to
/// every host but that one, whatever port it is reached on
async fn assert_hsts_skips_the_excluded_host(client: reqwest::ClientBuilder, version: Version) {
    init_crypto();

    let server = TestServer::builder()
        .with_https()
        .configure(|app| {
            app.set_tls(TlsConfig::from_pem_files(
                "tests/tls/server.pem",
                "tests/tls/server.key",
            ))
            .with_hsts(|hsts| hsts.with_exclude_hosts(["localhost"]))
        })
        .setup(|app| {
            app.map_get("/tls", || async { "Pass!" });
        })
        .build()
        .await;

    let client = client
        .no_proxy()
        .add_root_certificate(ca_certificate())
        .build()
        .unwrap();

    let excluded = client
        .get(format!("https://localhost:{}/tls", server.port))
        .send()
        .await
        .unwrap();
    assert_eq!(excluded.version(), version);
    assert!(excluded.status().is_success());
    assert!(excluded.headers().get(STRICT_TRANSPORT_SECURITY).is_none());

    let other = client
        .get(format!("https://127.0.0.1:{}/tls", server.port))
        .send()
        .await
        .unwrap();
    assert_eq!(other.version(), version);
    assert!(other.status().is_success());
    assert!(other.headers().get(STRICT_TRANSPORT_SECURITY).is_some());

    server.shutdown().await;
}

#[tokio::test]
#[cfg(feature = "http1")]
async fn it_does_not_send_hsts_to_an_excluded_host_over_http1() {
    assert_hsts_skips_the_excluded_host(reqwest::Client::builder().http1_only(), Version::HTTP_11)
        .await;
}

#[tokio::test]
#[cfg(feature = "http2")]
async fn it_does_not_send_hsts_to_an_excluded_host_over_http2() {
    assert_hsts_skips_the_excluded_host(
        reqwest::Client::builder().http2_prior_knowledge(),
        Version::HTTP_2,
    )
    .await;
}

/// A client that opened a connection and stalls the TLS handshake has sent no request, so
/// there is nothing to drain: shutting down must not wait on it
#[tokio::test]
async fn shutdown_does_not_wait_for_a_stalled_tls_handshake() {
    use volga::App;

    init_crypto();

    let port = TestServer::get_free_port();
    let (app, handle) = App::with_shutdown();
    let app = app
        .bind(format!("127.0.0.1:{port}"))
        .without_greeter()
        .set_tls(TlsConfig::from_pem_files(
            "tests/tls/server.pem",
            "tests/tls/server.key",
        ));
    let task = tokio::spawn(async move { app.run().await });

    // Connected, and then silent - no ClientHello ever arrives
    let _stalled = connect(port).await;
    // Give the accept loop a moment to take the connection in
    tokio::time::sleep(Duration::from_millis(50)).await;

    handle.shutdown();

    tokio::time::timeout(Duration::from_secs(2), task)
        .await
        .expect("server waited on a connection that never finished its TLS handshake")
        .expect("server task panicked")
        .expect("server returned an error");
}

/// The HTTPS redirection listener drains its connections on shutdown the way the server does,
/// and `run` returns only once it has: a client that never finishes its request on the
/// redirection port is closed when the timeout runs out, not left open past `run` (#254)
#[tokio::test]
async fn shutdown_closes_a_stalled_redirection_connection_before_run_returns() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use volga::App;

    init_crypto();

    let port = TestServer::get_free_port();
    let http_port = TestServer::get_free_port();
    let (app, handle) = App::with_shutdown();
    let app = app
        .bind(format!("127.0.0.1:{port}"))
        .without_greeter()
        .with_shutdown_timeout(Duration::from_millis(500))
        .set_tls(TlsConfig::from_pem_files(
            "tests/tls/server.pem",
            "tests/tls/server.key",
        ))
        .with_tls(|tls| tls.with_https_redirection().with_http_port(http_port));
    let task = tokio::spawn(async move { app.run().await });

    // A request that never ends keeps the redirection connection busy. It has to be one the
    // listener reads as unfinished rather than malformed, which would close it straight away:
    // an HTTP/1.1 head with no blank line, or, where only HTTP/2 is served, half its preface
    #[cfg(feature = "http1")]
    let unfinished: &[u8] = b"GET /tls HTTP/1.1\r\nHost: localhost\r\n";
    #[cfg(not(feature = "http1"))]
    let unfinished: &[u8] = b"PRI * HTTP/2.0\r\n";

    let mut stalled = connect(http_port).await;
    stalled.write_all(unfinished).await.unwrap();
    // Give the listener a moment to take the connection in
    tokio::time::sleep(Duration::from_millis(50)).await;

    let started = Instant::now();
    handle.shutdown();
    tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .expect("server did not exit after the shutdown timeout")
        .expect("server task panicked")
        .expect("server returned an error");

    let elapsed = started.elapsed();
    assert!(
        elapsed >= Duration::from_millis(500),
        "run returned before the redirection connection was drained: {elapsed:?}"
    );

    // Closed by the time `run` returned: what is left to read - an HTTP/2 server's SETTINGS
    // and GOAWAY frames - is already there, and the end of the stream follows it right away
    let mut rest = Vec::new();
    tokio::time::timeout(Duration::from_millis(200), stalled.read_to_end(&mut rest))
        .await
        .expect("the redirection connection outlived run")
        .ok();
}
