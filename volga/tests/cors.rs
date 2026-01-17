#![allow(missing_docs)]
#![cfg(all(feature = "test", feature = "middleware"))]

use volga::headers::{
    ACCESS_CONTROL_ALLOW_ORIGIN,
    ACCESS_CONTROL_ALLOW_HEADERS,
    ACCESS_CONTROL_ALLOW_METHODS,
    ACCESS_CONTROL_REQUEST_METHOD,
    ORIGIN,
    VARY
};
use volga::http::{Method, StatusCode};
use volga::test::TestServer;

#[tokio::test]
async fn it_adds_access_control_allow_origin_header() {
    let server = TestServer::builder()
        .configure(|app| app
            .with_cors(|cors| cors.with_origins(["http://127.0.0.1"])))
        .setup(|app| {
            app.use_cors();
            app.map_put("/test", || async {});
        })
        .build()
        .await;

    let response = server.client()
        .put(server.url("/test"))
        .header(ORIGIN, "http://127.0.0.1")
        .send()
        .await
        .unwrap();

    assert!(response.status().is_success());
    assert_eq!(response.headers().get(&ACCESS_CONTROL_ALLOW_ORIGIN).unwrap(), "http://127.0.0.1");
    assert_eq!(response.headers().get(&VARY).unwrap(), "Origin");
    
    server.shutdown().await;
}

#[tokio::test]
async fn it_adds_access_control_headers() {
    let server = TestServer::builder()
        .configure(|app| app
            .with_cors(|cors| cors
            .with_origins(["http://127.0.0.1"])
            .with_methods([Method::PUT])
            .with_any_header()))
        .setup(|app| {
            app.use_cors();
            app.map_put("/test", || async {});
        })
        .build()
        .await;

    let response = server.client()
        .request(Method::OPTIONS, server.url("/test"))
        .header(ORIGIN, "http://127.0.0.1")
        .header(ACCESS_CONTROL_REQUEST_METHOD, "PUT")
        .send()
        .await
        .unwrap();

    assert!(response.status().is_success());
    
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert_eq!(response.headers().get(&ACCESS_CONTROL_ALLOW_ORIGIN).unwrap(), "http://127.0.0.1");
    assert_eq!(response.headers().get(&ACCESS_CONTROL_ALLOW_HEADERS).unwrap(), "*");
    assert_eq!(response.headers().get(&ACCESS_CONTROL_ALLOW_METHODS).unwrap(), "PUT");
    assert_eq!(response.headers().get(&VARY).unwrap(), "origin, access-control-request-method, access-control-request-headers");
    
    server.shutdown().await;
}

#[tokio::test]
async fn it_disables_cors_for_route() {
    let server = TestServer::builder()
        .configure(|app| app
            .with_cors(|cors| cors
                .with_origins(["http://127.0.0.1"])
                .with_methods([Method::PUT])
                .with_any_header()))
        .setup(|app| {
            app.use_cors();
            app
                .map_put("/test", || async {})
                .disable_cors();
        })
        .build()
        .await;

    let response = server.client()
        .put(server.url("/test"))
        .send()
        .await
        .unwrap();

    assert!(response.status().is_success());
    assert!(response.headers().get(&ACCESS_CONTROL_ALLOW_ORIGIN).is_none());
    assert!(response.headers().get(&VARY).is_none());

    server.shutdown().await;
}

#[tokio::test]
async fn it_disables_cors_for_route_group() {
    let server = TestServer::builder()
        .configure(|app| app
            .with_cors(|cors| cors
                .with_origins(["http://127.0.0.1"])
                .with_methods([Method::PUT])
                .with_any_header()))
        .setup(|app| {
            app.use_cors();
            app.group("/v1", |g| {
                g.disable_cors();
                g.map_put("/test", || async {});
            });
        })
        .build()
        .await;

    let response = server.client()
        .put(server.url("/v1/test"))
        .send()
        .await
        .unwrap();

    assert!(response.status().is_success());
    assert!(response.headers().get(&ACCESS_CONTROL_ALLOW_ORIGIN).is_none());
    assert!(response.headers().get(&VARY).is_none());

    server.shutdown().await;
}

#[tokio::test]
async fn it_disables_cors_for_route_because_there_is_no_default_policy() {
    let server = TestServer::builder()
        .configure(|app| app
            .with_cors(|cors| cors
                .with_name("policy")
                .with_origins(["http://127.0.0.1"])
                .with_methods([Method::PUT])
                .with_any_header()))
        .setup(|app| {
            app.use_cors();
            app
                .map_put("/test", || async {});
        })
        .build()
        .await;

    let response = server.client()
        .put(server.url("/test"))
        .send()
        .await
        .unwrap();

    assert!(response.status().is_success());
    assert!(response.headers().get(&ACCESS_CONTROL_ALLOW_ORIGIN).is_none());
    assert!(response.headers().get(&VARY).is_none());

    server.shutdown().await;
}

