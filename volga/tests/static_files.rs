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

#[tokio::test]
async fn it_serves_the_shell_and_the_assets_with_different_cache_control() {
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

    // The index file, the fallback file and the index file requested by name are all
    // addressed by a stable name, so none of them may be immutable.
    for path in ["/", "/index.html", "/deep/unknown"] {
        let response = server.client().get(server.url(path)).send().await.unwrap();

        assert!(response.status().is_success(), "{path}");
        assert_eq!(
            response.headers().get("cache-control").unwrap(),
            "no-cache",
            "{path}"
        );
        assert!(response.headers().contains_key("etag"), "{path}");
    }

    // A content-hashed asset keeps the long-lived immutable policy.
    let response = server
        .client()
        .get(server.url("/assets/app.css"))
        .send()
        .await
        .unwrap();

    assert!(response.status().is_success());
    assert_eq!(
        response.headers().get("cache-control").unwrap(),
        "max-age=86400, public, immutable"
    );

    server.shutdown().await;
}

#[tokio::test]
async fn it_serves_static_files_with_configured_cache_control() {
    let server = TestServer::builder()
        .configure(|app| {
            app.with_host_env(|env| {
                env.with_content_root("tests/static")
                    .with_fallback_file("index.html")
                    .with_asset_cache_control(|cc| cc.with_max_age(60))
                    .with_shell_cache_control(|cc| cc.with_no_store())
            })
        })
        .setup(|app| {
            app.use_static_files();
        })
        .build()
        .await;

    for path in ["/", "/deep/unknown"] {
        let response = server.client().get(server.url(path)).send().await.unwrap();

        assert_eq!(
            response.headers().get("cache-control").unwrap(),
            "no-cache, no-store",
            "{path}"
        );
    }

    let response = server
        .client()
        .get(server.url("/assets/app.css"))
        .send()
        .await
        .unwrap();

    assert_eq!(
        response.headers().get("cache-control").unwrap(),
        "max-age=60, public, immutable"
    );

    server.shutdown().await;
}

#[tokio::test]
async fn it_revalidates_every_static_file_into_a_304() {
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

    // The shell is served `no-cache`, which promises revalidation - not a full body on
    // every reload. The index and the fallback are reached by their own handlers, so this
    // covers all three routes rather than the named-file one alone.
    for (path, expected) in [
        ("/", "no-cache"),
        ("/deep/unknown", "no-cache"),
        ("/assets/app.css", "max-age=86400, public, immutable"),
    ] {
        let first = server.client().get(server.url(path)).send().await.unwrap();
        assert!(first.status().is_success(), "{path}");
        let etag = first.headers().get("etag").unwrap().clone();

        let second = server
            .client()
            .get(server.url(path))
            .header("if-none-match", etag)
            .send()
            .await
            .unwrap();

        assert_eq!(second.status(), 304, "{path}");
        assert_eq!(second.content_length().unwrap_or(0), 0, "{path}");
        // A cache updates what it stored from the headers of the `304`, so the policy has
        // to be on it as well - otherwise a file keeps the policy it was first stored with.
        assert_eq!(
            second.headers().get("cache-control").unwrap(),
            expected,
            "{path}"
        );
    }

    server.shutdown().await;
}
