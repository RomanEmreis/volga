#![allow(missing_docs)]
#![allow(unused)]
#![cfg(all(feature = "test", feature = "middleware"))]

use hyper::StatusCode;
use std::io::{Error as IoError, ErrorKind};
use volga::error::Error;
use volga::headers::{Header, HttpHeaders, headers};
use volga::http::FilterResult;
use volga::middleware::{HttpContext, NextFn};
use volga::test::TestServer;
use volga::validation::ValidationError;
use volga::{HttpRequestMut, HttpResponse, Json, ok, status};

headers! {
    (XTest, "x-test")
}

/// Status, `Content-Type` and body of a `GET`
async fn get(server: &TestServer, path: &str) -> (u16, String, String) {
    let res = server.client().get(server.url(path)).send().await.unwrap();
    let status = res.status().as_u16();
    let content_type = res
        .headers()
        .get("content-type")
        .map(|v| v.to_str().unwrap().to_owned())
        .unwrap_or_default();
    (status, content_type, res.text().await.unwrap())
}

/// A `403` answering with a JSON body of its own
fn denied() -> Error {
    Error::from_parts(StatusCode::FORBIDDEN, None, "nope").with_response(Json("denied"))
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
async fn it_answers_a_filter_err_with_the_status_of_its_error() {
    let server = TestServer::spawn(|app| {
        app.map_get("/volga", || "ok")
            .filter(|| Err::<(), _>(Error::from_parts(StatusCode::UNAUTHORIZED, None, "no key")));
        app.map_get("/with-error", || "ok").filter(|| {
            FilterResult::err().with_error(Error::from_parts(
                StatusCode::FORBIDDEN,
                None,
                "forbidden",
            ))
        });
        app.map_get("/io", || "ok")
            .filter(|| Err::<(), _>(IoError::new(ErrorKind::NotFound, "gone")));
        app.map_get("/status", || "ok")
            .filter(|| Err::<(), _>(StatusCode::UNAUTHORIZED));
        app.map_get("/tuple", || "ok")
            .filter(|| Err::<(), _>((StatusCode::CONFLICT, "taken")));
        app.map_get("/validation", || "ok").filter(|| {
            Err::<(), _>(
                ValidationError::message("bad").with_status(StatusCode::UNPROCESSABLE_ENTITY),
            )
        });
        app.map_get("/string", || "ok")
            .filter(|| Err::<(), _>("nope"));
        app.map_get("/string-with-error", || "ok")
            .filter(|| FilterResult::err().with_error(String::from("nope")));
        app.map_get("/boxed", || "ok")
            .filter(|| Err::<(), Box<dyn std::error::Error + Send + Sync>>("boxed".into()));
        app.map_get("/parse", || "ok").filter(|| {
            "x".parse::<i32>()
                .map(|_| ())
                .map_err(|err| (StatusCode::BAD_REQUEST, err))
        });
        app.map_get("/false", || "ok").filter(|| false);
        app.group("/group", |api| {
            api.filter(|| {
                Err::<(), _>(Error::from_parts(StatusCode::UNAUTHORIZED, None, "no key"))
            });
            api.map_get("/test", || "ok");
        });
    })
    .await;

    let cases = [
        ("/volga", 401, "no key"),
        ("/with-error", 403, "forbidden"),
        ("/io", 404, "gone"),
        ("/status", 401, "Unauthorized"),
        ("/tuple", 409, "taken"),
        ("/validation", 422, "bad"),
        ("/string", 400, "nope"),
        ("/string-with-error", 400, "nope"),
        ("/boxed", 400, "boxed"),
        ("/parse", 400, "invalid digit found in string"),
        (
            "/false",
            400,
            "Validation: One or more request parameters are incorrect",
        ),
        ("/group/test", 401, "no key"),
    ];

    for (path, status, body) in cases {
        let (actual_status, _, actual_body) = get(&server, path).await;
        assert_eq!(
            (actual_status, actual_body.as_str()),
            (status, body),
            "{path}"
        );
    }

    server.shutdown().await;
}

#[tokio::test]
async fn it_hands_a_filter_err_to_map_err_as_it_is() {
    let server = TestServer::spawn(|app| {
        app.map_err(|err: Error| async move {
            let (status, instance, inner) = err.into_parts();
            let nested = inner.downcast_ref::<Error>().is_some();
            status!(
                status.as_u16(),
                "instance={} nested={nested}",
                instance.unwrap_or_default()
            )
        });
        app.map_get("/test", || "ok").filter(|| {
            Err::<(), _>(Error::from_parts(
                StatusCode::FORBIDDEN,
                Some("/custom".into()),
                "nope",
            ))
        });
    })
    .await;

    let (status, _, body) = get(&server, "/test").await;

    assert_eq!(status, 403);
    assert_eq!(body, "instance=/custom nested=false");

    server.shutdown().await;
}

#[tokio::test]
async fn it_answers_a_filter_err_with_the_response_its_error_carries() {
    let server = TestServer::spawn(|app| {
        app.map_get("/route", || "ok")
            .filter(|| Err::<(), _>(denied()));
        app.group("/group", |api| {
            api.filter(|| FilterResult::err().with_error(denied()));
            api.map_get("/test", || "ok");
        });
    })
    .await;

    for path in ["/route", "/group/test"] {
        assert_eq!(
            get(&server, path).await,
            (403, "application/json".into(), r#""denied""#.into()),
            "{path}"
        );
    }

    server.shutdown().await;
}

// A `Problem` is as large as it is, and an `Err` of one is what the `/problem` route is about
#[cfg(feature = "problem-details")]
#[allow(clippy::result_large_err)]
#[tokio::test]
async fn it_answers_a_filter_err_under_problem_details() {
    let server = TestServer::spawn(|app| {
        app.use_problem_details();
        app.map_get("/status", || "ok")
            .filter(|| Err::<(), _>(Error::from_parts(StatusCode::UNAUTHORIZED, None, "no key")));
        app.map_get("/carried", || "ok")
            .filter(|| Err::<(), _>(denied()));
        app.map_get("/problem", || "ok").filter(|| {
            Err::<(), _>(volga::error::ProblemDetails::new(422).with_detail("field is invalid"))
        });
    })
    .await;

    let (status, content_type, body) = get(&server, "/status").await;
    let body: serde_json::Value = serde_json::from_str(&body).unwrap();

    assert_eq!(status, 401);
    assert_eq!(content_type, "application/problem+json");
    assert_eq!(body["status"], 401);
    assert_eq!(body["detail"], "no key");

    assert_eq!(
        get(&server, "/carried").await,
        (403, "application/json".into(), r#""denied""#.into())
    );

    let (status, content_type, body) = get(&server, "/problem").await;
    let body: serde_json::Value = serde_json::from_str(&body).unwrap();

    assert_eq!(status, 422);
    assert_eq!(content_type, "application/problem+json");
    assert_eq!(body["detail"], "field is invalid");

    server.shutdown().await;
}

#[cfg(feature = "oauth")]
#[tokio::test]
async fn it_answers_a_filter_oauth_error_with_the_status_of_its_code() {
    use volga::auth::oauth::{OAuthError, OAuthErrorCode};

    let server = TestServer::spawn(|app| {
        app.map_get("/invalid-token", || "ok")
            .filter(|| Err::<(), _>(OAuthError::new(OAuthErrorCode::InvalidToken)));
        app.group("/scoped", |api| {
            api.filter(|| Err::<(), _>(OAuthError::new(OAuthErrorCode::InsufficientScope)));
            api.map_get("/test", || "ok");
        });
    })
    .await;

    let cases = [
        ("/invalid-token", 401, "invalid_token"),
        ("/scoped/test", 403, "insufficient_scope"),
    ];

    for (path, status, body) in cases {
        let (actual_status, _, actual_body) = get(&server, path).await;
        assert_eq!(
            (actual_status, actual_body.as_str()),
            (status, body),
            "{path}"
        );
    }

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
