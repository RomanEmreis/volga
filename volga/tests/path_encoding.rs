#![allow(missing_docs)]
#![cfg(feature = "test")]

//! A request path is percent-decoded once, in the router (#252): every extractor reads a
//! parameter the same way, a literal segment is matched against the text a segment spells,
//! and a path that does not decode is answered `400`.

use serde::Deserialize;
use volga::test::TestServer;
use volga::{NamedPath, Path, error::Error, ok, status};

async fn get(server: &TestServer, path: &str) -> (u16, String) {
    let response = server
        .client()
        .get(server.url(path))
        .send()
        .await
        .expect("the request failed");
    let status = response.status().as_u16();
    (status, response.text().await.expect("no body"))
}

#[derive(Deserialize)]
struct Name {
    name: String,
}

#[derive(Deserialize)]
struct Id {
    id: u32,
}

async fn spawn_extractors() -> TestServer {
    TestServer::spawn(|app| {
        app.map_get("/s/{name}", |name: String| async move { ok!("{name:?}") });
        app.map_get("/p/{name}", |Path((name,)): Path<(String,)>| async move {
            ok!("{name:?}")
        });
        app.map_get("/n/{name}", |NamedPath(n): NamedPath<Name>| async move {
            ok!("{:?}", n.name)
        });
        app.map_get("/su/{id}", |id: u32| async move { ok!("{id}") });
        app.map_get("/nu/{id}", |NamedPath(n): NamedPath<Id>| async move {
            ok!("{}", n.id)
        });
    })
    .await
}

/// Positional extractors and `NamedPath<T>` read one value
#[tokio::test]
async fn it_reads_a_parameter_the_same_way_through_every_extractor() {
    let server = spawn_extractors().await;

    for (segment, expected) in [
        ("John%20Doe", "\"John Doe\""),
        ("100%25", "\"100%\""),
        ("caf%C3%A9", "\"caf\u{e9}\""),
        ("a%2Fb", "\"a/b\""),
        ("C++", "\"C++\""),
        ("a&b=c", "\"a&b=c\""),
        ("%2520", "\"%20\""),
    ] {
        for prefix in ["s", "p", "n"] {
            assert_eq!(
                get(&server, &format!("/{prefix}/{segment}")).await,
                (200, expected.into()),
                "/{prefix}/{segment}"
            );
        }
    }

    for prefix in ["su", "nu"] {
        assert_eq!(
            get(&server, &format!("/{prefix}/%31")).await,
            (200, "1".into()),
            "/{prefix}/%31"
        );
    }

    server.shutdown().await;
}

/// A malformed escape, or escapes that are not UTF-8, is not a valid path, whichever
/// extractor would have read it
#[tokio::test]
async fn it_answers_a_path_that_does_not_decode_with_400() {
    let server = spawn_extractors().await;

    for segment in ["bad%zz", "%FF", "%2", "caf%C3"] {
        for prefix in ["s", "p", "n", "su", "nu"] {
            let path = format!("/{prefix}/{segment}");
            assert_eq!(get(&server, &path).await.0, 400, "{path}");
        }
    }
    // Before any route is looked up
    assert_eq!(get(&server, "/nothing/%zz").await.0, 400);

    server.shutdown().await;
}

#[tokio::test]
async fn it_reaches_a_literal_that_has_to_be_encoded_on_the_wire() {
    let server = TestServer::spawn(|app| {
        app.map_get("/caf\u{e9}", || async { ok!("literal hit") });
        app.map_get("/lit/a b", || async { ok!("literal hit") });
        app.map_get(
            "/lit/{name}",
            |name: String| async move { ok!("param {name}") },
        );
    })
    .await;

    assert_eq!(
        get(&server, "/caf%C3%A9").await,
        (200, "literal hit".into())
    );
    assert_eq!(
        get(&server, "/lit/a%20b").await,
        (200, "literal hit".into())
    );
    assert_eq!(get(&server, "/lit/a+b").await, (200, "param a+b".into()));
    assert_eq!(
        get(&server, "/lit/a%2520b").await,
        (200, "param a%20b".into())
    );

    server.shutdown().await;
}

#[tokio::test]
#[should_panic(expected = "carries a percent-escape")]
async fn it_refuses_a_literal_written_with_a_percent_escape() {
    let mut app = volga::App::new();
    app.map_get("/lit/a%20b", || async { ok!() });
}

/// The `400` is an error like any other: it reaches the error handler
#[tokio::test]
async fn it_hands_a_path_that_does_not_decode_to_the_error_handler() {
    let server = TestServer::spawn(|app| {
        app.map_err(
            |error: Error| async move { status!(error.status().as_u16(), text: "handled") },
        );
        app.map_get("/s/{name}", |name: String| async move { name });
    })
    .await;

    assert_eq!(get(&server, "/s/%zz").await, (400, "handled".into()));

    server.shutdown().await;
}

/// It runs through the global middleware, as a `404` and a `405` do
#[cfg(feature = "middleware")]
#[tokio::test]
async fn it_runs_the_global_middleware_for_a_path_that_does_not_decode() {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    let unmatched = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&unmatched);

    let server = TestServer::spawn(move |app| {
        app.wrap(move |ctx, next| {
            if !ctx.matched_route() {
                counter.fetch_add(1, Ordering::SeqCst);
            }
            next(ctx)
        });
        app.map_get("/s/{name}", |name: String| async move { name });
    })
    .await;

    assert_eq!(get(&server, "/s/%zz").await.0, 400);
    assert_eq!(unmatched.load(Ordering::SeqCst), 1);

    server.shutdown().await;
}
