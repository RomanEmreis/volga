#![allow(missing_docs)]
#![cfg(all(feature = "test", feature = "middleware"))]

//! The implicit `HEAD` endpoint that comes with a `GET` route answers through everything
//! the `GET` answers through: a `HEAD` request is not a way around middleware.

use std::sync::{Arc, Mutex};
use volga::headers::{ACCESS_CONTROL_ALLOW_ORIGIN, ORIGIN};
use volga::http::{Method, StatusCode};
use volga::test::TestServer;

#[tokio::test]
async fn it_runs_route_middleware_for_an_implicit_head_request() {
    let server = TestServer::spawn(|app| {
        app.map_get("/test", || async { "Pass!" })
            .wrap(|_ctx, _next| async move { volga::status!(403) });
    })
    .await;

    for method in [Method::GET, Method::HEAD] {
        let response = server
            .client()
            .request(method.clone(), server.url("/test"))
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::FORBIDDEN, "{method}");
    }

    server.shutdown().await;
}

#[tokio::test]
async fn it_runs_group_middleware_for_an_implicit_head_request() {
    let server = TestServer::spawn(|app| {
        app.group("/api", |api| {
            api.map_get("/test", || async { "Pass!" });
            api.wrap(|_ctx, _next| async move { volga::status!(403) });
        });
    })
    .await;

    for method in [Method::GET, Method::HEAD] {
        let response = server
            .client()
            .request(method.clone(), server.url("/api/test"))
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::FORBIDDEN, "{method}");
    }

    server.shutdown().await;
}

#[tokio::test]
async fn it_applies_a_route_cors_policy_to_an_implicit_head_request() {
    let server = TestServer::builder()
        .configure(|app| {
            app.with_cors(|cors| {
                cors.with_name("api")
                    .with_origins(["https://example.test"])
                    .with_methods([Method::GET, Method::HEAD])
            })
        })
        .setup(|app| {
            app.use_cors();

            app.map_get("/route", || async { "Pass!" }).cors_with("api");

            app.group("/api", |api| {
                api.map_get("/group", || async { "Pass!" });
                api.cors_with("api");
            });
        })
        .build()
        .await;

    for path in ["/route", "/api/group"] {
        let response = server
            .client()
            .head(server.url(path))
            .header(&ORIGIN, "https://example.test")
            .send()
            .await
            .unwrap();

        assert!(response.status().is_success(), "{path}: {response:?}");
        assert_eq!(
            response
                .headers()
                .get(&ACCESS_CONTROL_ALLOW_ORIGIN)
                .map(|value| value.to_str().unwrap()),
            Some("https://example.test"),
            "{path}"
        );
    }

    server.shutdown().await;
}

/// A `HEAD` mapped by hand is a route of its own: it is not the `GET` route's twin,
/// and nothing the `GET` route carries is bound to it. Mapping it replaces the twin,
/// so this holds whichever of the two is mapped first.
#[tokio::test]
async fn it_leaves_an_explicitly_mapped_head_route_alone() {
    for head_first in [true, false] {
        let server = TestServer::spawn(move |app| {
            if head_first {
                app.map_head("/test", || async { volga::ok!([("x-who", "head")]) });
            }

            app.map_get("/test", || async { "Pass!" })
                .wrap(|_ctx, _next| async move { volga::status!(403) });

            if !head_first {
                app.map_head("/test", || async { volga::ok!([("x-who", "head")]) });
            }
        })
        .await;

        let head = server
            .client()
            .head(server.url("/test"))
            .send()
            .await
            .unwrap();
        let get = server
            .client()
            .get(server.url("/test"))
            .send()
            .await
            .unwrap();

        assert!(head.status().is_success(), "head_first={head_first}");
        assert_eq!(
            head.headers()
                .get("x-who")
                .map(|value| value.to_str().unwrap()),
            Some("head"),
            "head_first={head_first}"
        );
        assert_eq!(
            get.status(),
            StatusCode::FORBIDDEN,
            "head_first={head_first}"
        );

        server.shutdown().await;
    }
}

