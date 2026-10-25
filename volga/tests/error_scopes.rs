#![allow(missing_docs)]
#![cfg(all(feature = "test", feature = "middleware"))]

//! Error handling scopes (#292): `map_err` covers the whole route or group it is set on,
//! whatever order it was registered in, a scope has one, and an error it passes on reaches
//! the scope around it. The rejections volga produces itself travel the pipeline as errors,
//! and the headers an error carries are added to whatever answers it.

use volga::{
    HttpResponse, HttpResult,
    error::Error,
    headers::WWW_AUTHENTICATE,
    http::{Method, StatusCode},
    ok, status,
    test::TestServer,
};

/// Answers an error with its own status and `$name` as the body, so a test reads which
/// handler answered it
macro_rules! named {
    ($name:literal) => {
        |err: Error| status!(err.status().as_u16(), $name)
    };
}

async fn send(server: &TestServer, method: Method, path: &str) -> reqwest::Response {
    server
        .client()
        .request(method, server.url(path))
        .send()
        .await
        .unwrap()
}

async fn get(server: &TestServer, path: &str) -> (u16, String) {
    let response = send(server, Method::GET, path).await;
    (response.status().as_u16(), response.text().await.unwrap())
}

async fn tag(mut response: HttpResponse) -> HttpResult {
    response.try_insert_raw_header("x-map-ok", "1")?;
    Ok(response)
}

#[tokio::test]
async fn it_covers_the_whole_route_whatever_the_order() {
    let server = TestServer::spawn(|app| {
        app.map_err(named!("app"));

        app.map_get("/before", || "unreachable")
            .filter(|| false)
            .map_err(named!("route"));
        app.map_get("/after", || "unreachable")
            .map_err(named!("route"))
            .filter(|| false);
    })
    .await;

    assert_eq!(get(&server, "/before").await, (400, "route".into()));
    assert_eq!(get(&server, "/after").await, (400, "route".into()));

    server.shutdown().await;
}

#[tokio::test]
async fn it_keeps_one_error_handler_per_route() {
    let server = TestServer::spawn(|app| {
        app.map_get("/twice", || Err::<(), _>(StatusCode::CONFLICT))
            .map_err(named!("first"))
            .map_err(named!("second"));
    })
    .await;

    assert_eq!(get(&server, "/twice").await, (409, "second".into()));

    server.shutdown().await;
}

#[tokio::test]
async fn it_covers_the_whole_group_whatever_the_order() {
    let server = TestServer::spawn(|app| {
        app.map_err(named!("app"));

        app.group("/before", |g| {
            g.map_err(named!("group"));
            g.filter(|| false);
            g.map_get("/x", || "unreachable");
        });
        app.group("/after", |g| {
            g.filter(|| false);
            g.map_err(named!("group"));
            g.map_get("/x", || "unreachable");
        });
        app.group("/twice", |g| {
            g.map_err(named!("first"));
            g.map_err(named!("second"));
            g.map_get("/x", || Err::<(), _>(StatusCode::CONFLICT));
        });
    })
    .await;

    assert_eq!(get(&server, "/before/x").await, (400, "group".into()));
    assert_eq!(get(&server, "/after/x").await, (400, "group".into()));
    assert_eq!(get(&server, "/twice/x").await, (409, "second".into()));

    server.shutdown().await;
}

#[tokio::test]
async fn it_passes_an_error_on_to_the_enclosing_scope() {
    let server = TestServer::spawn(|app| {
        app.map_err(named!("app"));

        app.map_get("/route", || Err::<(), _>(StatusCode::CONFLICT))
            .map_err(|err: Error| err);

        app.group("/outer", |outer| {
            outer.map_err(named!("outer"));

            outer.group("/inner", |inner| {
                inner.map_err(|err: Error| err);

                inner
                    .map_get("/answered", || Err::<(), _>(StatusCode::CONFLICT))
                    .map_err(named!("route"));
                inner
                    .map_get("/passed", || Err::<(), _>(StatusCode::CONFLICT))
                    .map_err(|err: Error| err);
            });
        });

        app.group("/up", |g| {
            g.map_err(|err: Error| err);
            g.map_get("/x", || Err::<(), _>(StatusCode::CONFLICT));
        });
    })
    .await;

    assert_eq!(get(&server, "/route").await, (409, "app".into()));
    assert_eq!(
        get(&server, "/outer/inner/answered").await,
        (409, "route".into())
    );
    assert_eq!(
        get(&server, "/outer/inner/passed").await,
        (409, "outer".into())
    );
    assert_eq!(get(&server, "/up/x").await, (409, "app".into()));

    server.shutdown().await;
}

