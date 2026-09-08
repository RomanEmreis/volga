#![allow(missing_docs)]
#![cfg(all(feature = "test", feature = "static-files"))]

use std::sync::atomic::{AtomicUsize, Ordering};
use volga::app::HostEnv;
use volga::headers::{ETagSource, Header, HttpHeaders, headers};
use volga::{HttpResponse, ok, test::TestServer};

headers! {
    (ServedBy, "x-served-by"),
    (Scope, "x-scope")
}

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

#[tokio::test]
async fn it_revalidates_on_the_last_modified_it_emitted() {
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

    // The checked-in fixtures carry a fractional mtime, as files on any modern filesystem
    // do, while an HTTP-date carries whole seconds - so this fails unless the two are
    // compared at the same precision.
    for path in ["/", "/deep/unknown", "/assets/app.css"] {
        let first = server.client().get(server.url(path)).send().await.unwrap();
        let last_modified = first.headers().get("last-modified").unwrap().clone();

        let second = server
            .client()
            .get(server.url(path))
            .header("if-modified-since", last_modified)
            .send()
            .await
            .unwrap();

        assert_eq!(second.status(), 304, "{path}");
    }

    server.shutdown().await;
}

#[tokio::test]
async fn it_does_not_report_a_conditional_write_as_not_modified() {
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

    let shell = server.client().get(server.url("/")).send().await.unwrap();
    let etag = shell.headers().get("etag").unwrap().clone();
    let last_modified = shell.headers().get("last-modified").unwrap().clone();

    // The fallback answers a route that was not found whatever the method was, so a write to
    // an unknown path reaches the shell too. A validator answers "your copy is current",
    // which is no answer to a `POST` - and the tag it would match describes the shell rather
    // than anything this request was aimed at.
    let posted = server
        .client()
        .post(server.url("/api/v1/orders/new"))
        .header("if-none-match", etag.clone())
        .body("{}")
        .send()
        .await
        .unwrap();

    assert_ne!(posted.status(), 304);

    let put = server
        .client()
        .put(server.url("/api/v1/orders/new"))
        .header("if-modified-since", last_modified)
        .body("{}")
        .send()
        .await
        .unwrap();

    assert_ne!(put.status(), 304);

    // HEAD is a request for a representation, so it still validates.
    let head = server
        .client()
        .head(server.url("/"))
        .header("if-none-match", etag)
        .send()
        .await
        .unwrap();

    assert_eq!(head.status(), 304);

    server.shutdown().await;
}

#[tokio::test]
async fn it_serves_a_file_and_a_dynamic_route_side_by_side() {
    // The static file server used to be routing, and a dynamic route at the same level
    // silently took its place - or was taken by it, depending on which was registered
    // last (#226). It is middleware now, so the router has nothing to collide with.
    let server = TestServer::builder()
        .configure(|app| app.set_host_env(HostEnv::new("tests/static")))
        .setup(|app| {
            app.map_get("/{id}", |id: String| async move { ok!("user:{id}") });
            app.use_static_files();
        })
        .build()
        .await;

    let file = server
        .client()
        .get(server.url("/index.html"))
        .send()
        .await
        .unwrap();

    assert!(file.status().is_success());
    assert_eq!(file.headers().get("Content-Type").unwrap(), "text/html");

    let route = server.client().get(server.url("/42")).send().await.unwrap();

    assert!(route.status().is_success());
    assert_eq!(route.text().await.unwrap(), "user:42");

    server.shutdown().await;
}

#[tokio::test]
async fn it_serves_a_directory_created_after_the_server_started() {
    // The number of routes to register used to be decided by walking the content root at
    // startup, so anything deeper than the tree was then could never be served.
    let content_root = tempfile::tempdir().unwrap();
    let root_path = content_root.path().to_path_buf();

    let server = TestServer::builder()
        .configure(move |app| app.set_host_env(HostEnv::new(&root_path)))
        .setup(|app| {
            app.use_static_files();
        })
        .build()
        .await;

    let nested = content_root.path().join("assets/vendor/theme");
    std::fs::create_dir_all(&nested).unwrap();
    std::fs::write(nested.join("app.css"), "body { color: red }").unwrap();

    let response = server
        .client()
        .get(server.url("/assets/vendor/theme/app.css"))
        .send()
        .await
        .unwrap();

    assert!(response.status().is_success());
    assert_eq!(response.headers().get("Content-Type").unwrap(), "text/css");
    assert_eq!(response.text().await.unwrap(), "body { color: red }");

    server.shutdown().await;
}

