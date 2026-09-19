#![allow(missing_docs)]
#![cfg(all(feature = "test", feature = "static-files"))]

//! The fallback file is served through a route answering `GET` and `HEAD` under the prefix
//! of the mount that serves it (#256, #257): a request for any other method gets the router's
//! `405`, a path outside the prefix is left to the rest of the application, and a route the
//! application mapped answers ahead of it.

use reqwest::Method;
use std::path::PathBuf;
use volga::headers::{Header, HttpHeaders, headers};
use volga::test::TestServer;
use volga::{App, HttpResponse, not_found, ok};

headers! {
    (ServedBy, "x-served-by"),
    (Scope, "x-scope")
}

/// A content root whose index file, fallback file and asset can be told apart by what they
/// hold.
fn site() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();

    std::fs::create_dir(root.join("assets")).unwrap();
    std::fs::write(root.join("index.html"), "INDEX").unwrap();
    std::fs::write(root.join("shell.html"), "SHELL").unwrap();
    std::fs::write(root.join("assets/app.css"), "CSS").unwrap();

    (dir, root)
}

/// Configures `app` to serve `root` with `shell.html` as the fallback file.
fn with_shell(app: App, root: PathBuf) -> App {
    app.with_host_env(|env| {
        env.with_content_root(&root)
            .with_fallback_file("shell.html")
    })
}

/// What `method` on `path` is answered with: the status, the body and the `Allow` header.
async fn send(server: &TestServer, method: Method, path: &str) -> (u16, String, Option<String>) {
    let response = server
        .client()
        .request(method, server.url(path))
        .send()
        .await
        .unwrap();

    let status = response.status().as_u16();
    let allow = response
        .headers()
        .get("allow")
        .map(|v| v.to_str().unwrap().to_owned());

    (status, response.text().await.unwrap(), allow)
}

async fn get(server: &TestServer, path: &str) -> (u16, String) {
    let (status, body, _) = send(server, Method::GET, path).await;
    (status, body)
}

/// The reproduction from #256: the shell is how a client-side URL renders the application,
/// so it answers `GET` and `HEAD`, and everything else reaching it is refused the way a
/// route refuses a method it does not have - whether a file, a route or nothing at all is at
/// that path.
#[tokio::test]
async fn it_answers_get_and_head_alone_with_the_fallback_file() {
    let (_dir, root) = site();

    let server = TestServer::builder()
        .configure(move |app| with_shell(app, root))
        .setup(|app| {
            app.map_get("/api/items", || "items");
            app.use_static_files();
        })
        .build()
        .await;

    assert_eq!(get(&server, "/settings").await, (200, "SHELL".into()));

    let (status, body, _) = send(&server, Method::HEAD, "/settings").await;
    assert_eq!((status, body.as_str()), (200, ""));

    let refused = [
        (Method::POST, "/settings"),
        (Method::PUT, "/api/unknown"),
        (Method::DELETE, "/api/unknown"),
        (Method::PATCH, "/api/unknown"),
        (Method::OPTIONS, "/api/unknown"),
        (Method::from_bytes(b"PURGE").unwrap(), "/api/unknown"),
        // A file is there: `GET` serves it, and a write is refused rather than answered
        // with the shell
        (Method::POST, "/assets/app.css"),
        (Method::DELETE, "/index.html"),
        // The mount point
        (Method::POST, "/"),
        // A route is there, which is what the router has always answered this way
        (Method::POST, "/api/items"),
    ];

    for (method, path) in refused {
        let (status, body, allow) = send(&server, method.clone(), path).await;

        assert_eq!(status, 405, "{method} {path}");
        assert_eq!(allow.as_deref(), Some("GET,HEAD"), "{method} {path}");
        assert_ne!(body, "SHELL", "{method} {path}");
    }

    server.shutdown().await;
}

