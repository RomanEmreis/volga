#![allow(missing_docs)]
#![cfg(all(feature = "test", feature = "static-files"))]

use volga::app::HostEnv;
use volga::{ok, test::TestServer};

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

#[tokio::test]
async fn it_responds_with_files_when_a_dynamic_route_was_registered_first() {
    let server = TestServer::builder()
        .configure(|app| app.set_host_env(HostEnv::new("tests/static")))
        .setup(|app| {
            // Registered first, so it owns the root's dynamic node and the
            // static segments arrive named `lang`, not `path_0`.
            app.map_get(
                "/{lang}/api",
                |lang: String| async move { ok!("api:{lang}") },
            );
            app.use_static_files();
        })
        .build()
        .await;

    let response = server
        .client()
        .get(server.url("/index.html"))
        .send()
        .await
        .unwrap();
    assert!(response.status().is_success());
    assert_eq!(response.headers().get("Content-Type").unwrap(), "text/html");

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
async fn it_responds_with_a_percent_encoded_file_name() {
    let server = TestServer::builder()
        .configure(|app| app.set_host_env(HostEnv::new("tests/static")))
        .setup(|app| {
            app.use_static_files();
        })
        .build()
        .await;

    let response = server
        .client()
        .get(server.url("/my%20file.css"))
        .send()
        .await
        .unwrap();

    assert!(response.status().is_success());
    assert_eq!(response.headers().get("Content-Type").unwrap(), "text/css");

    server.shutdown().await;
}

#[tokio::test]
async fn it_rejects_a_malformed_percent_encoded_path() {
    let server = TestServer::builder()
        .configure(|app| app.set_host_env(HostEnv::new("tests/static")))
        .setup(|app| {
            app.use_static_files();
        })
        .build()
        .await;

    let response = server
        .client()
        .get(server.url("/%zz.css"))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 400);

    server.shutdown().await;
}