#[tokio::test]
async fn it_serves_files_under_the_group_prefix_only() {
    let server = TestServer::builder()
        .configure(|app| app.set_host_env(HostEnv::new("tests/static")))
        .setup(|app| {
            app.group("/static", |g| {
                g.use_static_files();
            });
        })
        .build()
        .await;

    let mounted = server
        .client()
        .get(server.url("/static/assets/app.css"))
        .send()
        .await
        .unwrap();

    assert!(mounted.status().is_success());

    // The same file, addressed outside the prefix the group mounted it under.
    for path in ["/assets/app.css", "/", "/staticky/assets/app.css"] {
        let response = server.client().get(server.url(path)).send().await.unwrap();

        assert_eq!(response.status(), 404, "{path}");
    }

    server.shutdown().await;
}

#[tokio::test]
async fn it_runs_the_group_middleware_around_a_file_it_serves() {
    let server = TestServer::builder()
        .configure(|app| app.set_host_env(HostEnv::new("tests/static")))
        .setup(|app| {
            app.group("/static", |g| {
                g.filter(
                    |headers: HttpHeaders| async move { headers.get_raw("x-api-key").is_some() },
                );
                g.map_ok(|mut resp: HttpResponse| async move {
                    resp.insert_header(Header::<ServedBy>::from_static("the-group"));
                    resp
                });
                g.use_static_files();
            });
        })
        .build()
        .await;

    // A group carries the policy of everything under its prefix, and a file the group
    // serves is under it as much as a route the group mapped.
    let denied = server
        .client()
        .get(server.url("/static/assets/app.css"))
        .send()
        .await
        .unwrap();

    assert_eq!(denied.status(), 400);

    let allowed = server
        .client()
        .get(server.url("/static/assets/app.css"))
        .header("x-api-key", "secret")
        .send()
        .await
        .unwrap();

    assert!(allowed.status().is_success());
    assert_eq!(allowed.headers().get("x-served-by").unwrap(), "the-group");

    server.shutdown().await;
}

#[tokio::test]
async fn it_runs_the_middleware_of_every_group_around_a_nested_mount() {
    let server = TestServer::builder()
        .configure(|app| app.set_host_env(HostEnv::new("tests/static")))
        .setup(|app| {
            app.group("/app", |outer| {
                outer.map_ok(|mut resp: HttpResponse| async move {
                    resp.insert_header(Header::<ServedBy>::from_static("outer"));
                    resp
                });
                outer.group("/static", |inner| {
                    inner.map_ok(|mut resp: HttpResponse| async move {
                        resp.insert_header(Header::<Scope>::from_static("inner"));
                        resp
                    });
                    inner.use_static_files();
                });
            });
        })
        .build()
        .await;

    let response = server
        .client()
        .get(server.url("/app/static/assets/app.css"))
        .send()
        .await
        .unwrap();

    assert!(response.status().is_success());
    assert_eq!(response.headers().get("x-served-by").unwrap(), "outer");
    assert_eq!(response.headers().get("x-scope").unwrap(), "inner");

    server.shutdown().await;
}

