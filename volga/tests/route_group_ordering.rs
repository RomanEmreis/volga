#![allow(missing_docs)]
#![cfg(all(feature = "test", feature = "middleware"))]

//! A route group is a scope: what it configures reaches every route it registered,
//! whether the route was mapped before or after the configuration.

use std::sync::{Arc, Mutex};
use volga::headers::{ACCESS_CONTROL_ALLOW_ORIGIN, ORIGIN};
use volga::http::Method;
use volga::test::TestServer;

/// Records the order in which middleware entered the pipeline.
type Trace = Arc<Mutex<Vec<&'static str>>>;

/// Builds middleware that notes its own name on the way in.
macro_rules! mark {
    ($trace:expr, $name:literal) => {{
        let trace: Trace = Arc::clone($trace);
        move |ctx: volga::middleware::HttpContext, next: volga::middleware::NextFn| {
            let trace = Arc::clone(&trace);
            async move {
                trace.lock().expect("trace is not poisoned").push($name);
                next(ctx).await
            }
        }
    }};
}

/// Returns what the middleware recorded while serving `path`.
async fn trace_of(server: &TestServer, trace: &Trace, path: &str) -> Vec<&'static str> {
    let response = server.client().get(server.url(path)).send().await.unwrap();
    assert!(response.status().is_success(), "{path}: {response:?}");

    let recorded = trace.lock().expect("trace is not poisoned").clone();
    trace.lock().expect("trace is not poisoned").clear();
    recorded
}

#[tokio::test]
async fn it_applies_group_middleware_registered_after_a_route() {
    let trace = Trace::default();
    let group_trace = Arc::clone(&trace);

    let server = TestServer::spawn(move |app| {
        app.group("/api", |api| {
            api.map_get("/hello", || async { "Hello, World!" });
            api.wrap(mark!(&group_trace, "group"));
        });
    })
    .await;

    assert_eq!(trace_of(&server, &trace, "/api/hello").await, ["group"]);

    server.shutdown().await;
}

#[tokio::test]
async fn it_keeps_the_registration_order_of_group_middleware_interleaved_with_routes() {
    let trace = Trace::default();
    let group_trace = Arc::clone(&trace);

    let server = TestServer::spawn(move |app| {
        app.group("/api", |api| {
            api.wrap(mark!(&group_trace, "first"));
            api.map_get("/hello", || async { "Hello, World!" });
            api.wrap(mark!(&group_trace, "second"));
        });
    })
    .await;

    assert_eq!(
        trace_of(&server, &trace, "/api/hello").await,
        ["first", "second"]
    );

    server.shutdown().await;
}

#[tokio::test]
async fn it_runs_group_middleware_before_route_middleware() {
    let trace = Trace::default();
    let group_trace = Arc::clone(&trace);

    let server = TestServer::spawn(move |app| {
        app.group("/api", |api| {
            api.map_get("/hello", || async { "Hello, World!" })
                .wrap(mark!(&group_trace, "route"));
            api.wrap(mark!(&group_trace, "group"));
        });
    })
    .await;

    assert_eq!(
        trace_of(&server, &trace, "/api/hello").await,
        ["group", "route"]
    );

    server.shutdown().await;
}

#[tokio::test]
async fn it_inherits_parent_middleware_in_a_sub_group_declared_before_it() {
    let trace = Trace::default();
    let group_trace = Arc::clone(&trace);

    let server = TestServer::spawn(move |app| {
        app.group("/api", |api| {
            api.group("/early", |sub| {
                sub.wrap(mark!(&group_trace, "early"));
                sub.map_get("/hello", || async { "Hello, World!" });
            });

            api.wrap(mark!(&group_trace, "parent"));

            api.group("/late", |sub| {
                sub.wrap(mark!(&group_trace, "late"));
                sub.map_get("/hello", || async { "Hello, World!" });
            });
        });
    })
    .await;

    assert_eq!(
        trace_of(&server, &trace, "/api/early/hello").await,
        ["parent", "early"]
    );
    assert_eq!(
        trace_of(&server, &trace, "/api/late/hello").await,
        ["parent", "late"]
    );

    server.shutdown().await;
}

#[tokio::test]
async fn it_runs_an_outer_group_before_every_scope_nested_in_it() {
    let trace = Trace::default();
    let group_trace = Arc::clone(&trace);

    let server = TestServer::spawn(move |app| {
        app.group("/api", |api| {
            api.group("/users", |users| {
                users
                    .map_get("/{id}", |id: i32| async move { id })
                    .wrap(mark!(&group_trace, "route"));
                users.wrap(mark!(&group_trace, "sub-group"));
            });
            api.wrap(mark!(&group_trace, "group"));
        });
    })
    .await;

    assert_eq!(
        trace_of(&server, &trace, "/api/users/1").await,
        ["group", "sub-group", "route"]
    );

    server.shutdown().await;
}

/// The CORS policy of a group reaches the routes above it, and the one a route or a
/// sub-group chose for itself is not replaced by the one the enclosing scope chose.
#[tokio::test]
async fn it_applies_group_cors_registered_after_a_route() {
    let server = TestServer::builder()
        .configure(|app| {
            app.with_cors(|cors| {
                cors.with_name("api")
                    .with_origins(["https://example.test"])
                    .with_methods([Method::GET])
            })
            .with_cors(|cors| {
                cors.with_name("other")
                    .with_origins(["https://other.test"])
                    .with_methods([Method::GET])
            })
        })
        .setup(|app| {
            app.use_cors();

            app.group("/api", |api| {
                api.map_get("/hello", || async { "Hello, World!" });
                api.map_get("/own", || async { "Hello, World!" })
                    .cors_with("other");

                api.group("/users", |users| {
                    users.map_get("/{id}", |id: i32| async move { id });
                    users.cors_with("other");
                });

                api.cors_with("api");
            });
        })
        .build()
        .await;

    for (path, origin, expected) in [
        (
            "/api/hello",
            "https://example.test",
            Some("https://example.test"),
        ),
        ("/api/own", "https://example.test", None),
        ("/api/own", "https://other.test", Some("https://other.test")),
        (
            "/api/users/1",
            "https://other.test",
            Some("https://other.test"),
        ),
    ] {
        let response = server
            .client()
            .get(server.url(path))
            .header(&ORIGIN, origin)
            .send()
            .await
            .unwrap();

        assert!(response.status().is_success(), "{path}: {response:?}");
        assert_eq!(
            response
                .headers()
                .get(&ACCESS_CONTROL_ALLOW_ORIGIN)
                .map(|value| value.to_str().unwrap()),
            expected,
            "{path} from {origin}"
        );
    }

    server.shutdown().await;
}

#[cfg(feature = "openapi")]
#[tokio::test]
async fn it_applies_group_open_api_config_registered_after_a_route() {
    let server = TestServer::builder()
        .configure(|app| app.with_open_api(|open_api| open_api))
        .setup(|app| {
            app.use_open_api();

            app.group("/api", |api| {
                api.map_get("/hello", || async { "Hello, World!" })
                    .open_api(|op| op.with_summary("Says hello"));
                api.open_api(|op| op.with_description("The API"));
            });
        })
        .build()
        .await;

    let spec: serde_json::Value = server
        .client()
        .get(server.url("/openapi.json"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    let operation = &spec["paths"]["/api/hello"]["get"];

    assert_eq!(operation["summary"], "Says hello");
    assert_eq!(operation["description"], "The API");
    assert_eq!(operation["tags"][0], "/api");

    server.shutdown().await;
}
