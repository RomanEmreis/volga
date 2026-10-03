#![allow(missing_docs)]
#![cfg(feature = "test")]

//! The request body limit (#273): the application's, a route group's and a route's, the
//! most specific one winning, and `413 Content Too Large` for a body over it - before any of
//! it is read when its `Content-Length` already says it won't fit.

use futures_util::stream;
use volga::test::TestServer;
use volga::{HttpRequest, Json, Limit, ok};

/// A handler that reads the whole body and answers with how much of it there was
async fn read_body(req: HttpRequest) -> volga::HttpResult {
    use http_body_util::BodyExt;

    let bytes = req.into_body().collect().await?.to_bytes();
    ok!(bytes.len())
}

/// A handler that answers with the body limit the request was given
async fn report_limit(req: HttpRequest) -> volga::HttpResult {
    ok!(req.body_limit())
}

/// The status and the body `path` answers a `POST` of `len` bytes with
async fn post(server: &TestServer, path: &str, len: usize) -> (u16, String) {
    let response = server
        .client()
        .post(server.url(path))
        .body(vec![b'x'; len])
        .send()
        .await
        .unwrap();

    (response.status().as_u16(), response.text().await.unwrap())
}

/// The body limit `path` reports
async fn limit_at(server: &TestServer, path: &str) -> String {
    server
        .client()
        .get(server.url(path))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap()
}

#[tokio::test]
async fn it_answers_413_for_a_body_over_the_application_limit() {
    let server = TestServer::builder()
        .configure(|app| app.with_body_limit(Limit::Limited(16)))
        .setup(|app| {
            app.map_post("/upload", read_body);
        })
        .build()
        .await;

    assert_eq!(post(&server, "/upload", 16).await, (200, "16".into()));
    assert_eq!(
        post(&server, "/upload", 17).await,
        (413, "length limit exceeded".into())
    );

    server.shutdown().await;
}

#[tokio::test]
async fn it_answers_413_to_a_json_body_over_the_limit() {
    let server = TestServer::builder()
        .configure(|app| app.with_body_limit(Limit::Limited(16)))
        .setup(|app| {
            app.map_post("/json", |Json(value): Json<serde_json::Value>| async move {
                ok!(value)
            });
        })
        .build()
        .await;

    let response = server
        .client()
        .post(server.url("/json"))
        .json(&serde_json::json!({ "name": "a name too long to fit" }))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 413);

    server.shutdown().await;
}

#[tokio::test]
async fn it_answers_413_once_a_body_of_undeclared_length_goes_over_the_limit() {
    let server = TestServer::builder()
        .configure(|app| app.with_body_limit(Limit::Limited(16)))
        .setup(|app| {
            app.map_post("/upload", read_body);
        })
        .build()
        .await;

    let chunks = stream::iter(
        (0..4).map(|_| Ok::<_, std::io::Error>(bytes::Bytes::from_static(b"0123456789"))),
    );
    let response = server
        .client()
        .post(server.url("/upload"))
        .body(reqwest::Body::wrap_stream(chunks))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 413);

    server.shutdown().await;
}

#[tokio::test]
async fn it_lets_a_group_raise_and_lower_the_application_limit() {
    let server = TestServer::builder()
        .configure(|app| app.with_body_limit(Limit::Limited(16)))
        .setup(|app| {
            app.map_post("/small", read_body);
            app.group("/big", |big| {
                big.with_body_limit(Limit::Limited(1024));
                big.map_post("/upload", read_body);
            });
            app.group("/tiny", |tiny| {
                tiny.map_post("/upload", read_body);
                // Declared after the route, and it applies to it all the same
                tiny.with_body_limit(Limit::Limited(4));
            });
        })
        .build()
        .await;

    assert_eq!(post(&server, "/small", 100).await.0, 413);
    assert_eq!(post(&server, "/big/upload", 100).await, (200, "100".into()));
    assert_eq!(post(&server, "/big/upload", 1025).await.0, 413);
    assert_eq!(post(&server, "/tiny/upload", 4).await, (200, "4".into()));
    assert_eq!(post(&server, "/tiny/upload", 5).await.0, 413);

    server.shutdown().await;
}

#[tokio::test]
async fn it_lets_a_route_override_its_group() {
    let server = TestServer::builder()
        .setup(|app| {
            app.group("/api", |api| {
                api.with_body_limit(Limit::Limited(16));
                api.map_post("/chat", read_body);
                api.map_post("/attachments", read_body)
                    .with_body_limit(Limit::Limited(1024));
            });
        })
        .build()
        .await;

    assert_eq!(post(&server, "/api/chat", 100).await.0, 413);
    assert_eq!(
        post(&server, "/api/attachments", 100).await,
        (200, "100".into())
    );
    assert_eq!(post(&server, "/api/attachments", 1025).await.0, 413);

    server.shutdown().await;
}

#[tokio::test]
async fn it_lets_a_route_raise_the_application_limit() {
    let server = TestServer::builder()
        .configure(|app| app.with_body_limit(Limit::Limited(16)))
        .setup(|app| {
            app.map_post("/upload", read_body)
                .with_body_limit(Limit::Limited(1024));
        })
        .build()
        .await;

    assert_eq!(post(&server, "/upload", 100).await, (200, "100".into()));

    server.shutdown().await;
}