#[tokio::test]
async fn it_runs_the_group_middleware_once_for_a_request_it_declines() {
    static CALLS: AtomicUsize = AtomicUsize::new(0);

    let server = TestServer::builder()
        .configure(|app| app.set_host_env(HostEnv::new("tests/static")))
        .setup(|app| {
            app.group("/static", |g| {
                g.map_ok(|resp: HttpResponse| async move {
                    CALLS.fetch_add(1, Ordering::SeqCst);
                    resp
                });
                g.map_get("/info", || async { ok!("info") });
                g.use_static_files();
            });
        })
        .build()
        .await;

    // The mount declines this one - there is no `info` file - so the route answers it,
    // wrapped in the group's middleware. That middleware must not also run for the
    // request on its way past the mount.
    let route = server
        .client()
        .get(server.url("/static/info"))
        .send()
        .await
        .unwrap();

    assert!(route.status().is_success());
    assert_eq!(CALLS.load(Ordering::SeqCst), 1);

    let file = server
        .client()
        .get(server.url("/static/assets/app.css"))
        .send()
        .await
        .unwrap();

    assert!(file.status().is_success());
    assert_eq!(CALLS.load(Ordering::SeqCst), 2);

    server.shutdown().await;
}

#[tokio::test]
async fn it_leaves_a_write_to_the_path_of_a_file_to_the_router() {
    let server = TestServer::builder()
        .configure(|app| app.set_host_env(HostEnv::new("tests/static")))
        .setup(|app| {
            app.use_static_files();
            app.map_post("/index.html", || async { ok!("posted") });
        })
        .build()
        .await;

    let response = server
        .client()
        .post(server.url("/index.html"))
        .send()
        .await
        .unwrap();

    assert!(response.status().is_success());
    assert_eq!(response.text().await.unwrap(), "posted");

    server.shutdown().await;
}

#[cfg(unix)]
#[tokio::test]
async fn it_denies_a_file_that_leaves_the_content_root_through_a_symlink() {
    let outside = tempfile::tempdir().unwrap();
    let secret = outside.path().join("secret.txt");
    std::fs::write(&secret, "secret").unwrap();

    let content_root = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(&secret, content_root.path().join("secret.txt")).unwrap();

    let root_path = content_root.path().to_path_buf();
    let server = TestServer::builder()
        .configure(move |app| app.set_host_env(HostEnv::new(&root_path)))
        .setup(|app| {
            app.use_static_files();
        })
        .build()
        .await;

    let response = server
        .client()
        .get(server.url("/secret.txt"))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 403);

    server.shutdown().await;
}

#[cfg(feature = "compression-full")]
#[tokio::test]
async fn it_compresses_a_file_it_serves() {
    let server = TestServer::builder()
        .configure(|app| app.set_host_env(HostEnv::new("tests/static")))
        .setup(|app| {
            // Registered first, so it wraps everything the mount answers with.
            app.use_compression();
            app.use_static_files();
        })
        .build()
        .await;

    let response = server
        .client()
        .get(server.url("/assets/app.css"))
        .header("accept-encoding", "gzip")
        .send()
        .await
        .unwrap();

    assert!(response.status().is_success());
    assert_eq!(response.headers().get("vary").unwrap(), "accept-encoding");
    assert_eq!(
        response.headers().get("cache-control").unwrap(),
        "max-age=86400, public, immutable"
    );

    server.shutdown().await;
}

/// Replaces a file the way a deploy does - a new file renamed over the old - restoring the
/// modification time of what it replaced, so that nothing a `stat` reports moves but the
/// inode. A content-hashed build produces exactly this pair when it pins timestamps for
/// reproducibility: the shell's `<script src="/assets/index-a1b2c3.js">` keeps its byte
/// length across deploys, and `SOURCE_DATE_EPOCH` keeps its `mtime`.
fn deploy(path: &std::path::Path, contents: &str) {
    let modified = std::fs::metadata(path).unwrap().modified().unwrap();
    let staged = path.with_extension("staged");

    std::fs::write(&staged, contents).unwrap();
    std::fs::File::options()
        .write(true)
        .open(&staged)
        .unwrap()
        .set_modified(modified)
        .unwrap();
    std::fs::rename(&staged, path).unwrap();

    let after = std::fs::metadata(path).unwrap();
    assert_eq!(after.len() as usize, contents.len());
    assert_eq!(after.modified().unwrap(), modified);
}

