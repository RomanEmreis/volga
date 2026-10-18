#![allow(missing_docs)]
#![cfg(feature = "di")]

//! A dependency graph that cannot be resolved fails `App::run` at startup, before a
//! connection is accepted, instead of failing the first request that reaches it.

use std::{net::TcpListener, time::Duration};
use volga::{App, di::Dc};

struct A;
struct B;
struct Unregistered;

/// Runs the app and returns the error it refused to start with. An app that starts anyway
/// would serve forever, so a start is bounded and fails the test instead of hanging it.
async fn startup_error(app: App) -> std::io::Error {
    let listener = TcpListener::bind("127.0.0.1:0").expect("failed to bind");
    tokio::time::timeout(Duration::from_secs(5), app.run_with_std_listener(listener))
        .await
        .expect("the app started and kept serving")
        .expect_err("the app started")
}

#[tokio::test]
async fn it_refuses_to_start_on_a_dependency_cycle() {
    let mut app = App::new().without_greeter();
    app.add_scoped_factory(|_: Dc<B>| Ok(A));
    app.add_transient_factory(|_: Dc<A>| Ok(B));

    let message = startup_error(app).await.to_string();
    let (a, b) = (std::any::type_name::<A>(), std::any::type_name::<B>());
    assert!(
        message.contains(&format!("dependency cycle: {a} -> {b} -> {a}")),
        "{message}"
    );
}

#[tokio::test]
async fn it_refuses_to_start_on_a_dependency_nobody_registered() {
    let mut app = App::new().without_greeter();
    app.add_scoped_factory(|_: Dc<Unregistered>| Ok(A));

    let message = startup_error(app).await.to_string();
    assert!(
        message.contains(&format!(
            "`{}` depends on `{}`, which is not registered",
            std::any::type_name::<A>(),
            std::any::type_name::<Unregistered>()
        )),
        "{message}"
    );
}

/// A graph that does not resolve stops the app before anything is started, not after. What
/// this can observe is a `shutdown_on` trigger: a spawned one is polled, one that was never
/// spawned is dropped without ever being. The same ordering is what keeps the HTTPS redirect
/// listener of a TLS app from being left bound on a start that failed.
#[tokio::test]
async fn it_starts_no_background_task_when_the_graph_does_not_resolve() {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };

    let polled = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&polled);

    let mut app = App::new().without_greeter().shutdown_on(async move {
        flag.store(true, Ordering::SeqCst);
        std::future::pending::<()>().await;
    });
    app.add_scoped_factory(|_: Dc<Unregistered>| Ok(A));

    let _ = startup_error(app).await;
    tokio::task::yield_now().await;

    assert!(
        !polled.load(Ordering::SeqCst),
        "a shutdown trigger ran before the graph was known to resolve"
    );
}

