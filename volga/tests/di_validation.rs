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