/// A content root of its own, so that a test may rewrite what it serves without reaching
/// into the fixtures every other test reads.
fn content_root(index: &str) -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().to_path_buf();

    std::fs::create_dir(path.join("assets")).unwrap();
    std::fs::write(path.join("index.html"), index).unwrap();
    std::fs::write(path.join("assets/app.css"), "h1{color:red}").unwrap();

    (dir, path)
}

/// The bug from #233, end to end: a client that holds the tag of the previous shell must be
/// answered with the new one, not told its copy is current.
#[tokio::test]
async fn it_serves_the_new_shell_after_a_same_length_deploy_at_the_same_instant() {
    let (_root, path) = content_root("<script src=/a1b2c3.js>");
    let index = path.join("index.html");

    let server = TestServer::builder()
        .configure(move |app| app.set_host_env(HostEnv::new(&path)))
        .setup(|app| {
            app.use_static_files();
        })
        .build()
        .await;

    // Both the mount point and the index by name are addressed by a stable name, so both
    // are validated by the shell's tag.
    for target in ["/", "/index.html"] {
        let before = server
            .client()
            .get(server.url(target))
            .send()
            .await
            .unwrap();
        let etag = before.headers().get("etag").unwrap().clone();

        deploy(&index, "<script src=/d4e5f6.js>");

        let after = server
            .client()
            .get(server.url(target))
            .header("if-none-match", etag)
            .send()
            .await
            .unwrap();

        assert_eq!(after.status(), 200, "{target}");
        assert_eq!(
            after.text().await.unwrap(),
            "<script src=/d4e5f6.js>",
            "{target}"
        );

        deploy(&index, "<script src=/a1b2c3.js>");
    }

    server.shutdown().await;
}

/// The same deploy through the fallback handler, which reaches the shell by its own route
/// rather than through the mount.
#[tokio::test]
async fn it_serves_the_new_fallback_after_a_same_length_deploy() {
    let (_root, path) = content_root("<script src=/a1b2c3.js>");
    let index = path.join("index.html");

    let server = TestServer::builder()
        .configure(move |app| {
            app.with_host_env(|env| {
                env.with_content_root(&path)
                    .with_fallback_file("index.html")
            })
        })
        .setup(|app| {
            app.use_static_files();
        })
        .build()
        .await;

    let before = server
        .client()
        .get(server.url("/deep/unknown"))
        .send()
        .await
        .unwrap();
    let etag = before.headers().get("etag").unwrap().clone();

    deploy(&index, "<script src=/d4e5f6.js>");

    let after = server
        .client()
        .get(server.url("/deep/unknown"))
        .header("if-none-match", etag)
        .send()
        .await
        .unwrap();

    assert_eq!(after.status(), 200);
    assert_eq!(after.text().await.unwrap(), "<script src=/d4e5f6.js>");

    server.shutdown().await;
}

/// Two replicas of one build carry the same shell tag, so revalidation keeps working behind
/// a load balancer. This is what a tag carrying a sub-second `mtime` would give up.
#[tokio::test]
async fn it_agrees_on_the_shell_tag_across_replicas() {
    let (_here, here) = content_root("<script src=/a1b2c3.js>");
    // Written a moment later, the way a second replica receives the same build.
    std::thread::sleep(std::time::Duration::from_millis(20));
    let (_there, there) = content_root("<script src=/a1b2c3.js>");

    assert_ne!(
        std::fs::metadata(here.join("index.html"))
            .unwrap()
            .modified()
            .unwrap(),
        std::fs::metadata(there.join("index.html"))
            .unwrap()
            .modified()
            .unwrap()
    );

    let mut tags = Vec::new();
    for root in [here, there] {
        let server = TestServer::builder()
            .configure(move |app| app.set_host_env(HostEnv::new(&root)))
            .setup(|app| {
                app.use_static_files();
            })
            .build()
            .await;

        let response = server.client().get(server.url("/")).send().await.unwrap();
        tags.push(response.headers().get("etag").unwrap().clone());

        server.shutdown().await;
    }

    assert_eq!(tags[0], tags[1]);
}

