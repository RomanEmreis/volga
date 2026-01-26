#![allow(missing_docs)]
#![cfg(all(feature = "test", feature = "rate-limiting"))]

use std::time::Duration;

use volga::rate_limiting::{by, FixedWindow, SlidingWindow};
use volga::test::TestServer;

const RATE_LIMIT_MESSAGE: &str = "Rate limit exceeded. Try again later.";

#[tokio::test]
async fn it_rate_limits_by_header_with_named_policy() {
    let server = TestServer::builder()
        .configure(|app| {
            app.with_fixed_window(
                FixedWindow::new(2, Duration::from_secs(60))
                    .with_name("burst"),
            )
        })
        .setup(|app| {
            app.use_fixed_window(by::header("x-api-key").using("burst"));
            app.map_get("/limited", || async { "ok" });
        })
        .build()
        .await;

    let client = server.client();
    let url = server.url("/limited");

    for _ in 0..2 {
        let response = client
            .get(&url)
            .header("x-api-key", "alpha")
            .send()
            .await
            .unwrap();

        assert!(response.status().is_success());
        assert_eq!(response.text().await.unwrap(), "ok");
    }

    let response = client
        .get(&url)
        .header("x-api-key", "alpha")
        .send()
        .await
        .unwrap();

    assert_eq!(response.status().as_u16(), 429);
    assert_eq!(response.text().await.unwrap(), RATE_LIMIT_MESSAGE);

    let response = client
        .get(&url)
        .header("x-api-key", "beta")
        .send()
        .await
        .unwrap();

    assert!(response.status().is_success());
    assert_eq!(response.text().await.unwrap(), "ok");

    server.shutdown().await;
}

#[tokio::test]
async fn it_rate_limits_sliding_window_by_header_with_named_policy() {
    let server = TestServer::builder()
        .configure(|app| {
            app.with_sliding_window(
                SlidingWindow::new(2, Duration::from_secs(60))
                    .with_name("burst"),
            )
        })
        .setup(|app| {
            app.use_sliding_window(by::header("x-api-key").using("burst"));
            app.map_get("/limited", || async { "ok" });
        })
        .build()
        .await;

    let client = server.client();
    let url = server.url("/limited");

    for _ in 0..2 {
        let response = client
            .get(&url)
            .header("x-api-key", "alpha")
            .send()
            .await
            .unwrap();

        assert!(response.status().is_success());
        assert_eq!(response.text().await.unwrap(), "ok");
    }

    let response = client
        .get(&url)
        .header("x-api-key", "alpha")
        .send()
        .await
        .unwrap();

    assert_eq!(response.status().as_u16(), 429);
    assert_eq!(response.text().await.unwrap(), RATE_LIMIT_MESSAGE);

    let response = client
        .get(&url)
        .header("x-api-key", "beta")
        .send()
        .await
        .unwrap();

    assert!(response.status().is_success());
    assert_eq!(response.text().await.unwrap(), "ok");

    server.shutdown().await;
}

#[tokio::test]
async fn it_rate_limits_fixed_window_per_route() {
    let server = TestServer::builder()
        .configure(|app| app.with_fixed_window(FixedWindow::new(1, Duration::from_secs(60))))
        .setup(|app| {
            app.map_get("/limited", || async { "ok" })
                .fixed_window(by::ip());
        })
        .build()
        .await;

    let client = server.client();
    let url = server.url("/limited");

    let response = client.get(&url).send().await.unwrap();
    assert!(response.status().is_success());
    assert_eq!(response.text().await.unwrap(), "ok");

    let response = client.get(&url).send().await.unwrap();
    assert_eq!(response.status().as_u16(), 429);
    assert_eq!(response.text().await.unwrap(), RATE_LIMIT_MESSAGE);

    server.shutdown().await;
}

#[tokio::test]
async fn it_rate_limits_sliding_window_per_route() {
    let server = TestServer::builder()
        .configure(|app| app.with_sliding_window(SlidingWindow::new(1, Duration::from_secs(60))))
        .setup(|app| {
            app.map_get("/limited", || async { "ok" })
                .sliding_window(by::ip());
        })
        .build()
        .await;

    let client = server.client();
    let url = server.url("/limited");

    let response = client.get(&url).send().await.unwrap();
    assert!(response.status().is_success());
    assert_eq!(response.text().await.unwrap(), "ok");

    let response = client.get(&url).send().await.unwrap();
    assert_eq!(response.status().as_u16(), 429);
    assert_eq!(response.text().await.unwrap(), RATE_LIMIT_MESSAGE);

    server.shutdown().await;
}

#[tokio::test]
async fn it_rate_limits_fixed_window_per_route_group() {
    let server = TestServer::builder()
        .configure(|app| app.with_fixed_window(FixedWindow::new(1, Duration::from_secs(60))))
        .setup(|app| {
            app.group("/tests", |g| {
                g.fixed_window(by::ip());
                g.map_get("/limited", || async { "ok" });
            });
        })
        .build()
        .await;

    let client = server.client();
    let url = server.url("/tests/limited");

    let response = client.get(&url).send().await.unwrap();
    assert!(response.status().is_success());
    assert_eq!(response.text().await.unwrap(), "ok");

    let response = client.get(&url).send().await.unwrap();
    assert_eq!(response.status().as_u16(), 429);
    assert_eq!(response.text().await.unwrap(), RATE_LIMIT_MESSAGE);

    server.shutdown().await;
}

#[tokio::test]
async fn it_rate_limits_sliding_window_per_route_group() {
    let server = TestServer::builder()
        .configure(|app| app.with_sliding_window(SlidingWindow::new(1, Duration::from_secs(60))))
        .setup(|app| {
            app.group("/tests", |g| {
                g.sliding_window(by::ip());
                g.map_get("/limited", || async { "ok" });
            });
        })
        .build()
        .await;

    let client = server.client();
    let url = server.url("/tests/limited");

    let response = client.get(&url).send().await.unwrap();
    assert!(response.status().is_success());
    assert_eq!(response.text().await.unwrap(), "ok");

    let response = client.get(&url).send().await.unwrap();
    assert_eq!(response.status().as_u16(), 429);
    assert_eq!(response.text().await.unwrap(), RATE_LIMIT_MESSAGE);

    server.shutdown().await;
}