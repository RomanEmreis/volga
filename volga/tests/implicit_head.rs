#![allow(missing_docs)]
#![cfg(all(feature = "test", feature = "middleware"))]

//! The implicit `HEAD` endpoint that comes with a `GET` route answers through everything
//! the `GET` answers through: a `HEAD` request is not a way around middleware.

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
/// and nothing the `GET` route carries is bound to it.
#[tokio::test]
async fn it_leaves_an_explicitly_mapped_head_route_alone() {
    let server = TestServer::spawn(|app| {
        app.map_head("/test", || async { volga::ok!() });
        app.map_get("/test", || async { "Pass!" })
            .wrap(|_ctx, _next| async move { volga::status!(403) });
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

    assert!(head.status().is_success());
    assert_eq!(get.status(), StatusCode::FORBIDDEN);

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
