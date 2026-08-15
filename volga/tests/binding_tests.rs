#![allow(missing_docs)]
#![cfg(feature = "test")]

use std::{
    io::ErrorKind,
    time::{Duration, Instant},
};

use volga::{App, ShutdownHandle, ok};

fn pick_free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

/// Returns whether this machine can listen on the IPv6 loopback.
fn has_ipv6_loopback() -> bool {
    std::net::TcpListener::bind("[::1]:0").is_ok()
}

/// A `reqwest::Client` with proxies disabled, so localhost probes are
/// not redirected by HTTP(S)_PROXY env vars set in the test environment.
fn local_client() -> reqwest::Client {
    reqwest::Client::builder()
        .no_proxy()
        .build()
        .expect("failed to build reqwest client")
}

async fn wait_until_listening(client: &reqwest::Client, url: &str) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if client.get(url).send().await.is_ok() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("server never started listening on {url}");
}

fn build_app(addr: String) -> (App, ShutdownHandle) {
    let (app, handle) = App::with_shutdown();
    let mut app = app.bind(addr).without_greeter();
    app.map_get("/ping", || async { ok!("pong") });
    (app, handle)
}

/// Serves `/ping` on `addr` and probes it at `url`.
async fn assert_serves(addr: String, url: String) {
    let (app, handle) = build_app(addr);
    let task = tokio::spawn(async move { app.run().await });

    let client = local_client();
    wait_until_listening(&client, &url).await;

    let response = client.get(&url).send().await.unwrap();
    assert!(response.status().is_success());
    assert_eq!(response.text().await.unwrap(), "pong");

    handle.shutdown();
    task.await.unwrap().unwrap();
}

#[tokio::test]
async fn it_binds_a_host_name() {
    let port = pick_free_port();

    assert_serves(
        format!("localhost:{port}"),
        format!("http://localhost:{port}/ping"),
    )
    .await;
}

#[tokio::test]
async fn it_binds_an_unbracketed_ipv6_literal() {
    if !has_ipv6_loopback() {
        return;
    }
    let port = pick_free_port();

    assert_serves(format!("::1:{port}"), format!("http://[::1]:{port}/ping")).await;
}

#[tokio::test]
async fn it_binds_a_bracketed_ipv6_literal() {
    if !has_ipv6_loopback() {
        return;
    }
    let port = pick_free_port();

    assert_serves(format!("[::1]:{port}"), format!("http://[::1]:{port}/ping")).await;
}

#[tokio::test]
async fn it_binds_an_ipv4_literal() {
    let port = pick_free_port();

    assert_serves(
        format!("127.0.0.1:{port}"),
        format!("http://127.0.0.1:{port}/ping"),
    )
    .await;
}

/// A bind address that cannot be understood must fail the startup instead of
/// silently listening somewhere else - see https://github.com/RomanEmreis/volga/issues/210
#[tokio::test]
async fn it_does_not_listen_on_a_different_address_when_the_address_is_invalid() {
    for addr in ["invalid_ip", "localhost", "", ":7878", "localhost:70000"] {
        let (app, _handle) = App::with_shutdown();
        let err = app
            .bind(addr)
            .without_greeter()
            .run()
            .await
            .expect_err("an address that cannot be understood must not start a server");

        assert_eq!(err.kind(), ErrorKind::InvalidInput, "address: '{addr}'");
        assert!(
            err.to_string().starts_with("invalid bind address"),
            "address: '{addr}', error: {err}"
        );
    }
}

#[tokio::test]
async fn it_does_not_listen_when_the_host_name_cannot_be_resolved() {
    let (app, _handle) = App::with_shutdown();

    let err = app
        .bind("volga.invalid:7878")
        .without_greeter()
        .run()
        .await
        .expect_err("an unresolvable host name must not start a server");

    assert!(
        err.to_string()
            .starts_with("failed to bind 'volga.invalid:7878'"),
        "unexpected error: {err}"
    );
}