#[cfg(feature = "test")]
#[tokio::test]
async fn it_starts_with_a_graph_that_resolves() {
    use volga::test::TestServer;

    struct Greeting(&'static str);
    struct Greeter(std::sync::Arc<Greeting>);

    let server = TestServer::spawn(|app| {
        app.add_singleton(Greeting("hello"));
        app.add_scoped_factory(|greeting: Dc<Greeting>| Ok(Greeter(greeting.into_inner())));
        app.map_get("/", |greeter: Dc<Greeter>| async move { greeter.0.0 });
    })
    .await;

    let response = server
        .client()
        .get(server.url("/"))
        .send()
        .await
        .expect("the request failed");
    assert_eq!(response.text().await.expect("no body"), "hello");

    server.shutdown().await;
}

#[tokio::test]
async fn it_refuses_to_start_on_a_handler_taking_a_service_nobody_registered() {
    let mut app = App::new().without_greeter();
    app.map_get("/unreg", |_: Dc<Unregistered>| async { "ok" });

    let message = startup_error(app).await.to_string();
    assert!(
        message.contains(&format!(
            "`{}` is not registered, and is resolved by `GET /unreg`",
            std::any::type_name::<Unregistered>()
        )),
        "{message}"
    );
    // The report says what to do about it
    assert!(message.contains("`add_singleton`"), "{message}");
    assert!(message.contains("`Option<Dc<T>>`"), "{message}");
}

#[tokio::test]
async fn it_names_every_handler_taking_a_service_nobody_registered_in_one_issue() {
    let mut app = App::new().without_greeter();
    app.map_get("/a", |_: Dc<Unregistered>| async { "a" });
    app.group("/api", |api| {
        api.map_post("/b", |_: Dc<Unregistered>, _: Dc<B>| async { "b" });
    });
    app.add_singleton(B);

    let message = startup_error(app).await.to_string();
    assert!(
        message.contains(&format!(
            "`{}` is not registered, and is resolved by `GET /a`, `POST /api/b`",
            std::any::type_name::<Unregistered>()
        )),
        "{message}"
    );
    assert!(
        !message.contains(std::any::type_name::<B>()),
        "a registered service is reported: {message}"
    );
}

#[tokio::test]
async fn it_refuses_to_start_on_a_service_a_handler_takes_through_a_result() {
    let mut app = App::new().without_greeter();
    // A `Result` hands the handler the error, but the service is still expected to be there
    app.map_get(
        "/",
        |_: Result<Dc<Unregistered>, volga::error::Error>| async { "ok" },
    );

    let message = startup_error(app).await.to_string();
    assert!(message.contains("resolved by `GET /`"), "{message}");
}

#[tokio::test]
async fn it_refuses_to_start_on_an_error_handler_or_fallback_taking_a_service_nobody_registered() {
    let mut app = App::new().without_greeter();
    app.map_err(|err: volga::error::Error, _: Dc<Unregistered>| async move { err });
    app.map_fallback(|_: Dc<Unregistered>| async { "fallback" });

    let message = startup_error(app).await.to_string();
    assert!(
        message.contains("resolved by `map_err`, `map_fallback`"),
        "{message}"
    );
}

#[cfg(feature = "middleware")]
#[tokio::test]
async fn it_refuses_to_start_on_middleware_taking_a_service_nobody_registered() {
    use volga::{HttpRequestMut, HttpResponse, middleware::Next};

    let mut app = App::new().without_greeter();
    app.with(|_: Dc<Unregistered>, next: Next| next);
    app.tap_req(|req: HttpRequestMut, _: Dc<Unregistered>| async move { req });
    app.map_get("/", || async { "ok" })
        .filter(|_: Dc<Unregistered>| async { true })
        .map_ok(|resp: HttpResponse, _: Dc<Unregistered>| async move { resp });
    app.group("/api", |api| {
        api.map_err(|err: volga::error::Error, _: Dc<Unregistered>| async move { err });
        api.map_fallback(|_: Dc<Unregistered>| async { "fallback" });
        api.map_get("/", || async { "ok" });
    });

    let message = startup_error(app).await.to_string();
    // What a group maps is declared when the app starts, after what is always attached
    assert!(
        message.contains(&format!(
            "`{}` is not registered, and is resolved by global `with`, global `tap_req`, \
             `filter` on `GET /`, `map_ok` on `GET /`, `map_err` on group `/api`, \
             `map_fallback` on group `/api`",
            std::any::type_name::<Unregistered>()
        )),
        "{message}"
    );
}

#[cfg(feature = "middleware")]
#[tokio::test]
async fn it_checks_group_middleware_reaching_only_the_routes_of_a_sub_group() {
    use volga::middleware::Next;

    let mut app = App::new().without_greeter();
    app.group("/api", |api| {
        api.with(|_: Dc<Unregistered>, next: Next| next);
        api.group("/v1", |v1| {
            v1.map_get("/x", || async { "x" });
        });
    });

    let message = startup_error(app).await.to_string();
    assert!(
        message.contains("resolved by `with` on group `/api`"),
        "{message}"
    );
}

#[cfg(all(feature = "middleware", feature = "test"))]
#[tokio::test]
async fn it_starts_when_group_middleware_has_nothing_to_run_on() {
    use volga::{middleware::Next, test::TestServer};

    // The group maps no route, fallback or mount, so its middleware never runs and what it
    // resolves cannot fail a request
    let server = TestServer::spawn(|app| {
        app.group("/api", |api| {
            api.with(|_: Dc<Unregistered>, next: Next| next);
            api.filter(|_: Dc<Unregistered>| true);
            api.group("/v1", |_| {});
        });
        app.map_get("/", || async { "ok" });
    })
    .await;

    let response = server
        .client()
        .get(server.url("/"))
        .send()
        .await
        .expect("the request failed");
    assert_eq!(response.text().await.expect("no body"), "ok");

    server.shutdown().await;
}

#[cfg(feature = "ws")]
#[tokio::test]
async fn it_refuses_to_start_on_a_websocket_handler_taking_a_service_nobody_registered() {
    use volga::ws::WebSocket;

    let mut app = App::new().without_greeter();
    app.map_ws("/ws", |_: WebSocket, _: Dc<Unregistered>| async {});
    app.map_msg("/msg", |msg: String, _: Dc<Unregistered>| msg);

    let message = startup_error(app).await.to_string();
    assert!(
        message.contains("resolved by `map_ws` at `/ws`, `map_msg` at `/msg`"),
        "{message}"
    );
}

#[cfg(feature = "signed-cookie")]
#[tokio::test]
async fn it_refuses_to_start_on_signed_cookies_without_a_key() {
    use volga::http::{SignedCookies, SignedKey};

    let mut app = App::new().without_greeter();
    app.map_get("/", |_: SignedCookies| async { "ok" });

    let message = startup_error(app).await.to_string();
    assert!(
        message.contains(&format!(
            "`{}` is not registered, and is resolved by `GET /`",
            std::any::type_name::<SignedKey>()
        )),
        "{message}"
    );
}

#[cfg(feature = "test")]
#[tokio::test]
async fn it_starts_when_a_handler_takes_a_service_nobody_registered_as_optional() {
    use volga::test::TestServer;

    let server = TestServer::spawn(|app| {
        app.map_get("/", |service: Option<Dc<Unregistered>>| async move {
            if service.is_some() { "some" } else { "none" }
        });
    })
    .await;

    let response = server
        .client()
        .get(server.url("/"))
        .send()
        .await
        .expect("the request failed");
    assert_eq!(response.text().await.expect("no body"), "none");

    server.shutdown().await;
}

#[cfg(all(feature = "middleware", feature = "static-files"))]
#[tokio::test]
async fn it_checks_group_middleware_reaching_only_a_static_mount() {
    use volga::middleware::Next;

    let mut app = App::new()
        .without_greeter()
        .with_host_env(|env| env.with_content_root("tests/static"));
    app.group("/assets", |assets| {
        assets.with(|_: Dc<Unregistered>, next: Next| next);
        assets.use_static_assets();
    });

    let message = startup_error(app).await.to_string();
    assert!(
        message.contains("resolved by `with` on group `/assets`"),
        "{message}"
    );
}

#[cfg(all(feature = "middleware", feature = "static-files", feature = "test"))]
#[tokio::test]
async fn it_starts_when_group_middleware_reaches_only_mounts_that_are_turned_down() {
    use volga::{middleware::Next, test::TestServer};

    let server = TestServer::builder()
        .configure(|app| app.with_host_env(|env| env.with_content_root("tests/static")))
        .setup(|app| {
            // A mount under a route parameter is turned down when it is registered
            app.group("/{tenant}", |tenant| {
                tenant.with(|_: Dc<Unregistered>, next: Next| next);
                tenant.use_static_assets();
            });
            // So is a second mount on a prefix another mount answers
            app.group("/static", |first| {
                first.use_static_assets();
            });
            app.group("/static", |second| {
                second.filter(|_: Dc<Unregistered>| true);
                second.use_static_assets();
            });
            app.map_get("/", || async { "ok" });
        })
        .build()
        .await;

    let response = server
        .client()
        .get(server.url("/"))
        .send()
        .await
        .expect("the request failed");
    assert_eq!(response.text().await.expect("no body"), "ok");

    server.shutdown().await;
}

#[cfg(feature = "test")]
#[tokio::test]
async fn it_starts_when_the_handlers_taking_a_service_nobody_registered_are_replaced() {
    use volga::{error::Error, test::TestServer};

    // Each slot holds one handler, so the first one in each pair never runs
    let server = TestServer::spawn(|app| {
        app.map_err(|err: Error, _: Dc<Unregistered>| async move { err });
        app.map_err(|err: Error| async move { err });

        app.map_fallback(|_: Dc<Unregistered>| async { "first" });
        app.map_fallback(|| async { "fallback" });

        app.group("/api", |api| {
            api.map_fallback(|_: Dc<Unregistered>| async { "first" });
            api.map_fallback(|| async { "api" });
        });
    })
    .await;

    let client = server.client();
    for (path, expected) in [("/nope", "fallback"), ("/api/nope", "api")] {
        let body = client
            .get(server.url(path))
            .send()
            .await
            .expect("the request failed")
            .text()
            .await
            .expect("no body");
        assert_eq!(body, expected, "{path}");
    }

    server.shutdown().await;
}

#[cfg(all(feature = "problem-details", feature = "test"))]
#[tokio::test]
async fn it_starts_when_problem_details_replace_an_error_handler_taking_a_service_nobody_registered()
 {
    use volga::{error::Error, test::TestServer};

    let server = TestServer::spawn(|app| {
        app.map_err(|err: Error, _: Dc<Unregistered>| async move { err });
        app.use_problem_details();
        app.map_get("/", || async { "ok" });
    })
    .await;

    let response = server
        .client()
        .get(server.url("/"))
        .send()
        .await
        .expect("the request failed");
    assert_eq!(response.text().await.expect("no body"), "ok");

    server.shutdown().await;
}