#[tokio::test]
async fn it_disables_the_limit_for_a_route_or_a_group() {
    let server = TestServer::builder()
        .configure(|app| app.with_body_limit(Limit::Limited(16)))
        .setup(|app| {
            app.map_post("/stream", read_body).without_body_limit();
            app.group("/unlimited", |group| {
                group.without_body_limit();
                group.map_post("/upload", read_body);
            });
            app.group("/limited", |group| {
                group.with_body_limit(Limit::Limited(4));
                group
                    .map_post("/unlimited", read_body)
                    .with_body_limit(Limit::Unlimited);
            });
        })
        .build()
        .await;

    assert_eq!(post(&server, "/stream", 1000).await, (200, "1000".into()));
    assert_eq!(
        post(&server, "/unlimited/upload", 1000).await,
        (200, "1000".into())
    );
    assert_eq!(
        post(&server, "/limited/unlimited", 1000).await,
        (200, "1000".into())
    );

    server.shutdown().await;
}

#[tokio::test]
async fn it_picks_the_most_specific_limit() {
    let server = TestServer::builder()
        .configure(|app| app.with_body_limit(Limit::Limited(1)))
        .setup(|app| {
            app.map_get("/app", report_limit);
            app.group("/outer", |outer| {
                outer.with_body_limit(Limit::Limited(2));
                outer.map_get("/route", report_limit);
                outer
                    .map_get("/own", report_limit)
                    .with_body_limit(Limit::Limited(5));

                // A nested group declared before the outer one set its limit, and one
                // declared after it: both inherit it, and both keep a limit of their own
                outer.group("/inherits", |inner| {
                    inner.map_get("/route", report_limit);
                });
                outer.group("/inner", |inner| {
                    inner.with_body_limit(Limit::Limited(3));
                    inner.map_get("/route", report_limit);
                    inner
                        .map_get("/own", report_limit)
                        .with_body_limit(Limit::Limited(4));
                });
            });
        })
        .build()
        .await;

    assert_eq!(limit_at(&server, "/app").await, "1");
    assert_eq!(limit_at(&server, "/outer/route").await, "2");
    assert_eq!(limit_at(&server, "/outer/own").await, "5");
    assert_eq!(limit_at(&server, "/outer/inherits/route").await, "2");
    assert_eq!(limit_at(&server, "/outer/inner/route").await, "3");
    assert_eq!(limit_at(&server, "/outer/inner/own").await, "4");

    server.shutdown().await;
}

#[tokio::test]
async fn it_reads_limit_default_as_the_framework_default() {
    let server = TestServer::builder()
        .configure(|app| app.with_body_limit(Limit::Limited(16)))
        .setup(|app| {
            app.group("/api", |api| {
                api.with_body_limit(Limit::Default);
                api.map_get("/route", report_limit);
            });
        })
        .build()
        .await;

    assert_eq!(
        limit_at(&server, "/api/route").await,
        (5 * 1024 * 1024).to_string()
    );

    server.shutdown().await;
}

/// A group's fallback answers under the group's prefix on the group's behalf, so the
/// middleware around it reads the group's body limit, and a request no route or group
/// claims reads the application's
#[cfg(feature = "middleware")]
#[tokio::test]
async fn it_gives_a_group_fallback_the_group_limit() {
    use volga::middleware::{HttpContext, NextFn};

    let server = TestServer::builder()
        .configure(|app| app.with_body_limit(Limit::Limited(1)))
        .setup(|app| {
            app.group("/api", |api| {
                api.with_body_limit(Limit::Limited(2));
                api.wrap(|ctx: HttpContext, _next: NextFn| async move {
                    ok!(ctx.request().body_limit())
                });
                api.map_fallback(|| async { "unreachable" });
            });
            app.wrap(|ctx: HttpContext, next: NextFn| async move {
                if ctx.matched_route() || ctx.request().uri().path().starts_with("/api") {
                    return next(ctx).await;
                }
                ok!(ctx.request().body_limit())
            });
        })
        .build()
        .await;

    assert_eq!(limit_at(&server, "/api/nope").await, "2");
    assert_eq!(limit_at(&server, "/nope").await, "1");

    server.shutdown().await;
}

/// A `Content-Length` over the limit is refused before any of the body is read: the client
/// that waits for `100 Continue` is answered `413` instead, and never sends the body
#[cfg(feature = "http1")]
#[tokio::test]
async fn it_refuses_a_declared_length_over_the_limit_before_reading_the_body() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;
    use tokio::time::{Duration, timeout};

    let server = TestServer::builder()
        .configure(|app| app.with_body_limit(Limit::Limited(16)))
        .setup(|app| {
            app.map_post("/upload", read_body);
        })
        .build()
        .await;

    let addr = server.url("").trim_start_matches("http://").to_owned();
    let mut stream = TcpStream::connect(addr).await.unwrap();
    stream
        .write_all(
            b"POST /upload HTTP/1.1\r\n\
              Host: localhost\r\n\
              Content-Length: 1048576\r\n\
              Expect: 100-continue\r\n\
              \r\n",
        )
        .await
        .unwrap();

    let mut response = vec![0; 1024];
    let read = timeout(Duration::from_secs(5), stream.read(&mut response))
        .await
        .expect("the server waited for the body instead of refusing it")
        .unwrap();
    let response = String::from_utf8_lossy(&response[..read]);

    assert!(
        response.starts_with("HTTP/1.1 413"),
        "unexpected response: {response}"
    );

    server.shutdown().await;
}