/// The first half of #257: a group mounts its fallback file under its own prefix, as it
/// mounts its files.
#[tokio::test]
async fn it_serves_the_fallback_file_of_a_group_under_its_prefix_alone() {
    let (_dir, root) = site();

    let server = TestServer::builder()
        .configure(move |app| with_shell(app, root))
        .setup(|app| {
            app.map_get("/api/models", || "models");
            app.group("/static", |g| {
                g.use_static_files();
            });
        })
        .build()
        .await;

    assert_eq!(
        get(&server, "/static/assets/app.css").await,
        (200, "CSS".into())
    );
    assert_eq!(
        get(&server, "/static/deep/link").await,
        (200, "SHELL".into())
    );
    assert_eq!(get(&server, "/static").await, (200, "INDEX".into()));
    assert_eq!(get(&server, "/api/models").await, (200, "models".into()));

    for path in ["/api/nope", "/totally/elsewhere", "/staticky/deep"] {
        assert_eq!(get(&server, path).await, (404, String::new()), "{path}");
    }

    server.shutdown().await;
}

/// The fallback file and the application's fallback used to share one slot, and whichever
/// was registered last replaced the other.
#[tokio::test]
async fn it_leaves_the_application_fallback_in_place() {
    for fallback_first in [true, false] {
        let (_dir, root) = site();

        let server = TestServer::builder()
            .configure(move |app| with_shell(app, root))
            .setup(move |app| {
                if fallback_first {
                    app.map_fallback(|| async { not_found!("app fallback") });
                }
                app.group("/static", |g| {
                    g.use_static_files();
                });
                if !fallback_first {
                    app.map_fallback(|| async { not_found!("app fallback") });
                }
            })
            .build()
            .await;

        assert_eq!(
            get(&server, "/static/deep/link").await,
            (200, "SHELL".into()),
            "fallback first: {fallback_first}"
        );
        assert_eq!(
            get(&server, "/elsewhere").await,
            (404, "app fallback".into()),
            "fallback first: {fallback_first}"
        );

        server.shutdown().await;
    }
}

/// With no index file to answer the mount point, the fallback file answers it, as it did
/// when it was the application's fallback.
#[tokio::test]
async fn it_serves_the_fallback_file_at_a_mount_point_with_no_index() {
    let (_dir, root) = site();

    let server = TestServer::builder()
        .configure(move |app| {
            app.with_host_env(|env| {
                env.with_content_root(&root)
                    .with_index_file("missing.html")
                    .with_fallback_file("shell.html")
            })
        })
        .setup(|app| {
            app.use_static_files();
            app.group("/static", |g| {
                g.use_static_files();
            });
        })
        .build()
        .await;

    for path in ["/", "/static", "/static/"] {
        assert_eq!(get(&server, path).await, (200, "SHELL".into()), "{path}");
    }

    server.shutdown().await;
}

/// The shell answers what is left: a literal or a parameter is read before the catch-all it
/// answers under, and a path such a route leaves unanswered comes back to it.
#[tokio::test]
async fn it_gives_way_to_the_routes_the_application_mapped() {
    let (_dir, root) = site();

    let server = TestServer::builder()
        .configure(move |app| with_shell(app, root))
        .setup(|app| {
            app.use_static_files();
            app.map_get(
                "/users/{id}",
                |id: String| async move { format!("user {id}") },
            );
            app.map_post("/{*rest}", |rest: String| async move {
                format!("posted {rest}")
            });
        })
        .build()
        .await;

    assert_eq!(get(&server, "/users/7").await, (200, "user 7".into()));
    assert_eq!(get(&server, "/users").await, (200, "SHELL".into()));
    assert_eq!(get(&server, "/users/7/more").await, (200, "SHELL".into()));

    // A route mapped for another method at the shell's own position shares it
    let (status, body, _) = send(&server, Method::POST, "/a/b").await;
    assert_eq!((status, body.as_str()), (200, "posted a/b"));
    assert_eq!(get(&server, "/a/b").await, (200, "SHELL".into()));

    let (status, _, allow) = send(&server, Method::PUT, "/a/b").await;
    assert_eq!(status, 405);
    assert_eq!(allow.as_deref(), Some("GET,POST,HEAD"));

    server.shutdown().await;
}

