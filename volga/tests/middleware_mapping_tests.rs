#![allow(missing_docs)]
#![cfg(all(feature = "test", feature = "middleware"))]

use hyper::StatusCode;
use volga::{HttpRequest, HttpResponse, Results};
use volga::error::Error;
use volga::headers::{HttpHeaders, custom_headers, Header};
use volga::test::TestServer;

custom_headers! {
    (XTest, "x-test")
}

#[tokio::test]
async fn it_adds_middleware_request() {
    let server = TestServer::spawn(|app| {
        app.wrap(|context, next| async move {
            next(context).await
        });
        app.wrap(|_, _| async move {
            Results::text("Pass!")
        });
        app.map_get("/test", || async {
            Results::text("Unreachable!")
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
async fn it_adds_map_ok_middleware() {
    let server = TestServer::spawn(|app| {
        app.map_ok(|mut resp: HttpResponse| async move {
            resp.headers_mut().insert("X-Test", "Test".parse().unwrap());
            resp
        });
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
    assert_eq!(response.headers().get("X-Test").unwrap(), "Test");
    assert_eq!(response.text().await.unwrap(), "Pass!");
    
    server.shutdown().await;
}

#[tokio::test]
async fn it_adds_map_req_middleware() {
    let server = TestServer::spawn(|app| {
        app.tap_req(|mut req: HttpRequest| async move {
            req.insert_header(Header::<XTest>::try_from("Pass!").unwrap());
            req
        });
        app.map_get("/test", |headers: HttpHeaders| async move {
            let val = headers.get("X-Test").unwrap().to_str().unwrap();
            Results::text(val)
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
async fn it_adds_map_ok_middleware_for_route() {
    let server = TestServer::spawn(|app| {
        app
            .map_get("/test", async || "Pass!")
            .map_ok(|mut resp: HttpResponse| async move {
                resp
                    .headers_mut()
                    .insert("X-Test", "Test".parse().unwrap());
                resp
            });
    }).await;

    let response = server.client()
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
        app
            .map_get("/test", |headers: HttpHeaders| async move {
                let val = headers.get("X-Test").unwrap().to_str().unwrap();
                Results::text(val)
            })
            .tap_req(|mut req: HttpRequest| async move {
                req.insert_header(Header::<XTest>::try_from("Pass!").unwrap());
                req
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
async fn it_adds_map_ok_middleware_for_group() {
    let server = TestServer::spawn(|app| {
        app.group("/tests", |api| {
            api.map_ok(|mut resp: HttpResponse| async move {
                    resp.headers_mut().insert("X-Test", "Test".parse().unwrap());
                    resp
                });
            api.map_get("/test", async || "Pass!");
        });
    }).await;

    let response = server.client()
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
            api.tap_req(|mut req: HttpRequest| async move {
                req.try_insert_header::<XTest>("Pass!").unwrap();
                req
            });
            api.map_get("/test", |headers: HttpHeaders| async move {
                let val = headers.get("X-Test").unwrap().to_str().unwrap();
                Results::text(val)
            });
        });
    }).await;

    let response = server.client()
        .get(server.url("/tests/test"))
        .send()
        .await
        .unwrap();

    assert!(response.status().is_success());
    assert_eq!(response.text().await.unwrap(), "Pass!");
    
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
    }).await;

    let response = server.client()
        .get(server.url("/test"))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(response.text().await.unwrap(), "\"Some Error occurred!\"");
    
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
    }).await;

    let response = server.client()
        .get(server.url("/tests/test"))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(response.text().await.unwrap(), "\"Some Error occurred!\"");
    
    server.shutdown().await;
}

#[tokio::test]
async fn it_adds_invalid_filter_middleware_for_route() {
    let server = TestServer::spawn(|app| {
        app
            .map_get("/test", async || ())
            .filter(async || false);
    }).await;

    let response = server.client()
        .get(server.url("/test"))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        response.text().await.unwrap(), 
        "\"Validation: One or more request parameters are incorrect\""
    );
    
    server.shutdown().await;
}

#[tokio::test]
async fn it_adds_valid_filter_middleware_for_route() {
    let server = TestServer::spawn(|app| {
        app
            .map_get("/test", async || "Pass!")
            .filter(async || true);
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
async fn it_adds_invalid_filter_middleware_for_group() {
    let server = TestServer::spawn(|app| {
        app.group("/tests", |api| {
            api.filter(async || false);
            api.map_get("/test", async || ());
        });
    }).await;

    let response = server.client()
        .get(server.url("/tests/test"))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        response.text().await.unwrap(), 
        "\"Validation: One or more request parameters are incorrect\""
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
    }).await;

    let response = server.client()
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
        app.wrap(|ctx, next| async move { next(ctx).await })
            .with(|next| next)
            .map_get("/test", || async { "Pass!" });
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
async fn it_adds_shortcut_with_middleware() {
    let server = TestServer::spawn(|app| {
        app
            .wrap(async |ctx, next| next(ctx).await)
            .with(async |_| volga::bad_request!("Error!"))
            .with(|next| next)
            .map_get("/test", async || "Pass!");
    }).await;

    let response = server.client()
        .get(server.url("/test"))
        .send()
        .await
        .unwrap();

    assert!(!response.status().is_success());
    assert_eq!(response.text().await.unwrap(), "\"Error!\"");
    
    server.shutdown().await;
}

#[tokio::test]
async fn it_adds_with_middleware_for_route() {
    let server = TestServer::spawn(|app| {
        app
            .map_get("/test", async || "Pass!")
            .wrap(async |ctx, next| next(ctx).await)
            .with(|next| next);

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
async fn it_adds_shortcut_with_middleware_for_route() {
    let server = TestServer::spawn(|app| {
        app
            .map_get("/test", async || "Pass!")
            .wrap(|ctx, next| async move { next(ctx).await })
            .with(|_| async move { volga::bad_request!("Error!") })
            .with(|next| next);
    }).await;

    let response = server.client()
        .get(server.url("/test"))
        .send()
        .await
        .unwrap();

    assert!(!response.status().is_success());
    assert_eq!(response.text().await.unwrap(), "\"Error!\"");
    
    server.shutdown().await;
}

#[tokio::test]
async fn it_adds_with_middleware_for_group() {
    let server = TestServer::spawn(|app| {
        app.group("/tests", |api| {
            api
                .wrap(async |ctx, next| next(ctx).await)
                .with(|next| next);

            api.map_get("/test", || async { "Pass!" });
        });
    }).await;

    let response = server.client()
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
            api
                .wrap(|ctx, next| async move { next(ctx).await })
                .with(|_| async move { volga::bad_request!("Error!") })
                .with(|next| next);
            
            api.map_get("/test", || async { "Pass!" });
        })
    }).await;

    let response = server.client()
        .get(server.url("/tests//test"))
        .send()
        .await
        .unwrap();

    assert!(!response.status().is_success());
    assert_eq!(response.text().await.unwrap(), "\"Error!\"");
    
    server.shutdown().await;
}