#![allow(missing_docs)]
#![cfg(feature = "test")]

//! Requests that match no route, or match a path but not a method, run the same
//! pipeline as a matched request: global middleware, the per-request scope its
//! extractors read, and the application error handler (#229).

use volga::test::TestServer;
use volga::{ClientIp, ok};

#[cfg(feature = "middleware")]
use volga::{
    HttpResponse, HttpResult,
    error::Error,
    headers::{ACCESS_CONTROL_ALLOW_ORIGIN, ACCESS_CONTROL_REQUEST_METHOD, ORIGIN, StatusCode},
    http::Method,
    middleware::Next,
};

#[cfg(feature = "middleware")]
const MAPPED: &str = "/mapped";
#[cfg(feature = "middleware")]
const UNMATCHED: &str = "/unmatched";

#[cfg(feature = "middleware")]
async fn tag(mut response: HttpResponse) -> HttpResult {
    response.try_insert_raw_header("x-map-ok", "1")?;
    Ok(response)
}

#[cfg(feature = "middleware")]
#[tokio::test]
async fn it_runs_global_middleware_for_an_unmatched_path() {
    let server = TestServer::spawn(|app| {
        app.wrap(|ctx, next| async move {
            let mut response = next(ctx).await?;
            response.try_insert_raw_header("x-wrap", "1")?;
            Ok(response)
        });
        app.with(|next: Next| async move {
            let mut response: HttpResponse = next.await?;
            response.try_insert_raw_header("x-with", "1")?;
            Ok::<_, Error>(response)
        });
        app.map_ok(tag);

        app.map_get(MAPPED, || async { ok!(text: "mapped") });
        app.map_fallback(|| async { ok!(text: "fallback") });
    })
    .await;

    for (path, status) in [(MAPPED, 200), (UNMATCHED, 200)] {
        let response = server.client().get(server.url(path)).send().await.unwrap();

        assert_eq!(response.status().as_u16(), status, "{path}");
        for header in ["x-wrap", "x-with", "x-map-ok"] {
            assert_eq!(
                response.headers().get(header).map(|v| v.to_str().unwrap()),
                Some("1"),
                "{header} missing for {path}"
            );
        }
    }

    server.shutdown().await;
}

#[cfg(feature = "middleware")]
#[tokio::test]
async fn it_runs_global_middleware_for_an_unmapped_method() {
    let server = TestServer::spawn(|app| {
        app.map_ok(tag);
        app.map_get(MAPPED, || async { ok!(text: "mapped") });
    })
    .await;

    let response = server
        .client()
        .post(server.url(MAPPED))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status().as_u16(), 405);
    assert_eq!(response.headers().get("allow").unwrap(), "GET,HEAD");
    assert_eq!(response.headers().get("x-map-ok").unwrap(), "1");

    server.shutdown().await;
}

#[cfg(feature = "middleware")]
#[tokio::test]
async fn it_short_circuits_an_unmatched_path_from_a_filter() {
    let server = TestServer::spawn(|app| {
        app.filter(|headers: volga::headers::HttpHeaders| async move {
            headers.get_raw("x-key").is_some()
        });
        app.map_get(MAPPED, || async { ok!(text: "mapped") });
        app.map_fallback(|| async { ok!(text: "fallback") });
    })
    .await;

    for path in [MAPPED, UNMATCHED] {
        let response = server.client().get(server.url(path)).send().await.unwrap();
        assert_eq!(response.status().as_u16(), 400, "{path}");
    }

    let response = server
        .client()
        .get(server.url(UNMATCHED))
        .header("x-key", "1")
        .send()
        .await
        .unwrap();

    assert_eq!(response.status().as_u16(), 200);
    assert_eq!(response.text().await.unwrap(), "fallback");

    server.shutdown().await;
}

#[cfg(all(feature = "middleware", feature = "rate-limiting"))]
#[tokio::test]
async fn it_rate_limits_an_unmatched_path() {
    use std::time::Duration;
    use volga::rate_limiting::{TokenBucket, by};

    let server = TestServer::builder()
        .configure(|app| {
            app.with_token_bucket(
                TokenBucket::new(3, 0.0)
                    .with_name("tiny")
                    .with_eviction(Duration::from_secs(300)),
            )
        })
        .setup(|app| {
            app.use_token_bucket(by::ip().using("tiny"));
            app.map_get(MAPPED, || async { ok!(text: "mapped") });
            app.map_fallback(|| async { ok!(text: "fallback") });
        })
        .build()
        .await;

    // The bucket is keyed by IP, so the mapped path spends the same three tokens
    // the unmatched one would - request the unmatched path first and it has to
    // run out on its own.
    let client = server.client();
    let url = server.url(UNMATCHED);

    for _ in 0..3 {
        let response = client.get(&url).send().await.unwrap();
        assert!(response.status().is_success());
    }

    for _ in 0..2 {
        let response = client.get(&url).send().await.unwrap();
        assert_eq!(response.status().as_u16(), 429);
    }

    server.shutdown().await;
}