/// A `GET` catch-all the application maps where the shell answers takes that position over
/// - it is not a second name for a route the application wrote, so it does not panic as one
/// would - in whichever order the two were mapped.
#[tokio::test]
async fn it_gives_a_catch_all_mapped_by_hand_the_shells_position() {
    for route_first in [true, false] {
        let (_dir, root) = site();

        let server = TestServer::builder()
            .configure(move |app| with_shell(app, root))
            .setup(move |app| {
                if route_first {
                    app.map_get(
                        "/{*rest}",
                        |rest: String| async move { format!("mine {rest}") },
                    );
                }
                app.use_static_files();
                if !route_first {
                    app.map_get(
                        "/{*rest}",
                        |rest: String| async move { format!("mine {rest}") },
                    );
                }
            })
            .build()
            .await;

        assert_eq!(
            get(&server, "/a/b").await,
            (200, "mine a/b".into()),
            "route first: {route_first}"
        );
        // A file still answers ahead of either
        assert_eq!(
            get(&server, "/assets/app.css").await,
            (200, "CSS".into()),
            "route first: {route_first}"
        );

        server.shutdown().await;
    }
}

/// The shell answers under the group, so the group's middleware - and that of every group
/// around it - runs around it, as it runs around the files.
#[tokio::test]
async fn it_runs_the_group_middleware_around_the_fallback_file() {
    let (_dir, root) = site();

    let server = TestServer::builder()
        .configure(move |app| with_shell(app, root))
        .setup(|app| {
            app.group("/app", |outer| {
                outer.map_ok(|mut resp: HttpResponse| async move {
                    resp.insert_header(Header::<ServedBy>::from_static("outer"));
                    resp
                });
                outer.group("/static", |inner| {
                    inner.use_static_files();
                    inner.filter(|headers: HttpHeaders| async move {
                        headers.get_raw("x-api-key").is_some()
                    });
                    inner.map_ok(|mut resp: HttpResponse| async move {
                        resp.insert_header(Header::<Scope>::from_static("inner"));
                        resp
                    });
                });
            });
        })
        .build()
        .await;

    let denied = server
        .client()
        .get(server.url("/app/static/deep/link"))
        .send()
        .await
        .unwrap();

    assert_eq!(denied.status(), 400);

    let allowed = server
        .client()
        .get(server.url("/app/static/deep/link"))
        .header("x-api-key", "secret")
        .send()
        .await
        .unwrap();

    assert_eq!(allowed.status(), 200);
    assert_eq!(allowed.headers().get("x-served-by").unwrap(), "outer");
    assert_eq!(allowed.headers().get("x-scope").unwrap(), "inner");
    assert_eq!(allowed.text().await.unwrap(), "SHELL");

    server.shutdown().await;
}

/// `map_fallback_to_file` on its own serves the shell under the root, for `GET` and `HEAD`.
#[tokio::test]
async fn it_maps_the_fallback_file_under_the_root_on_its_own() {
    let (_dir, root) = site();

    let server = TestServer::builder()
        .configure(move |app| with_shell(app, root))
        .setup(|app| {
            app.map_fallback_to_file();
            app.map_get("/health", || async { ok!("up") });
        })
        .build()
        .await;

    assert_eq!(get(&server, "/").await, (200, "SHELL".into()));
    assert_eq!(get(&server, "/deep/link").await, (200, "SHELL".into()));
    assert_eq!(get(&server, "/health").await, (200, "up".into()));

    let (status, _, allow) = send(&server, Method::DELETE, "/deep/link").await;
    assert_eq!(status, 405);
    assert_eq!(allow.as_deref(), Some("GET,HEAD"));

    server.shutdown().await;
}

/// The route the shell answers under is not one the application wrote, so it is not
/// described as one of its operations.
#[cfg(feature = "openapi")]
#[tokio::test]
async fn it_leaves_the_fallback_file_out_of_the_openapi_document() {
    let (_dir, root) = site();

    let server = TestServer::builder()
        .configure(move |app| with_shell(app, root).with_open_api(|open_api| open_api))
        .setup(|app| {
            app.use_open_api();
            app.map_get("/api/items", || "items");
            app.use_static_files();
            app.group("/static", |g| {
                g.use_static_files();
            });
        })
        .build()
        .await;

    let spec: serde_json::Value = server
        .client()
        .get(server.url("/openapi.json"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    let paths = spec["paths"].as_object().unwrap();

    assert!(paths.contains_key("/api/items"));
    for path in ["/", "/{path}", "/static", "/static/{path}"] {
        assert!(!paths.contains_key(path), "{path} is described");
    }

    server.shutdown().await;
}
