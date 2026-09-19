#![allow(missing_docs)]
#![cfg(feature = "test")]

//! A route group claims its prefix with a fallback of its own (#257): a request under the
//! prefix that no route answers is answered there, whatever its method, rather than by
//! whatever the application answers under a shorter prefix.

use reqwest::Method;
use serde::Deserialize;
use volga::http::Uri;
use volga::test::TestServer;
use volga::{NamedPath, not_found, ok};

#[cfg(feature = "middleware")]
use {
    volga::HttpResponse,
    volga::headers::{ACCESS_CONTROL_ALLOW_ORIGIN, ACCESS_CONTROL_REQUEST_METHOD, ORIGIN},
    volga::headers::{Header, HttpHeaders, headers},
};

#[cfg(feature = "middleware")]
headers! {
    (ServedBy, "x-served-by")
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

#[tokio::test]
async fn it_answers_every_method_under_the_prefix() {
    let server = TestServer::spawn(|app| {
        app.group("/api", |api| {
            api.map_get("/models", || async { ok!("models") });
            api.map_fallback(|method: volga::http::Method, uri: Uri| async move {
                not_found!("api: no {method} {}", uri.path())
            });
        });
    })
    .await;

    let methods = [
        Method::GET,
        Method::POST,
        Method::PUT,
        Method::DELETE,
        Method::PATCH,
        Method::OPTIONS,
        Method::from_bytes(b"PURGE").unwrap(),
    ];

    for method in methods {
        for path in ["/api/nope", "/api/v1/deep/path", "/api", "/api/"] {
            let (status, body, _) = send(&server, method.clone(), path).await;

            assert_eq!(status, 404, "{method} {path}");
            assert_eq!(body, format!("api: no {method} {path}"), "{method} {path}");
        }
    }

    // A HEAD is answered by the fallback too, without the body
    let (status, body, _) = send(&server, Method::HEAD, "/api/nope").await;
    assert_eq!((status, body.as_str()), (404, ""));

    // Outside the prefix the application answers as it did before
    assert_eq!(get(&server, "/elsewhere").await, (404, String::new()));
    assert_eq!(get(&server, "/apis/nope").await, (404, String::new()));

    server.shutdown().await;
}

/// A fallback answers where no route is, not where a route lacks a method: a request for a
/// method a route does not have is still that route's `405`, at the prefix itself as well.
#[tokio::test]
async fn it_leaves_a_route_to_refuse_the_methods_it_lacks() {
    let server = TestServer::spawn(|app| {
        app.group("/api", |api| {
            api.map_get("/", || async { ok!("index") });
            api.map_get("/models", || async { ok!("models") });
            api.map_fallback(|| async { not_found!("api") });
        });
    })
    .await;

    assert_eq!(get(&server, "/api").await, (200, "index".into()));
    assert_eq!(get(&server, "/api/models").await, (200, "models".into()));

    for path in ["/api", "/api/models"] {
        let (status, _, allow) = send(&server, Method::POST, path).await;

        assert_eq!(status, 405, "{path}");
        assert_eq!(allow.as_deref(), Some("GET,HEAD"), "{path}");
    }

    // A path the route does not cover is the fallback's again
    assert_eq!(get(&server, "/api/models/7").await, (404, "api".into()));

    server.shutdown().await;
}

/// The deeper prefix is the more specific claim, and the router reads a literal segment
/// before a catch-all at every position, so the deepest fallback answers - ahead of a
/// catch-all route mapped under `/`, too.
#[tokio::test]
async fn it_answers_with_the_fallback_of_the_deepest_prefix() {
    let server = TestServer::spawn(|app| {
        app.map_get("/{*path}", |path: String| async move { ok!("root {path}") });

        app.group("/api", |api| {
            api.map_fallback(|| async { not_found!("api") });
            api.group("/v2", |v2| {
                v2.map_fallback(|| async { not_found!("v2") });
            });
        });
    })
    .await;

    assert_eq!(
        get(&server, "/settings").await,
        (200, "root settings".into())
    );
    assert_eq!(get(&server, "/apis/x").await, (200, "root apis/x".into()));
    assert_eq!(get(&server, "/api/nope").await, (404, "api".into()));
    assert_eq!(get(&server, "/api/v1/nope").await, (404, "api".into()));
    assert_eq!(get(&server, "/api/v2").await, (404, "v2".into()));
    assert_eq!(get(&server, "/api/v2/nope").await, (404, "v2".into()));

    // The root catch-all answers `GET` alone, and the API still answers every method
    let (status, _, allow) = send(&server, Method::POST, "/settings").await;
    assert_eq!((status, allow.as_deref()), (405, Some("GET,HEAD")));

    let (status, body, _) = send(&server, Method::POST, "/api/nope").await;
    assert_eq!((status, body.as_str()), (404, "api"));

    server.shutdown().await;
}

/// The scenario #257 opens with: an SPA served under `/` next to an API under `/api`.
#[cfg(feature = "static-files")]
#[tokio::test]
async fn it_answers_under_the_api_prefix_ahead_of_the_fallback_file() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    std::fs::write(root.join("index.html"), "SHELL").unwrap();

    let server = TestServer::builder()
        .configure(move |app| {
            app.with_host_env(|env| {
                env.with_content_root(&root)
                    .with_fallback_file("index.html")
            })
        })
        .setup(|app| {
            app.use_static_files();
            app.group("/api", |api| {
                api.map_get("/models", || async { ok!("models") });
                api.map_fallback(|| async { not_found!("api") });
            });
        })
        .build()
        .await;

    assert_eq!(
        get(&server, "/settings/profile").await,
        (200, "SHELL".into())
    );
    assert_eq!(get(&server, "/api/models").await, (200, "models".into()));
    assert_eq!(get(&server, "/api/nope").await, (404, "api".into()));
    assert_eq!(get(&server, "/api").await, (404, "api".into()));

    let (status, body, _) = send(&server, Method::PUT, "/api/nope").await;
    assert_eq!((status, body.as_str()), (404, "api"));

    server.shutdown().await;
}