#[tokio::test]
async fn it_disables_cors_for_route_group_because_there_is_no_default_policy() {
    let server = TestServer::builder()
        .configure(|app| app
            .with_cors(|cors| cors
                .with_name("policy")
                .with_origins(["http://127.0.0.1"])
                .with_methods([Method::PUT])
                .with_any_header()))
        .setup(|app| {
            app.use_cors();
            app.group("/v1", |g| {
                g.map_put("/test", || async {});
            });
        })
        .build()
        .await;

    let response = server.client()
        .put(server.url("/v1/test"))
        .send()
        .await
        .unwrap();

    assert!(response.status().is_success());
    assert!(response.headers().get(&ACCESS_CONTROL_ALLOW_ORIGIN).is_none());
    assert!(response.headers().get(&VARY).is_none());

    server.shutdown().await;
}

#[tokio::test]
async fn it_applies_named_cors_for_route() {
    let server = TestServer::builder()
        .configure(|app| app
            .with_cors(|cors| cors
                .with_name("policy")
                .with_origins(["http://127.0.0.1"])
                .with_methods([Method::PUT])
                .with_any_header())
            .with_cors(|cors| cors
                .with_origins(["http://192.168.1.1"])
                .with_methods([Method::GET])
                .with_vary_header(false)
                .with_any_header()))
        .setup(|app| {
            app.use_cors();
            app
                .map_put("/test", || async {})
                .cors_with("policy");
        })
        .build()
        .await;

    let response = server.client()
        .put(server.url("/test"))
        .header(ORIGIN, "http://127.0.0.1")
        .send()
        .await
        .unwrap();

    assert!(response.status().is_success());
    assert_eq!(response.headers().get(&ACCESS_CONTROL_ALLOW_ORIGIN).unwrap(), "http://127.0.0.1");
    assert_eq!(response.headers().get(&VARY).unwrap(), "Origin");

    server.shutdown().await;
}

#[tokio::test]
async fn it_applies_named_cors_for_route_group() {
    let server = TestServer::builder()
        .configure(|app| app
            .with_cors(|cors| cors
                .with_name("policy")
                .with_origins(["http://127.0.0.1"])
                .with_methods([Method::PUT])
                .with_any_header())
            .with_cors(|cors| cors
                .with_origins(["http://192.168.1.1"])
                .with_methods([Method::GET])
                .with_vary_header(false)
                .with_any_header()))
        .setup(|app| {
            app.use_cors();
            app.group("/v1", |g| {
                g.cors_with("policy");
                g.map_put("/test", || async {});
            });
        })
        .build()
        .await;

    let response = server.client()
        .put(server.url("/v1/test"))
        .header(ORIGIN, "http://127.0.0.1")
        .send()
        .await
        .unwrap();

    assert!(response.status().is_success());
    assert_eq!(response.headers().get(&ACCESS_CONTROL_ALLOW_ORIGIN).unwrap(), "http://127.0.0.1");
    assert_eq!(response.headers().get(&VARY).unwrap(), "Origin");

    server.shutdown().await;
}

#[tokio::test]
async fn it_prefers_route_cors_policy_over_the_route_group() {
    let server = TestServer::builder()
        .configure(|app| app
            .with_cors(|cors| cors
                .with_name("policy")
                .with_origins(["http://192.168.1.1"])
                .with_methods([Method::PUT])
                .with_any_header())
            .with_cors(|cors| cors
                .with_origins(["http://127.0.0.1"])
                .with_methods([Method::GET])
                .with_vary_header(false)
                .with_any_header()))
        .setup(|app| {
            app.use_cors();
            app.group("/v1", |g| {
                g.cors_with("policy");
                g.map_get("/test", || async {}).cors();
            });
        })
        .build()
        .await;

    let response = server.client()
        .get(server.url("/v1/test"))
        .header(ORIGIN, "http://127.0.0.1")
        .send()
        .await
        .unwrap();

    assert!(response.status().is_success());
    
    assert_eq!(response.headers().get(&ACCESS_CONTROL_ALLOW_ORIGIN).unwrap(), "http://127.0.0.1");
    assert!(response.headers().get(&VARY).is_none());

    server.shutdown().await;
}