#![allow(missing_docs)]
#![cfg(feature = "test")]

use reqwest::Method;
use volga::{HttpRequest, Results};
use volga::test::TestServer;

#[tokio::test]
async fn it_maps_to_get_request() {
    let server = TestServer::spawn(|app| {
        app.map_get("/test", || async {
            Results::text("Pass!")
        });
    }).await;

    let response = server.client()
        .get(server.url("/test"))
        .send()
        .await
        .unwrap();

    assert!(response.status().is_success());
    assert_eq!(response.text().await.unwrap(), "Pass!");
    
    server.shutdown().await;
}

#[tokio::test]
async fn it_maps_to_post_request() {
    let server = TestServer::spawn(|app| {
        app.map_post("/test", || async {
            Results::text("Pass!")
        });
    }).await;
    
    let response = server.client()
        .post(server.url("/test"))
        .send()
        .await
        .unwrap();

    assert!(response.status().is_success());
    assert_eq!(response.text().await.unwrap(), "Pass!");
    
    server.shutdown().await;
}

#[tokio::test]
async fn it_maps_to_put_request() {
    let server = TestServer::spawn(|app| {
        app.map_put("/test", || async {
            Results::text("Pass!")
        });
    }).await;

    let response = server.client()
        .put(server.url("/test"))
        .send()
        .await
        .unwrap();

    assert!(response.status().is_success());
    assert_eq!(response.text().await.unwrap(), "Pass!");
    
    server.shutdown().await;
}

#[tokio::test]
async fn it_maps_to_patch_request() {
    let server = TestServer::spawn(|app| {
        app.map_patch("/test", || async {
            Results::text("Pass!")
        });
    }).await;

    let response = server.client()
        .patch(server.url("/test"))
        .send()
        .await
        .unwrap();

    assert!(response.status().is_success());
    assert_eq!(response.text().await.unwrap(), "Pass!");
    
    server.shutdown().await;
}

#[tokio::test]
async fn it_maps_to_delete_request() {
    let server = TestServer::spawn(|app| {
        app.map_delete("/test", || async {
            Results::text("Pass!")
        });
    }).await;

    let response = server.client()
        .delete(server.url("/test"))
        .send()
        .await
        .unwrap();

    assert!(response.status().is_success());
    assert_eq!(response.text().await.unwrap(), "Pass!");
    
    server.shutdown().await;
}

#[tokio::test]
async fn it_maps_to_head_request() {
    let server = TestServer::spawn(|app| {
        app.map_head("/test", || async {
            Results::ok()
        });
    }).await;

    let response = server.client()
        .head(server.url("/test"))
        .send()
        .await
        .unwrap();

    assert!(response.status().is_success());
    assert_eq!(response.text().await.unwrap(), "");
    
    server.shutdown().await;
}

#[tokio::test]
async fn it_maps_to_options_request() {
    let server = TestServer::spawn(|app| {
        app.map_options("/test", || async {
            Results::ok()
        });
    }).await;

    let response = server.client()
        .request(Method::OPTIONS, server.url("/test"))
        .send()
        .await
        .unwrap();

    assert!(response.status().is_success());
    assert_eq!(response.text().await.unwrap(), "");
    
    server.shutdown().await;
}

#[tokio::test]
async fn it_maps_to_trace_request() {
    let server = TestServer::spawn(|app| {
        app.map_trace("/test", |req: HttpRequest| async {
            let boxed_body = req.into_boxed_body();
            Results::stream(boxed_body)
        });
    }).await;

    let response = server.client()
        .request(Method::TRACE, server.url("/test"))
        .send()
        .await
        .unwrap();
    
    assert!(response.status().is_success());
    assert_eq!(response.text().await.unwrap(), "");
    
    server.shutdown().await;
}

#[tokio::test]
async fn it_maps_to_head_along_with_get_request() {
    let server = TestServer::spawn(|app| {
        app.map_get("/test", || async {
            Results::text("Pass!")
        });
    }).await;

    let response = server.client()
        .head(server.url("/test"))
        .send()
        .await
        .unwrap();

    assert!(response.status().is_success());
    assert_eq!(response.headers().get("Content-Length").unwrap(), "5");
    assert_eq!(response.text().await.unwrap(), "");
    
    server.shutdown().await;
}

#[tokio::test]
async fn it_ignores_head_along_with_get_request_if_disabled_explicitly() {
    let server = TestServer::builder()
        .configure(|app| app.without_implicit_head())
        .setup(|app| {
            app.map_get("/test", || async {
                Results::text("Pass!")
            });
        })
        .build()
        .await;

    let response = server.client()
        .head(server.url("/test"))
        .send()
        .await
        .unwrap();

    assert!(response.status().is_client_error());
    assert_eq!(response.status(), 405);

    server.shutdown().await;
}

#[tokio::test]
async fn it_overrides_default_head_map() {
    let server = TestServer::spawn(|app| {
        app.map_head("/test", || async {
            volga::ok!([
                ("x-header", "Hello from HEAD")
            ])
        });
        app.map_get("/test", || async {
            volga::ok!("Pass!", [
                ("x-header", "Hello from GET")
            ])
        });
    }).await;

    let response = server.client()
        .head(server.url("/test"))
        .send()
        .await
        .unwrap();

    assert!(response.status().is_success());
    assert_eq!(response.headers().get("x-header").unwrap(), "Hello from HEAD");
    assert_eq!(response.text().await.unwrap(), "");
    
    server.shutdown().await;
}