/// A fallback reads the parameters of the prefix it answers under, as a route does.
#[tokio::test]
async fn it_binds_the_parameters_of_a_parameterized_prefix() {
    #[derive(Deserialize)]
    struct Tenant {
        tenant: String,
    }

    let server = TestServer::spawn(|app| {
        app.group("/tenants/{tenant}", |tenant| {
            tenant.map_get("/users", || async { ok!("users") });
            tenant.map_fallback(|params: NamedPath<Tenant>| async move {
                not_found!("nothing here for {}", params.tenant)
            });
        });
    })
    .await;

    assert_eq!(
        get(&server, "/tenants/acme/users").await,
        (200, "users".into())
    );
    for path in [
        "/tenants/acme",
        "/tenants/acme/nope",
        "/tenants/acme/users/7",
    ] {
        assert_eq!(
            get(&server, path).await,
            (404, "nothing here for acme".into()),
            "{path}"
        );
    }

    server.shutdown().await;
}

/// A second fallback at one prefix replaces the first, as a second handler for one route
/// does.
#[tokio::test]
async fn it_replaces_a_fallback_mapped_again() {
    let server = TestServer::spawn(|app| {
        app.group("/api", |api| {
            api.map_fallback(|| async { not_found!("first") });
        });
        app.group("/api", |api| {
            api.map_fallback(|| async { not_found!("second") });
        });
    })
    .await;

    assert_eq!(get(&server, "/api/nope").await, (404, "second".into()));
    assert_eq!(get(&server, "/api").await, (404, "second".into()));

    server.shutdown().await;
}

/// The application's fallback keeps answering whatever no group claimed.
#[tokio::test]
async fn it_leaves_the_rest_to_the_application_fallback() {
    let server = TestServer::spawn(|app| {
        app.map_fallback(|| async { not_found!("app") });
        app.group("/api", |api| {
            api.map_fallback(|| async { not_found!("api") });
        });
    })
    .await;

    assert_eq!(get(&server, "/api/nope").await, (404, "api".into()));
    assert_eq!(get(&server, "/nope").await, (404, "app".into()));

    server.shutdown().await;
}

/// The group's middleware - and that of every group around it - runs around its fallback,
/// whether it was added before the fallback or after, so an unknown path under a guarded
/// prefix is refused the way a known one is.
#[cfg(feature = "middleware")]
#[tokio::test]
async fn it_runs_the_group_middleware_around_its_fallback() {
    let server = TestServer::spawn(|app| {
        app.group("/api", |api| {
            api.group("/v2", |v2| {
                v2.map_fallback(|| async { not_found!("v2") });
            });
            api.map_fallback(|| async { not_found!("api") });

            api.filter(
                |headers: HttpHeaders| async move { headers.get_raw("x-api-key").is_some() },
            );
            api.map_ok(|mut resp: HttpResponse| async move {
                resp.insert_header(Header::<ServedBy>::from_static("api"));
                resp
            });
        });
    })
    .await;

    for path in ["/api/nope", "/api", "/api/v2/nope"] {
        let (status, _, _) = send(&server, Method::DELETE, path).await;
        assert_eq!(status, 400, "{path}");
    }

    for (path, body) in [
        ("/api/nope", "api"),
        ("/api", "api"),
        ("/api/v2/nope", "v2"),
    ] {
        let response = server
            .client()
            .delete(server.url(path))
            .header("x-api-key", "secret")
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), 404, "{path}");
        assert_eq!(
            response.headers().get("x-served-by").unwrap(),
            "api",
            "{path}"
        );
        assert_eq!(response.text().await.unwrap(), body, "{path}");
    }

    server.shutdown().await;
}