/// An asset stays on the cheap tag by default. It is served `immutable`, so a client never
/// revalidates it and never asks - which is why it is not worth a read per version, and why
/// the `304` here is the documented consequence of that trade rather than a hole.
#[tokio::test]
async fn it_keeps_assets_on_the_metadata_tag_by_default() {
    let (_root, path) = content_root("<script src=/a1b2c3.js>");
    let asset = path.join("assets/app.css");

    let server = TestServer::builder()
        .configure(move |app| app.set_host_env(HostEnv::new(&path)))
        .setup(|app| {
            app.use_static_files();
        })
        .build()
        .await;

    let before = server
        .client()
        .get(server.url("/assets/app.css"))
        .send()
        .await
        .unwrap();
    let etag = before.headers().get("etag").unwrap().clone();

    deploy(&asset, "h1{color:tan}");

    let after = server
        .client()
        .get(server.url("/assets/app.css"))
        .header("if-none-match", etag)
        .send()
        .await
        .unwrap();

    assert_eq!(after.status(), 304);

    server.shutdown().await;
}

/// ...and picks up the content tag once the assets are configured to revalidate, where the
/// same deploy has to be noticed.
#[tokio::test]
async fn it_derives_asset_tags_from_content_when_configured() {
    let (_root, path) = content_root("<script src=/a1b2c3.js>");
    let asset = path.join("assets/app.css");

    let server = TestServer::builder()
        .configure(move |app| {
            app.with_host_env(|env| {
                env.with_content_root(&path)
                    .with_asset_cache_control(|cc| cc.with_no_cache())
                    .with_asset_etag(ETagSource::Content)
            })
        })
        .setup(|app| {
            app.use_static_files();
        })
        .build()
        .await;

    let before = server
        .client()
        .get(server.url("/assets/app.css"))
        .send()
        .await
        .unwrap();
    let etag = before.headers().get("etag").unwrap().clone();

    deploy(&asset, "h1{color:tan}");

    let after = server
        .client()
        .get(server.url("/assets/app.css"))
        .header("if-none-match", etag)
        .send()
        .await
        .unwrap();

    assert_eq!(after.status(), 200);
    assert_eq!(after.text().await.unwrap(), "h1{color:tan}");

    server.shutdown().await;
}

/// The shell can be put back on the cheap tag, for a deployment that never rewrites it in
/// place and would rather not pay the read.
#[tokio::test]
async fn it_keeps_the_shell_on_the_metadata_tag_when_configured() {
    let (_root, path) = content_root("<script src=/a1b2c3.js>");
    let index = path.join("index.html");

    let server = TestServer::builder()
        .configure(move |app| {
            app.with_host_env(|env| {
                env.with_content_root(&path)
                    .with_shell_etag(ETagSource::Metadata)
            })
        })
        .setup(|app| {
            app.use_static_files();
        })
        .build()
        .await;

    let before = server.client().get(server.url("/")).send().await.unwrap();
    let etag = before.headers().get("etag").unwrap().clone();

    deploy(&index, "<script src=/d4e5f6.js>");

    let after = server
        .client()
        .get(server.url("/"))
        .header("if-none-match", etag)
        .send()
        .await
        .unwrap();

    assert_eq!(after.status(), 304);

    server.shutdown().await;
}

/// Every tag the static file server emits stays weak, whichever source it came from - the
/// compression middleware may re-encode the body after it is set, so nothing here can
/// promise octet-equality of what is actually sent.
#[tokio::test]
async fn it_emits_weak_tags_from_either_source() {
    let (_root, path) = content_root("<script src=/a1b2c3.js>");

    let server = TestServer::builder()
        .configure(move |app| app.set_host_env(HostEnv::new(&path)))
        .setup(|app| {
            app.use_static_files();
        })
        .build()
        .await;

    // The shell derives its tag from the content, the asset from the metadata.
    for target in ["/", "/assets/app.css"] {
        let response = server
            .client()
            .get(server.url(target))
            .send()
            .await
            .unwrap();
        let etag = response.headers().get("etag").unwrap().to_str().unwrap();

        assert!(etag.starts_with("W/\""), "{target}: {etag}");
    }

    server.shutdown().await;
}
