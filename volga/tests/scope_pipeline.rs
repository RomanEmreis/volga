#![allow(missing_docs)]
#![cfg(feature = "test")]

use std::net::IpAddr;
use volga::{ClientIp, Limit};
use volga::test::TestServer;

#[tokio::test]
async fn it_returns_404_for_unknown_route() {
    let server = TestServer::spawn(|_app| {}).await;

    let response = server.client()
        .get(server.url("/missing"))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status().as_u16(), 404);

    server.shutdown().await;
}

#[tokio::test]
async fn it_returns_405_for_wrong_method() {
    let server = TestServer::spawn(|app| {
        app.map_get("/only-get", || async { "ok" });
    }).await;

    let response = server.client()
        .post(server.url("/only-get"))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status().as_u16(), 405);

    server.shutdown().await;
}

#[tokio::test]
async fn it_extracts_client_ip_from_extensions() {
    let server = TestServer::spawn(|app| {
        app.map_get("/ip", |ip: ClientIp| async move { ip.to_string() });
    }).await;

    let response = server.client()
        .get(server.url("/ip"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();

    let addr: IpAddr = response
        .split(':')
        .next()
        .unwrap()
        .parse()
        .unwrap();

    assert!(addr.is_loopback());

    server.shutdown().await;
}

#[tokio::test]
#[cfg(feature = "http1")]
async fn it_rejects_request_with_large_headers_http1() {
    let server = TestServer::builder()
        .configure(|app| app.with_max_header_list_size(Limit::Limited(10)))
        .setup(|app| {
            app.map_get("/", || async { "ok" });
        })
        .build()
        .await;

    let response = server.client()
        .get(server.url("/"))
        .header("x-large-header", "this-is-too-large")
        .send()
        .await
        .unwrap();

    assert_eq!(response.status().as_u16(), 431);

    server.shutdown().await;
}

#[tokio::test]
#[cfg(feature = "http2")]
async fn it_rejects_request_with_large_headers_http2() {
    let server = TestServer::builder()
        .configure(|app| app.with_max_header_count(Limit::Limited(1)))
        .setup(|app| {
            app.map_get("/", || async { "ok" });
        })
        .build()
        .await;

    let response = server.client()
        .get(server.url("/"))
        .header("x-one", "one")
        .header("x-two", "two")
        .send()
        .await
        .unwrap();

    assert_eq!(response.status().as_u16(), 431);

    server.shutdown().await;
}

#[tokio::test]
#[cfg(feature = "static-files")]
async fn it_extracts_host_env_from_extensions() {
    use volga::app::HostEnv;

    let server = TestServer::builder()
        .configure(|app| app.with_host_env(|env| env.with_content_root("tests/static")))
        .setup(|app| {
            app.map_get("/env", |env: HostEnv| async move {
                env.content_root().to_str().unwrap().to_string()
            });
        })
        .build()
        .await;

    let response = server.client()
        .get(server.url("/env"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();

    assert_eq!(response, "tests/static");

    server.shutdown().await;
}