/// Two sub-groups naming one position differently map their fallbacks at one resource - the
/// later replaces the earlier - and the middleware of the group around them runs around the
/// one fallback left there once, not once per spelling.
#[cfg(feature = "middleware")]
#[tokio::test]
async fn it_runs_the_group_middleware_once_for_a_fallback_mapped_under_two_spellings() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    let runs = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&runs);

    let server = TestServer::spawn(move |app| {
        app.group("/api", move |api| {
            api.wrap(move |ctx, next| {
                let counter = Arc::clone(&counter);
                async move {
                    counter.fetch_add(1, Ordering::SeqCst);
                    next(ctx).await
                }
            });
            api.group("/{tenant}", |g| {
                g.map_fallback(|| async { not_found!("tenant") });
            });
            api.group("/{org}", |g| {
                g.map_fallback(|| async { not_found!("org") });
            });
        });
    })
    .await;

    for path in ["/api/acme/nope", "/api/acme"] {
        runs.store(0, Ordering::SeqCst);

        assert_eq!(get(&server, path).await, (404, "org".into()), "{path}");
        assert_eq!(runs.load(Ordering::SeqCst), 1, "{path}");
    }

    server.shutdown().await;
}

/// A fallback is not a route, so middleware reading `matched_route` tells a request it
/// answers apart from one a route answers - the group's own middleware included.
#[cfg(feature = "middleware")]
#[tokio::test]
async fn it_does_not_report_a_fallback_as_a_matched_route() {
    use std::sync::{Arc, Mutex};

    let seen: Arc<Mutex<Vec<(&'static str, bool)>>> = Arc::new(Mutex::new(Vec::new()));
    let global = Arc::clone(&seen);
    let group = Arc::clone(&seen);

    let server = TestServer::builder()
        .setup(move |app| {
            let global = Arc::clone(&global);
            app.wrap(move |ctx, next| {
                let global = Arc::clone(&global);
                async move {
                    global.lock().unwrap().push(("global", ctx.matched_route()));
                    next(ctx).await
                }
            });

            let group = Arc::clone(&group);
            app.group("/api", move |api| {
                api.wrap(move |ctx, next| {
                    let group = Arc::clone(&group);
                    async move {
                        group.lock().unwrap().push(("group", ctx.matched_route()));
                        next(ctx).await
                    }
                });
                api.map_get("/models", || async { ok!("models") });
                api.map_fallback(|| async { not_found!("api") });
            });
        })
        .build()
        .await;

    assert_eq!(get(&server, "/api/models").await.0, 200);
    assert_eq!(
        std::mem::take(&mut *seen.lock().unwrap()),
        vec![("global", true), ("group", true)]
    );

    assert_eq!(get(&server, "/api/nope").await.0, 404);
    assert_eq!(
        std::mem::take(&mut *seen.lock().unwrap()),
        vec![("global", false), ("group", false)]
    );

    server.shutdown().await;
}

/// A preflight is answered for an endpoint that exists, and a fallback answering every
/// method under a prefix is not one - so the preflight reaches the fallback, which answers
/// it the way it answers anything else, with the CORS headers on the way out.
#[cfg(feature = "middleware")]
#[tokio::test]
async fn it_does_not_answer_a_preflight_for_a_path_only_a_fallback_answers() {
    let server = TestServer::builder()
        .configure(|app| app.with_cors(|cors| cors.with_any_origin().with_any_method()))
        .setup(|app| {
            app.use_cors();
            app.group("/api", |api| {
                api.map_get("/models", || async { ok!("models") });
                api.map_fallback(|| async { not_found!("api") });
            });
        })
        .build()
        .await;

    let preflight = |path: &'static str| {
        server
            .client()
            .request(Method::OPTIONS, server.url(path))
            .header(&ORIGIN, "http://example.test")
            .header(ACCESS_CONTROL_REQUEST_METHOD, "GET")
            .send()
    };

    let response = preflight("/api/models").await.unwrap();
    assert_eq!(response.status(), 204);

    let response = preflight("/api/nope").await.unwrap();
    assert_eq!(response.status(), 404);
    assert_eq!(
        response
            .headers()
            .get(&ACCESS_CONTROL_ALLOW_ORIGIN)
            .unwrap(),
        "*"
    );
    assert_eq!(response.text().await.unwrap(), "api");

    server.shutdown().await;
}
