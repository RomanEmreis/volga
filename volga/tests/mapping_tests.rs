#![allow(missing_docs)]
#![cfg(feature = "test")]

use reqwest::Method;
use volga::test::TestServer;
use volga::{HttpRequest, stream};

#[tokio::test]
async fn it_maps_to_get_request() {
    let server = TestServer::spawn(|app| {
        app.map_get("/test", async || "Pass!");
    })
    .await;

    let response = server
        .client()
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
        app.map_post("/test", async || "Pass!");
    })
    .await;

    let response = server
        .client()
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
        app.map_put("/test", async || "Pass!");
    })
    .await;

    let response = server
        .client()
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
        app.map_patch("/test", async || "Pass!");
    })
    .await;

    let response = server
        .client()
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
        app.map_delete("/test", async || "Pass!");
    })
    .await;

    let response = server
        .client()
        .delete(server.url("/test"))
        .send()
        .await
        .unwrap();

    assert!(response.status().is_success());
    assert_eq!(response.text().await.unwrap(), "Pass!");

    server.shutdown().await;
}

/// Mapping a route that is already mapped replaces it, and the layers bound to the
/// registration being replaced go with it.
#[cfg(feature = "middleware")]
#[tokio::test]
async fn it_replaces_a_route_that_is_mapped_again() {
    let server = TestServer::spawn(|app| {
        app.map_get("/test", || async { "first" })
            .wrap(|_ctx, _next| async move { volga::status!(403) });
        app.map_get("/test", || async { "second" });
    })
    .await;

    let response = server
        .client()
        .get(server.url("/test"))
        .send()
        .await
        .unwrap();

    assert!(response.status().is_success());
    assert_eq!(response.text().await.unwrap(), "second");

    server.shutdown().await;
}

/// Two paths that name one route are one route everywhere it is remembered, so the
/// operation describes the registration that answers rather than one it replaced.
#[cfg(all(feature = "middleware", feature = "openapi"))]
#[tokio::test]
async fn it_describes_the_route_that_answers_when_a_path_is_mapped_again() {
    let server = TestServer::builder()
        .configure(|app| app.with_open_api(|open_api| open_api))
        .setup(|app| {
            app.use_open_api();

            app.map_get("/test", || async { "first" })
                .open_api(|op| op.with_summary("first registration"));
            app.map_get("/test/", || async { "second" });
        })
        .build()
        .await;

    let response = server
        .client()
        .get(server.url("/test"))
        .send()
        .await
        .unwrap();

    assert!(response.status().is_success());
    assert_eq!(response.text().await.unwrap(), "second");

    let spec: serde_json::Value = server
        .client()
        .get(server.url("/openapi.json"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    assert!(spec["paths"]["/test"]["get"].is_object());
    assert_eq!(spec["paths"]["/test/"], serde_json::Value::Null);
    assert_eq!(
        spec["paths"]["/test"]["get"]["summary"],
        serde_json::Value::Null
    );

    server.shutdown().await;
}

#[tokio::test]
async fn it_maps_to_head_request() {
    let server = TestServer::spawn(|app| {
        app.map_head("/test", async || "Pass!");
    })
    .await;

    let response = server
        .client()
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
        app.map_options("/test", async || {});
    })
    .await;

    let response = server
        .client()
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
            stream!(req.into_body().into_data_stream())
        });
    })
    .await;

    let response = server
        .client()
        .request(Method::TRACE, server.url("/test"))
        .send()
        .await
        .unwrap();

    assert!(response.status().is_success());
    assert_eq!(response.text().await.unwrap(), "");

    server.shutdown().await;
}

#[tokio::test]
async fn it_maps_to_query_request() {
    let server = TestServer::spawn(|app| {
        app.map_query("/test", async || "Pass!");
    })
    .await;

    let response = server
        .client()
        .request(Method::from_bytes(b"QUERY").unwrap(), server.url("/test"))
        .send()
        .await
        .unwrap();

    assert!(response.status().is_success());
    assert_eq!(response.text().await.unwrap(), "Pass!");

    server.shutdown().await;
}

#[tokio::test]
async fn it_maps_to_query_request_in_group() {
    let server = TestServer::spawn(|app| {
        app.group("/test", |api| {
            api.map_query("/test", async || "Pass!");
        });
    })
    .await;

    let response = server
        .client()
        .request(
            Method::from_bytes(b"QUERY").unwrap(),
            server.url("/test/test"),
        )
        .send()
        .await
        .unwrap();

    assert!(response.status().is_success());
    assert_eq!(response.text().await.unwrap(), "Pass!");

    server.shutdown().await;
}

#[tokio::test]
async fn it_maps_with_typed_method() {
    let server = TestServer::spawn(|app| {
        app.map(Method::GET, "/test", async || "Pass!");
    })
    .await;

    let response = server
        .client()
        .get(server.url("/test"))
        .send()
        .await
        .unwrap();

    assert!(response.status().is_success());
    assert_eq!(response.text().await.unwrap(), "Pass!");

    server.shutdown().await;
}

#[tokio::test]
async fn it_maps_with_string_method() {
    let server = TestServer::spawn(|app| {
        app.map("QUERY", "/test", async || "Pass!");
    })
    .await;

    let response = server
        .client()
        .request(Method::from_bytes(b"QUERY").unwrap(), server.url("/test"))
        .send()
        .await
        .unwrap();

    assert!(response.status().is_success());
    assert_eq!(response.text().await.unwrap(), "Pass!");

    server.shutdown().await;
}

#[tokio::test]
async fn it_maps_with_owned_string_pattern() {
    let server = TestServer::spawn(|app| {
        app.map(Method::GET, format!("/test/{}", "v1"), async || "Pass!");
    })
    .await;

    let response = server
        .client()
        .get(server.url("/test/v1"))
        .send()
        .await
        .unwrap();

    assert!(response.status().is_success());
    assert_eq!(response.text().await.unwrap(), "Pass!");

    server.shutdown().await;
}

#[tokio::test]
async fn it_maps_with_method_in_group() {
    let server = TestServer::spawn(|app| {
        app.group("/test", |api| {
            api.map("QUERY", "/test", async || "Pass!");
        });
    })
    .await;

    let response = server
        .client()
        .request(
            Method::from_bytes(b"QUERY").unwrap(),
            server.url("/test/test"),
        )
        .send()
        .await
        .unwrap();

    assert!(response.status().is_success());
    assert_eq!(response.text().await.unwrap(), "Pass!");

    server.shutdown().await;
}

#[tokio::test]
async fn it_maps_to_head_along_with_get_request() {
    let server = TestServer::spawn(|app| {
        app.map_get("/test", async || "Pass!");
    })
    .await;

    let response = server
        .client()
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
async fn it_overrides_default_head_map() {
    let server = TestServer::spawn(|app| {
        app.map_head("/test", || async {
            volga::ok!([("x-header", "Hello from HEAD")])
        });
        app.map_get("/test", || async {
            volga::ok!("Pass!"; [
                ("x-header", "Hello from GET")
            ])
        });
    })
    .await;

    let response = server
        .client()
        .head(server.url("/test"))
        .send()
        .await
        .unwrap();

    assert!(response.status().is_success());
    assert_eq!(
        response.headers().get("x-header").unwrap(),
        "Hello from HEAD"
    );
    assert_eq!(response.text().await.unwrap(), "");

    server.shutdown().await;
}
