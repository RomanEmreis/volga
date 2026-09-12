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
