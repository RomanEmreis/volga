#![allow(missing_docs)]
#![cfg(all(feature = "test", feature = "static-files"))]

use volga::app::HostEnv;
use volga::test::TestServer;

#[tokio::test]
async fn it_responds_with_index_file() {
    let server = TestServer::builder()
        .configure(|app| app.set_host_env(HostEnv::new("tests/static")))
        .setup(|app| {
            app.use_static_files();
        })
        .build()
        .await;

    let response = server.client().get(server.url("/")).send().await.unwrap();

    assert!(response.status().is_success());
    assert_eq!(response.headers().get("Content-Type").unwrap(), "text/html");

    server.shutdown().await;
}

#[tokio::test]
async fn it_responds_with_fallback_file() {
    let server = TestServer::builder()
        .configure(|app| {
            app.with_host_env(|env| {
                env.with_content_root("tests/static")
                    .with_fallback_file("index.html")
            })
        })
        .setup(|app| {
            app.group("/static", |g| {
                g.use_static_files();
            });
        })
        .build()
        .await;

    let response = server
        .client()
        .get(server.url("/test/thing"))
        .send()
        .await
        .unwrap();

    assert!(response.status().is_success());
    assert_eq!(response.headers().get("Content-Type").unwrap(), "text/html");

    server.shutdown().await;
}

#[tokio::test]
async fn it_responds_with_files_listing() {
    let server = TestServer::builder()
        .configure(|app| {
            app.with_host_env(|env| env.with_content_root("tests/static").with_files_listing())
        })
        .setup(|app| {
            app.use_static_files();
        })
        .build()
        .await;

    let response = server.client().get(server.url("/")).send().await.unwrap();

    assert!(response.status().is_success());
    assert_eq!(
        response.headers().get("Content-Type").unwrap(),
        "text/html; charset=utf-8"
    );

    server.shutdown().await;
}

#[tokio::test]
async fn it_responds_with_nested_file() {
    let server = TestServer::builder()
        .configure(|app| app.set_host_env(HostEnv::new("tests/static")))
        .setup(|app| {
            app.use_static_files();
        })
        .build()
        .await;

    let response = server
        .client()
        .get(server.url("/assets/app.css"))
        .send()
        .await
        .unwrap();

    assert!(response.status().is_success());
    assert_eq!(response.headers().get("Content-Type").unwrap(), "text/css");

    server.shutdown().await;
}

#[tokio::test]
async fn it_responds_with_nested_file_deterministically() {
    let server = TestServer::builder()
        .configure(|app| {
            app.with_host_env(|env| {
                env.with_content_root("tests/static")
                    .with_fallback_file("index.html")
            })
        })
        .setup(|app| {
            app.use_static_files();
        })
        .build()
        .await;

    // The segments of a nested path used to be reassembled in an arbitrary
    // order, so the same URL served the file on one request and fell through
    // to the fallback file on the next.
    for _ in 0..25 {
        let response = server
            .client()
            .get(server.url("/assets/app.css"))
            .send()
            .await
            .unwrap();

        assert!(response.status().is_success());
        assert_eq!(response.headers().get("Content-Type").unwrap(), "text/css");
    }

    server.shutdown().await;
}

#[tokio::test]
async fn it_responds_with_nested_file_from_a_group() {
    let server = TestServer::builder()
        .configure(|app| app.set_host_env(HostEnv::new("tests/static")))
        .setup(|app| {
            app.group("/static", |g| {
                g.use_static_files();
            });
        })
        .build()
        .await;

    let response = server
        .client()
        .get(server.url("/static/assets/app.css"))
        .send()
        .await
        .unwrap();

    assert!(response.status().is_success());
    assert_eq!(response.headers().get("Content-Type").unwrap(), "text/css");

    server.shutdown().await;
}