/// A group configures the twin by looking it up once its closure returns, so a `HEAD`
/// the group mapped by hand - which replaced the twin - is configured once, not twice.
#[tokio::test]
async fn it_runs_group_middleware_once_for_a_head_mapped_by_hand() {
    let hits = Arc::new(Mutex::new(0usize));
    let group_hits = Arc::clone(&hits);

    let server = TestServer::spawn(move |app| {
        app.group("/api", |api| {
            api.map_get("/test", || async { "Pass!" });
            api.map_head("/test", || async { volga::ok!() });

            let counter = Arc::clone(&group_hits);
            api.wrap(move |ctx, next| {
                let counter = Arc::clone(&counter);
                async move {
                    *counter.lock().expect("counter is not poisoned") += 1;
                    next(ctx).await
                }
            });
        });
    })
    .await;

    for method in [Method::HEAD, Method::GET] {
        *hits.lock().expect("counter is not poisoned") = 0;

        let response = server
            .client()
            .request(method.clone(), server.url("/api/test"))
            .send()
            .await
            .unwrap();

        assert!(response.status().is_success(), "{method}");
        assert_eq!(
            *hits.lock().expect("counter is not poisoned"),
            1,
            "{method}"
        );
    }

    server.shutdown().await;
}

/// A dynamic segment is one route whatever its placeholder is called, so a `HEAD` mapped
/// under a different name for it is still the `GET` route's own `HEAD`.
#[tokio::test]
async fn it_matches_an_explicit_head_by_route_shape() {
    let hits = Arc::new(Mutex::new(0usize));
    let group_hits = Arc::clone(&hits);

    let server = TestServer::spawn(move |app| {
        app.map_get("/users/{id}", |id: String| async move { id })
            .wrap(|_ctx, _next| async move { volga::status!(403) });
        app.map_head("/users/{name}", || async {
            volga::ok!([("x-who", "head")])
        });

        app.group("/api", |api| {
            api.map_get("/users/{id}", |id: String| async move { id });
            api.map_head("/users/{name}", || async { volga::ok!() });

            let counter = Arc::clone(&group_hits);
            api.wrap(move |ctx, next| {
                let counter = Arc::clone(&counter);
                async move {
                    *counter.lock().expect("counter is not poisoned") += 1;
                    next(ctx).await
                }
            });
        });
    })
    .await;

    // The route's middleware stays with the route, and the HEAD mapped by hand answers
    let head = server
        .client()
        .head(server.url("/users/x"))
        .send()
        .await
        .unwrap();

    assert!(head.status().is_success());
    assert_eq!(
        head.headers()
            .get("x-who")
            .map(|value| value.to_str().unwrap()),
        Some("head")
    );

    // ... and the group configures that HEAD once, as a route of its own
    let head = server
        .client()
        .head(server.url("/api/users/x"))
        .send()
        .await
        .unwrap();

    assert!(head.status().is_success());
    assert_eq!(*hits.lock().expect("counter is not poisoned"), 1);

    server.shutdown().await;
}

/// Only a `HEAD` mapped by hand takes the pattern over from the endpoint standing in
/// for the `GET`; another verb on the same pattern is a route of its own and leaves it
/// where it is.
#[tokio::test]
async fn it_keeps_the_twin_when_another_verb_is_mapped_on_the_pattern() {
    let server = TestServer::spawn(|app| {
        app.group("/api", |api| {
            api.map_get("/test", || async { "Pass!" });
            api.map_post("/test", || async { "Pass!" });
            api.wrap(|_ctx, _next| async move { volga::status!(403) });
        });
    })
    .await;

    for method in [Method::GET, Method::HEAD, Method::POST] {
        let response = server
            .client()
            .request(method.clone(), server.url("/api/test"))
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::FORBIDDEN, "{method}");
    }

    server.shutdown().await;
}

/// The twin is registered after the route is in the tree: looking for an existing `HEAD`
/// before that resolves a static path through the dynamic route covering it, and reports
/// that route's `HEAD` as this one's.
#[tokio::test]
async fn it_maps_a_twin_for_a_static_route_shadowed_by_a_dynamic_one() {
    let server = TestServer::spawn(|app| {
        app.map_get("/users/{id}", |id: String| async move { id });
        app.map_get("/users/alice", || async { "alice" });
    })
    .await;

    for path in ["/users/alice", "/users/bob"] {
        let response = server.client().head(server.url(path)).send().await.unwrap();

        assert!(response.status().is_success(), "{path}: {response:?}");
    }

    server.shutdown().await;
}

#[tokio::test]
async fn it_maps_no_implicit_head_when_disabled_explicitly() {
    let server = TestServer::builder()
        .configure(|app| app.without_implicit_head())
        .setup(|app| {
            app.group("/api", |api| {
                api.map_get("/test", || async { "Pass!" });
                api.wrap(|_ctx, _next| async move { volga::status!(403) });
            });
        })
        .build()
        .await;

    let response = server
        .client()
        .head(server.url("/api/test"))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);

    server.shutdown().await;
}