#[cfg(feature = "middleware")]
#[tokio::test]
async fn it_applies_cors_headers_to_an_unmatched_path() {
    let server = TestServer::builder()
        .configure(|app| app.with_cors(|cors| cors.with_any_origin().with_any_method()))
        .setup(|app| {
            app.use_cors();
            app.map_get(MAPPED, || async { ok!(text: "mapped") });
        })
        .build()
        .await;

    let response = server
        .client()
        .get(server.url(UNMATCHED))
        .header(&ORIGIN, "http://example.test")
        .send()
        .await
        .unwrap();

    assert_eq!(response.status().as_u16(), 404);
    assert_eq!(
        response
            .headers()
            .get(&ACCESS_CONTROL_ALLOW_ORIGIN)
            .unwrap(),
        "*"
    );

    server.shutdown().await;
}

#[cfg(feature = "middleware")]
#[tokio::test]
async fn it_does_not_answer_a_preflight_for_an_unmatched_path() {
    let server = TestServer::builder()
        .configure(|app| app.with_cors(|cors| cors.with_any_origin().with_any_method()))
        .setup(|app| {
            app.use_cors();
            app.map_get(MAPPED, || async { ok!(text: "mapped") });
        })
        .build()
        .await;

    let response = server
        .client()
        .request(Method::OPTIONS, server.url(UNMATCHED))
        .header(&ORIGIN, "http://example.test")
        .header(ACCESS_CONTROL_REQUEST_METHOD, "GET")
        .send()
        .await
        .unwrap();

    // A preflight only succeeds for a route that exists; the CORS headers ride
    // along on the 404 so the browser reports a missing resource rather than a
    // blocked origin.
    assert_eq!(response.status().as_u16(), 404);
    assert_eq!(
        response
            .headers()
            .get(&ACCESS_CONTROL_ALLOW_ORIGIN)
            .unwrap(),
        "*"
    );

    server.shutdown().await;
}

#[cfg(feature = "middleware")]
#[tokio::test]
async fn it_does_not_answer_a_preflight_for_an_unmapped_method() {
    let server = TestServer::builder()
        .configure(|app| app.with_cors(|cors| cors.with_any_origin().with_any_method()))
        .setup(|app| {
            app.use_cors();
            app.map_get(MAPPED, || async { ok!(text: "mapped") });
        })
        .build()
        .await;

    let response = server
        .client()
        .request(Method::OPTIONS, server.url(MAPPED))
        .header(&ORIGIN, "http://example.test")
        .header(ACCESS_CONTROL_REQUEST_METHOD, "DELETE")
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
    assert_eq!(response.headers().get("allow").unwrap(), "GET,HEAD");
    assert_eq!(
        response
            .headers()
            .get(&ACCESS_CONTROL_ALLOW_ORIGIN)
            .unwrap(),
        "*"
    );

    server.shutdown().await;
}

#[tokio::test]
async fn it_extracts_the_request_scope_in_a_fallback() {
    let server = TestServer::spawn(|app| {
        app.map_fallback(|ip: ClientIp| async move { ok!("{}", ip.into_inner().ip()) });
    })
    .await;

    let response = server
        .client()
        .get(server.url("/unmatched"))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status().as_u16(), 200);

    let ip: std::net::IpAddr = response.text().await.unwrap().parse().unwrap();
    assert!(ip.is_loopback());

    server.shutdown().await;
}

#[tokio::test]
async fn it_routes_a_fallback_error_to_the_error_handler() {
    use volga::{error::Error, status};

    let server = TestServer::spawn(|app| {
        app.map_err(|error: Error| async move { status!(418, "{}", error) });
        app.map_fallback(|| async { Err::<&str, _>(Error::server_error("no such thing")) });
    })
    .await;

    let response = server
        .client()
        .get(server.url("/unmatched"))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status().as_u16(), 418);
    assert_eq!(response.text().await.unwrap(), "no such thing");

    server.shutdown().await;
}
