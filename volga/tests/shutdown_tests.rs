#![allow(missing_docs)]
#![cfg(feature = "test")]

use std::time::{Duration, Instant};

use volga::{App, ShutdownHandle, ok};

fn pick_free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

async fn wait_until_listening(port: u16) {
    let deadline = Instant::now() + Duration::from_secs(5);
    let url = format!("http://127.0.0.1:{port}/ping");
    while Instant::now() < deadline {
        if reqwest::get(&url).await.is_ok() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("server never started listening on port {port}");
}

fn build_app(port: u16) -> (App, ShutdownHandle) {
    let (app, handle) = App::with_shutdown();
    let mut app = app.bind(format!("127.0.0.1:{port}")).without_greeter();
    app.map_get("/ping", || async { ok!("pong") });
    (app, handle)
}

#[tokio::test]
async fn manual_shutdown_stops_a_running_server() {
    let port = pick_free_port();
    let (app, handle) = build_app(port);
    let task = tokio::spawn(async move { app.run().await });

    wait_until_listening(port).await;

    let response = reqwest::get(format!("http://127.0.0.1:{port}/ping"))
        .await
        .unwrap();
    assert!(response.status().is_success());

    handle.shutdown();

    let result = tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .expect("server did not exit after shutdown")
        .expect("server task panicked");
    result.expect("server returned an error");
}

#[tokio::test]
async fn shutdown_is_idempotent_with_a_running_server() {
    let port = pick_free_port();
    let (app, handle) = build_app(port);
    let task = tokio::spawn(async move { app.run().await });

    wait_until_listening(port).await;

    handle.shutdown();
    handle.shutdown(); // second call must be a no-op

    let result = tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .expect("server did not exit after shutdown")
        .expect("server task panicked");
    result.expect("server returned an error");
}

#[tokio::test]
async fn on_signal_constructor_drives_server_shutdown() {
    let port = pick_free_port();
    let (signal_tx, signal_rx) = tokio::sync::oneshot::channel::<()>();
    let handle = ShutdownHandle::on_signal(async move {
        let _ = signal_rx.await;
    });

    let mut app = App::new()
        .with_shutdown_signal(handle)
        .bind(format!("127.0.0.1:{port}"))
        .without_greeter();
    app.map_get("/ping", || async { ok!("pong") });
    let task = tokio::spawn(async move { app.run().await });

    wait_until_listening(port).await;

    signal_tx.send(()).unwrap();

    tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .expect("server did not exit after on_signal trigger")
        .expect("server task panicked")
        .expect("server returned an error");
}

#[tokio::test]
async fn shutdown_on_chained_triggers_compose() {
    let port = pick_free_port();
    let handle = ShutdownHandle::new();
    let (tx_a, rx_a) = tokio::sync::oneshot::channel::<()>();
    let (_tx_b, rx_b) = tokio::sync::oneshot::channel::<()>();
    handle.shutdown_on(async move {
        let _ = rx_a.await;
    });
    handle.shutdown_on(async move {
        let _ = rx_b.await;
    });

    let mut app = App::new()
        .with_shutdown_signal(handle.clone())
        .bind(format!("127.0.0.1:{port}"))
        .without_greeter();
    app.map_get("/ping", || async { ok!("pong") });
    let task = tokio::spawn(async move { app.run().await });

    wait_until_listening(port).await;

    // Firing only the first trigger is enough.
    tx_a.send(()).unwrap();

    tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .expect("server did not exit after first trigger")
        .expect("server task panicked")
        .expect("server returned an error");
}

#[tokio::test]
async fn from_cancellation_token_drives_server_shutdown() {
    use tokio_util::sync::CancellationToken;

    let port = pick_free_port();
    let token = CancellationToken::new();
    let handle: ShutdownHandle = token.clone().into();

    let mut app = App::new()
        .with_shutdown_signal(handle)
        .bind(format!("127.0.0.1:{port}"))
        .without_greeter();
    app.map_get("/ping", || async { ok!("pong") });
    let task = tokio::spawn(async move { app.run().await });

    wait_until_listening(port).await;

    token.cancel();

    tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .expect("server did not exit after cancel on outer token")
        .expect("server task panicked")
        .expect("server returned an error");
}

#[tokio::test]
async fn shutdown_drains_in_flight_requests() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    let port = pick_free_port();
    let (app, handle) = App::with_shutdown();
    let started = Arc::new(AtomicBool::new(false));
    let started_for_handler = Arc::clone(&started);

    let mut app = app.bind(format!("127.0.0.1:{port}")).without_greeter();
    app.map_get("/slow", move || {
        let started = Arc::clone(&started_for_handler);
        async move {
            started.store(true, Ordering::SeqCst);
            tokio::time::sleep(Duration::from_millis(300)).await;
            ok!("done")
        }
    });
    app.map_get("/ping", || async { ok!("pong") });
    let task = tokio::spawn(async move { app.run().await });

    wait_until_listening(port).await;

    let request = tokio::spawn(async move {
        reqwest::get(format!("http://127.0.0.1:{port}/slow"))
            .await
            .unwrap()
    });

    // Wait until the slow handler is actually executing.
    let started_deadline = Instant::now() + Duration::from_secs(5);
    while !started.load(Ordering::SeqCst) {
        assert!(
            Instant::now() < started_deadline,
            "/slow handler did not start within 5s"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    handle.shutdown();

    let response = tokio::time::timeout(Duration::from_secs(5), request)
        .await
        .expect("in-flight request did not finish")
        .expect("request task panicked");
    assert!(response.status().is_success());
    assert_eq!(response.text().await.unwrap(), "done");

    tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .expect("server did not exit after drain")
        .expect("server task panicked")
        .expect("server returned an error");
}
