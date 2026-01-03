#![allow(missing_docs)]

use volga::headers::{
    ACCESS_CONTROL_ALLOW_ORIGIN,
    ACCESS_CONTROL_ALLOW_HEADERS,
    ACCESS_CONTROL_ALLOW_METHODS,
    ORIGIN,
};
use volga::http::{Method, StatusCode};

mod common;
use common::TestServer;

#[tokio::test]
async fn it_adds_access_control_allow_origin_header() {
    let server = TestServer::builder()
        .with_app(|app| app
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
    assert_eq!(response.headers().get(ACCESS_CONTROL_ALLOW_ORIGIN).unwrap(), "http://127.0.0.1");
    
    server.shutdown().await;
}

#[tokio::test]
async fn it_adds_access_control_headers() {
    let server = TestServer::builder()
        .with_app(|app| app
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
        .send()
        .await
        .unwrap();
    
    assert!(response.status().is_success());
    
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert_eq!(response.headers().get(&ACCESS_CONTROL_ALLOW_ORIGIN).unwrap(), "http://127.0.0.1");
    assert_eq!(response.headers().get(&ACCESS_CONTROL_ALLOW_HEADERS).unwrap(), "*");
    assert_eq!(response.headers().get(&ACCESS_CONTROL_ALLOW_METHODS).unwrap(), "PUT");
    
    server.shutdown().await;
}
