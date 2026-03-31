#![allow(missing_docs)]
#![allow(unused)]
#![cfg(all(feature = "test", feature = "middleware"))]

use hyper::StatusCode;
use volga::error::Error;
use volga::headers::{Header, HttpHeaders, headers};
use volga::middleware::{HttpContext, NextFn};
use volga::test::TestServer;
use volga::{HttpRequestMut, HttpResponse, ok};

headers! {
    (XTest, "x-test")
}

#[tokio::test]
async fn it_adds_middleware_request() {
    let server = TestServer::spawn(|app| {
        app.attach(|ctx: HttpContext, next: NextFn| async move { next(ctx).await });
        app.wrap(|_, _| async move { ok!("Pass!") });
        app.map_get("/test", || async { ok!("Unreachable!") });
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
async fn it_adds_map_ok_middleware() {
    let server = TestServer::spawn(|app| {
        app.map_ok(|mut resp: HttpResponse| async move {
            resp.insert_header(Header::<XTest>::try_from("Test").unwrap());
            resp
        });
        app.map_get("/test", || async { ok!("Pass!") });
    })
    .await;

    let response = server
        .client()
        .get(server.url("/test"))
        .send()
        .await
        .unwrap();

    assert!(response.status().is_success());
    assert_eq!(response.headers().get("X-Test").unwrap(), "Test");
    assert_eq!(response.text().await.unwrap(), "Pass!");

    server.shutdown().await;
}

#[tokio::test]
async fn it_adds_map_req_middleware() {
    let server = TestServer::spawn(|app| {
        app.tap_req(|mut req: HttpRequestMut| async move {
            req.insert_header(Header::<XTest>::try_from("Pass!").unwrap());
            req
        });
        app.map_get("/test", |headers: HttpHeaders| async move {
            let val = headers.try_get::<XTest>()?;
            Ok::<_, Error>(val.to_string())
        });
    })
    .await;

    let response = server
        .client()
        .get(server.url("/test"))
        .send()
        .await
        .unwrap();

    assert!(response.status().is_success());
    assert_eq!(response.text().await.unwrap(), "x-test: Pass!");

    server.shutdown().await;
}

#[tokio::test]
async fn it_adds_map_ok_middleware_for_route() {
    let server = TestServer::spawn(|app| {
        app.map_get("/test", async || "Pass!")
            .map_ok(|mut resp: HttpResponse| async move {
                resp.try_insert_header::<XTest>("Test").unwrap();
                resp
            });
    })
    .await;

    let response = server
        .client()
        .get(server.url("/test"))
        .send()
        .await
        .unwrap();

    assert!(response.status().is_success());
    assert_eq!(response.headers().get("X-Test").unwrap(), "Test");
    assert_eq!(response.text().await.unwrap(), "Pass!");

    server.shutdown().await;
}

#[tokio::test]
async fn it_adds_map_req_middleware_for_route() {
    let server = TestServer::spawn(|app| {
        app.map_get("/test", |headers: HttpHeaders| async move {
            let val = headers.try_get::<XTest>()?;
            Ok::<_, Error>(val.to_string())
        })
        .tap_req(|mut req: HttpRequestMut| async move {
            req.insert_header(Header::<XTest>::try_from("Pass!").unwrap());
            req
        });
    })
    .await;

    let response = server
        .client()
        .get(server.url("/test"))
        .send()
        .await
        .unwrap();

    assert!(response.status().is_success());
    assert_eq!(response.text().await.unwrap(), "x-test: Pass!");

    server.shutdown().await;
}

#[tokio::test]
async fn it_adds_map_ok_middleware_for_group() {
    let server = TestServer::spawn(|app| {
        app.group("/tests", |api| {
            api.map_ok(|mut resp: HttpResponse| async move {
                resp.try_insert_header::<XTest>("Test").unwrap();
                resp
            });
            api.map_get("/test", async || "Pass!");
        });
    })
    .await;

    let response = server
        .client()
        .get(server.url("/tests/test"))
        .send()
        .await
        .unwrap();

    assert!(response.status().is_success());
    assert_eq!(response.headers().get("X-Test").unwrap(), "Test");
    assert_eq!(response.text().await.unwrap(), "Pass!");

    server.shutdown().await;
}

#[tokio::test]
async fn it_adds_map_req_middleware_for_group() {
    let server = TestServer::spawn(|app| {
        app.group("/tests", |api| {
            api.tap_req(|mut req: HttpRequestMut| async move {
                req.try_insert_header::<XTest>("Pass!").unwrap();
                req
            });
            api.map_get("/test", |headers: HttpHeaders| async move {
                let val = headers.try_get::<XTest>()?;
                Ok::<_, Error>(val.to_string())
            });
        });
    })
    .await;

    let response = server
        .client()
        .get(server.url("/tests/test"))
        .send()
        .await
        .unwrap();

    assert!(response.status().is_success());
    assert_eq!(response.text().await.unwrap(), "x-test: Pass!");

    server.shutdown().await;
}

#[tokio::test]
async fn it_adds_map_err_middleware_for_route() {
    let server = TestServer::spawn(|app| {
        app.map_err(|err: Error| async move {
            let mut err_str = err.to_string();
            err_str.push_str(" occurred!");
            Error::server_error(err_str)
        })
        .map_get("/test", || async {
            Err::<(), Error>(Error::server_error("Some Error"))
        });
    })
    .await;

    let response = server
        .client()
        .get(server.url("/test"))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(response.text().await.unwrap(), "Some Error occurred!");

    server.shutdown().await;
}

#[tokio::test]
async fn it_adds_map_err_middleware_for_group() {
    let server = TestServer::spawn(|app| {
        app.group("/tests", |api| {
            api.map_err(|err: Error| async move {
                let mut err_str = err.to_string();
                err_str.push_str(" occurred!");
                Error::server_error(err_str)
            });
            api.map_get("/test", || async {
                Err::<(), Error>(Error::server_error("Some Error"))
            });
        });
    })
    .await;

    let response = server
        .client()
        .get(server.url("/tests/test"))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(response.text().await.unwrap(), "Some Error occurred!");

    server.shutdown().await;
}

#[tokio::test]
async fn it_adds_invalid_filter_middleware_for_route() {
    let server = TestServer::spawn(|app| {
        app.map_get("/test", async || ()).filter(async || false);
    })
    .await;

    let response = server
        .client()
        .get(server.url("/test"))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        response.text().await.unwrap(),
        "Validation: One or more request parameters are incorrect"
    );

    server.shutdown().await;
}

#[tokio::test]
async fn it_adds_valid_filter_middleware_for_route() {
    let server = TestServer::spawn(|app| {
        app.map_get("/test", async || "Pass!").filter(async || true);
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
async fn it_adds_invalid_filter_middleware_for_group() {
    let server = TestServer::spawn(|app| {
        app.group("/tests", |api| {
            api.filter(async || false);
            api.map_get("/test", async || ());
        });
    })
    .await;

    let response = server
        .client()
        .get(server.url("/tests/test"))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        response.text().await.unwrap(),
        "Validation: One or more request parameters are incorrect"
    );

    server.shutdown().await;
}

#[tokio::test]
async fn it_adds_valid_filter_middleware_for_group() {
    let server = TestServer::spawn(|app| {
        app.group("/tests", |api| {
            api.filter(async || true);
            api.map_get("/test", async || "Pass!");
        });
    })
    .await;

    let response = server
        .client()
        .get(server.url("/tests/test"))
        .send()
        .await
        .unwrap();

    assert!(response.status().is_success());
    assert_eq!(response.text().await.unwrap(), "Pass!");

    server.shutdown().await;
}

#[tokio::test]
async fn it_adds_with_middleware() {
    let server = TestServer::spawn(|app| {
        app.wrap(|ctx: HttpContext, next: NextFn| async move { next(ctx).await })
            .with(|next| next)
            .map_get("/test", || async { "Pass!" });
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
async fn it_adds_shortcut_with_middleware() {
    let server = TestServer::spawn(|app| {
        app.wrap(async |ctx: HttpContext, next: NextFn| next(ctx).await)
            .with(async |_| volga::bad_request!("Error!"))
            .with(|next| next)
            .map_get("/test", async || "Pass!");
    })
    .await;

    let response = server
        .client()
        .get(server.url("/test"))
        .send()
        .await
        .unwrap();

    assert!(!response.status().is_success());
    assert_eq!(response.text().await.unwrap(), "Error!");

    server.shutdown().await;
}

#[tokio::test]
async fn it_adds_with_middleware_for_route() {
    let server = TestServer::spawn(|app| {
        app.map_get("/test", async || "Pass!")
            .wrap(async |ctx: HttpContext, next: NextFn| next(ctx).await)
            .with(|next| next);
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
async fn it_adds_shortcut_with_middleware_for_route() {
    let server = TestServer::spawn(|app| {
        app.map_get("/test", async || "Pass!")
            .wrap(|ctx: HttpContext, next: NextFn| async move { next(ctx).await })
            .with(|_| async move { volga::bad_request!("Error!") })
            .with(|next| next);
    })
    .await;

    let response = server
        .client()
        .get(server.url("/test"))
        .send()
        .await
        .unwrap();

    assert!(!response.status().is_success());
    assert_eq!(response.text().await.unwrap(), "Error!");

    server.shutdown().await;
}

#[tokio::test]
async fn it_adds_with_middleware_for_group() {
    let server = TestServer::spawn(|app| {
        app.group("/tests", |api| {
            api.wrap(async |ctx: HttpContext, next: NextFn| next(ctx).await)
                .with(|next| next);

            api.map_get("/test", || async { "Pass!" });
        });
    })
    .await;

    let response = server
        .client()
        .get(server.url("/tests//test"))
        .send()
        .await
        .unwrap();

    assert!(response.status().is_success());
    assert_eq!(response.text().await.unwrap(), "Pass!");

    server.shutdown().await;
}

#[tokio::test]
async fn it_adds_shortcut_with_middleware_for_group() {
    let server = TestServer::spawn(|app| {
        app.group("/tests", |api| {
            api.wrap(|ctx: HttpContext, next: NextFn| async move { next(ctx).await })
                .with(|_| async move { volga::bad_request!("Error!") })
                .with(|next| next);

            api.map_get("/test", || async { "Pass!" });
        })
    })
    .await;

    let response = server
        .client()
        .get(server.url("/tests//test"))
        .send()
        .await
        .unwrap();

    assert!(!response.status().is_success());
    assert_eq!(response.text().await.unwrap(), "Error!");

    server.shutdown().await;
}
#[tokio::test]
async fn it_routes_nested_group() {
    let server = TestServer::spawn(|app| {
        app.group("/api", |api| {
            api.map_get("/info", async || "api");

            api.group("/users", |users| {
                users.map_get("/{id}", |id: i32| async move { id.to_string() });
            });
        });
    })
    .await;

    let info = server
        .client()
        .get(server.url("/api/info"))
        .send()
        .await
        .unwrap();

    assert!(info.status().is_success());
    assert_eq!(info.text().await.unwrap(), "api");

    let user = server
        .client()
        .get(server.url("/api/users/42"))
        .send()
        .await
        .unwrap();

    assert!(user.status().is_success());
    assert_eq!(user.text().await.unwrap(), "42");

    server.shutdown().await;
}

#[tokio::test]
async fn it_inherits_parent_middleware_in_nested_group() {
    let server = TestServer::spawn(|app| {
        app.group("/api", |api| {
            api.map_ok(|mut resp: HttpResponse| async move {
                resp.try_insert_header::<XTest>("from-parent").unwrap();
                resp
            });

            api.group("/users", |users| {
                users.map_get("/list", async || "users");
            });
        });
    })
    .await;

    let response = server
        .client()
        .get(server.url("/api/users/list"))
        .send()
        .await
        .unwrap();

    assert!(response.status().is_success());
    assert_eq!(response.headers().get("X-Test").unwrap(), "from-parent");
    assert_eq!(response.text().await.unwrap(), "users");

    server.shutdown().await;
}

#[tokio::test]
async fn it_applies_child_middleware_only_to_nested_group() {
    let server = TestServer::spawn(|app| {
        app.group("/api", |api| {
            api.map_get("/info", async || "api");

            api.group("/users", |users| {
                users.map_ok(|mut resp: HttpResponse| async move {
                    resp.try_insert_header::<XTest>("from-child").unwrap();
                    resp
                });
                users.map_get("/list", async || "users");
            });
        });
    })
    .await;

    let nested = server
        .client()
        .get(server.url("/api/users/list"))
        .send()
        .await
        .unwrap();

    assert!(nested.status().is_success());
    assert_eq!(nested.headers().get("X-Test").unwrap(), "from-child");

    let parent = server
        .client()
        .get(server.url("/api/info"))
        .send()
        .await
        .unwrap();

    assert!(parent.status().is_success());
    assert!(parent.headers().get("X-Test").is_none());

    server.shutdown().await;
}

#[tokio::test]
async fn it_applies_middleware_top_to_bottom_in_nested_group() {
    let server = TestServer::spawn(|app| {
        app.group("/api", |api| {
            api.tap_req(|mut req: HttpRequestMut| async move {
                req.try_insert_header::<XTest>("parent").unwrap();
                req
            });

            api.group("/inner", |inner| {
                inner.map_get("/test", |headers: HttpHeaders| async move {
                    headers.try_get::<XTest>().map(|v| v.to_string())
                });
            });
        });
    })
    .await;

    let response = server
        .client()
        .get(server.url("/api/inner/test"))
        .send()
        .await
        .unwrap();

    assert!(response.status().is_success());
    assert_eq!(response.text().await.unwrap(), "x-test: parent");

    server.shutdown().await;
}