#[tokio::test]
async fn it_leaves_the_errors_of_an_enclosing_scope_to_it() {
    let server = TestServer::spawn(|app| {
        app.map_err(named!("app"));
        app.filter(|headers: volga::headers::HttpHeaders| headers.get_raw("x-global").is_none());

        app.group("/g", |g| {
            g.map_err(named!("group"));
            g.group("/inner", |inner| {
                inner.filter(|headers: volga::headers::HttpHeaders| {
                    headers.get_raw("x-inner").is_none()
                });
                inner.map_get("/x", || "x").map_err(named!("route"));
            });
        });
    })
    .await;

    let response = server
        .client()
        .get(server.url("/g/inner/x"))
        .header("x-inner", "1")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 400);
    assert_eq!(response.text().await.unwrap(), "group");

    let response = server
        .client()
        .get(server.url("/g/inner/x"))
        .header("x-global", "1")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 400);
    assert_eq!(response.text().await.unwrap(), "app");

    server.shutdown().await;
}

#[tokio::test]
async fn it_adds_the_error_headers_to_whatever_answers_the_error() {
    let server = TestServer::spawn(|app| {
        app.map_err(|err: Error| status!(500, "app: {}", err.status().as_u16()));

        app.map_get("/challenge", || {
            Err::<(), _>(
                Error::from(StatusCode::UNAUTHORIZED)
                    .with_header((WWW_AUTHENTICATE, r#"Basic realm="x""#)),
            )
        });
        app.map_get("/twice", || {
            Err::<(), _>(
                Error::from(StatusCode::CONFLICT)
                    .with_header(("x-reason", "one"))
                    .with_header(("x-reason", "two")),
            )
        });
        app.map_get("/own", || {
            Err::<(), _>(
                Error::from(StatusCode::UNAUTHORIZED)
                    .with_header((WWW_AUTHENTICATE, r#"Basic realm="x""#)),
            )
        })
        .map_err(|_: Error| status!(401; [(WWW_AUTHENTICATE, "Bearer")]));
        app.map_get("/passed", || {
            Err::<(), _>(Error::from(StatusCode::CONFLICT).with_header(("x-reason", "one")))
        })
        .map_err(|err: Error| err);
    })
    .await;

    // A handler of its own that knows nothing of the challenge keeps it all the same
    let response = send(&server, Method::GET, "/challenge").await;
    assert_eq!(response.status(), 500);
    assert_eq!(response.headers()[WWW_AUTHENTICATE], r#"Basic realm="x""#);
    assert_eq!(response.text().await.unwrap(), "app: 401");

    let response = send(&server, Method::GET, "/twice").await;
    let values: Vec<_> = response.headers().get_all("x-reason").iter().collect();
    assert_eq!(values, ["one", "two"]);

    // A header the handler sets itself wins
    let response = send(&server, Method::GET, "/own").await;
    let values: Vec<_> = response
        .headers()
        .get_all(WWW_AUTHENTICATE)
        .iter()
        .collect();
    assert_eq!(values, ["Bearer"]);

    // Passed on, the error is answered once, and so are its headers
    let response = send(&server, Method::GET, "/passed").await;
    let values: Vec<_> = response.headers().get_all("x-reason").iter().collect();
    assert_eq!(values, ["one"]);

    server.shutdown().await;
}

#[tokio::test]
async fn it_adds_the_error_headers_to_the_default_and_the_attached_responses() {
    let server = TestServer::spawn(|app| {
        app.map_get("/default", || {
            Err::<(), _>(Error::from(StatusCode::CONFLICT).with_header(("x-reason", "default")))
        });
        app.map_get("/attached", || {
            Err::<(), _>(
                Error::from(StatusCode::CONFLICT)
                    .with_response("attached")
                    .with_header(("x-reason", "attached")),
            )
        });
    })
    .await;

    let response = send(&server, Method::GET, "/default").await;
    assert_eq!(response.status(), 409);
    assert_eq!(response.headers()["x-reason"], "default");
    assert_eq!(response.text().await.unwrap(), "Conflict");

    let response = send(&server, Method::GET, "/attached").await;
    assert_eq!(response.status(), 409);
    assert_eq!(response.headers()["x-reason"], "attached");
    assert_eq!(response.text().await.unwrap(), "attached");

    server.shutdown().await;
}

#[tokio::test]
async fn it_refuses_an_unmatched_path_and_method_with_an_error() {
    let server = TestServer::spawn(|app| {
        app.map_ok(tag);
        app.map_err(
            |err: Error| status!(err.status().as_u16(), { "status": err.status().as_u16() }),
        );
        app.map_get("/mapped", || async { ok!(text: "mapped") });
    })
    .await;

    let response = send(&server, Method::GET, "/nope").await;
    assert_eq!(response.status(), 404);
    assert!(response.headers().get("x-map-ok").is_none());
    assert_eq!(response.text().await.unwrap(), r#"{"status":404}"#);

    let response = send(&server, Method::DELETE, "/mapped").await;
    assert_eq!(response.status(), 405);
    assert_eq!(response.headers()["allow"], "GET,HEAD");
    assert!(response.headers().get("x-map-ok").is_none());
    assert_eq!(response.text().await.unwrap(), r#"{"status":405}"#);

    let response = send(&server, Method::GET, "/mapped").await;
    assert_eq!(response.headers()["x-map-ok"], "1");

    server.shutdown().await;
}

#[tokio::test]
async fn it_answers_a_fallback_of_its_own_as_a_response() {
    let server = TestServer::spawn(|app| {
        app.map_ok(tag);
        app.map_err(named!("app"));
        app.map_fallback(|| async { status!(404, "fallback") });
    })
    .await;

    let response = send(&server, Method::GET, "/nope").await;
    assert_eq!(response.status(), 404);
    assert_eq!(response.headers()["x-map-ok"], "1");
    assert_eq!(response.text().await.unwrap(), "fallback");

    server.shutdown().await;
}

#[tokio::test]
async fn it_keeps_the_cors_headers_on_an_error() {
    use volga::headers::{ACCESS_CONTROL_ALLOW_ORIGIN, ORIGIN};

    let server = TestServer::builder()
        .configure(|app| app.with_cors(|cors| cors.with_any_origin().with_any_method()))
        .setup(|app| {
            app.use_cors();
            app.map_err(named!("app"));
            app.map_get("/fails", || Err::<(), _>(StatusCode::CONFLICT));
        })
        .build()
        .await;

    let response = server
        .client()
        .get(server.url("/fails"))
        .header(ORIGIN, "http://example.test")
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 409);
    assert_eq!(response.headers()[ACCESS_CONTROL_ALLOW_ORIGIN], "*");
    assert_eq!(response.text().await.unwrap(), "app");

    server.shutdown().await;
}

#[cfg(feature = "problem-details")]
#[tokio::test]
async fn it_renders_a_405_as_problem_details_with_its_allow_header() {
    let server = TestServer::spawn(|app| {
        app.map_problem();
        app.map_get("/mapped", || "mapped");
    })
    .await;

    let response = send(&server, Method::POST, "/mapped").await;
    assert_eq!(response.status(), 405);
    assert_eq!(response.headers()["allow"], "GET,HEAD");
    assert_eq!(
        response.headers()["content-type"],
        "application/problem+json"
    );

    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["status"], 405);

    server.shutdown().await;
}

#[cfg(feature = "rate-limiting")]
#[tokio::test]
async fn it_refuses_a_request_over_its_rate_limit_with_an_error() {
    use std::time::Duration;
    use volga::rate_limiting::{FixedWindow, by};

    let server = TestServer::builder()
        .configure(|app| app.with_fixed_window(FixedWindow::new(1, Duration::from_secs(60))))
        .setup(|app| {
            app.map_get("/limited", || "ok")
                .map_ok(tag)
                .fixed_window(by::ip())
                .map_err(named!("route"));
        })
        .build()
        .await;

    let response = send(&server, Method::GET, "/limited").await;
    assert_eq!(response.status(), 200);
    assert_eq!(response.headers()["x-map-ok"], "1");

    let response = send(&server, Method::GET, "/limited").await;
    assert_eq!(response.status(), 429);
    assert!(response.headers().get("x-map-ok").is_none());
    assert_eq!(response.text().await.unwrap(), "route");

    server.shutdown().await;
}

#[cfg(any(
    feature = "compression-brotli",
    feature = "compression-gzip",
    feature = "compression-zstd",
    feature = "compression-full"
))]
#[tokio::test]
async fn it_refuses_an_unacceptable_encoding_with_an_error() {
    use volga::headers::{ACCEPT_ENCODING, VARY};

    let server = TestServer::spawn(|app| {
        app.use_compression();
        app.map_err(named!("app"));
        app.map_get("/", || "body");
    })
    .await;

    let response = server
        .client()
        .get(server.url("/"))
        .header(ACCEPT_ENCODING, "unknown")
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 406);
    assert_eq!(response.headers()[VARY], "accept-encoding");
    assert!(response.headers().contains_key(ACCEPT_ENCODING));
    assert_eq!(response.text().await.unwrap(), "app");

    server.shutdown().await;
}

#[cfg(all(feature = "jwt-auth", feature = "problem-details"))]
mod authorize {
    use super::*;
    use serde::{Deserialize, Serialize};
    use volga::{
        App,
        auth::{AuthClaims, DecodingKey, roles},
        headers::CACHE_CONTROL,
    };

    #[derive(Clone, Debug, Serialize, Deserialize)]
    struct Claims {
        sub: String,
        role: String,
        exp: u64,
    }

    impl AuthClaims for Claims {
        fn role(&self) -> Option<&str> {
            Some(&self.role)
        }
    }

    fn with_auth(app: App) -> App {
        app.with_bearer_auth(|auth| auth.set_decoding_key(DecodingKey::from_secret(b"secret")))
    }

    #[tokio::test]
    async fn it_renders_a_refusal_with_the_route_problem_handler() {
        let server = TestServer::builder()
            .configure(with_auth)
            .setup(|app| {
                app.map_get("/x", || "x")
                    .map_ok(tag)
                    .authorize::<Claims>(roles(["admin"]))
                    .map_problem();
            })
            .build()
            .await;

        let response = send(&server, Method::GET, "/x").await;
        assert_eq!(response.status(), 401);
        assert_eq!(
            response.headers()["content-type"],
            "application/problem+json"
        );
        assert_eq!(response.headers()[WWW_AUTHENTICATE], "Bearer");
        assert_eq!(response.headers()[CACHE_CONTROL], "no-store");
        assert!(response.headers().get("x-map-ok").is_none());

        let body: serde_json::Value = response.json().await.unwrap();
        assert_eq!(body["status"], 401);

        server.shutdown().await;
    }

    #[tokio::test]
    async fn it_keeps_the_challenge_through_a_handler_of_its_own() {
        let server = TestServer::builder()
            .configure(with_auth)
            .setup(|app| {
                app.map_err(
                    |err: Error| status!(err.status().as_u16(), { "error": err.to_string() }),
                );
                app.group("/api", |api| {
                    api.authorize::<Claims>(roles(["admin"]));
                    api.map_get("/x", || "x");
                });
            })
            .build()
            .await;

        let response = server
            .client()
            .get(server.url("/api/x"))
            .header("Authorization", "Bearer not-a-jwt")
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), 401);
        let challenge = response.headers()[WWW_AUTHENTICATE].to_str().unwrap();
        assert!(
            challenge.contains(r#"error="invalid_token""#),
            "{challenge}"
        );
        assert_eq!(response.headers()[CACHE_CONTROL], "no-store");

        let body: serde_json::Value = response.json().await.unwrap();
        assert_eq!(body["error"], "Token is malformed or invalid");

        server.shutdown().await;
    }
}
