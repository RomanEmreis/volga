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

/// A `reqwest::Client` with proxies disabled, so localhost probes are
/// not redirected by HTTP(S)_PROXY env vars set in the test environment.
///
/// A server built without `http1` speaks only HTTP/2, which over cleartext
/// takes prior knowledge - an HTTP/1.1 probe never gets an answer from it.
fn local_client() -> reqwest::Client {
    let builder = reqwest::Client::builder().no_proxy();
    #[cfg(not(feature = "http1"))]
    let builder = builder.http2_prior_knowledge();
    builder.build().expect("failed to build reqwest client")
}

async fn wait_until_listening(client: &reqwest::Client, port: u16) {
    let deadline = Instant::now() + Duration::from_secs(5);
    let url = format!("http://127.0.0.1:{port}/ping");
    while Instant::now() < deadline {
        if client.get(&url).send().await.is_ok() {
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

    let client = local_client();
    wait_until_listening(&client, port).await;

    let response = client
        .get(format!("http://127.0.0.1:{port}/ping"))
        .send()
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

    wait_until_listening(&local_client(), port).await;

    handle.shutdown();
    handle.shutdown(); // second call must be a no-op

    let result = tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .expect("server did not exit after shutdown")
        .expect("server task panicked");
    result.expect("server returned an error");
}

#[tokio::test]
async fn shutdown_on_drives_server_shutdown() {
    let port = pick_free_port();
    let (signal_tx, signal_rx) = tokio::sync::oneshot::channel::<()>();

    let mut app = App::new()
        .bind(format!("127.0.0.1:{port}"))
        .without_greeter()
        .shutdown_on(async move {
            let _ = signal_rx.await;
        });
    app.map_get("/ping", || async { ok!("pong") });
    let task = tokio::spawn(async move { app.run().await });

    wait_until_listening(&local_client(), port).await;

    signal_tx.send(()).unwrap();

    tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .expect("server did not exit after shutdown_on trigger")
        .expect("server task panicked")
        .expect("server returned an error");
}

#[tokio::test]
async fn shutdown_on_chained_triggers_compose() {
    let port = pick_free_port();
    let (tx_a, rx_a) = tokio::sync::oneshot::channel::<()>();
    let (_tx_b, rx_b) = tokio::sync::oneshot::channel::<()>();

    let mut app = App::new()
        .bind(format!("127.0.0.1:{port}"))
        .without_greeter()
        .shutdown_on(async move {
            let _ = rx_a.await;
        })
        .shutdown_on(async move {
            let _ = rx_b.await;
        });
    app.map_get("/ping", || async { ok!("pong") });
    let task = tokio::spawn(async move { app.run().await });

    wait_until_listening(&local_client(), port).await;

    // Firing only the first trigger is enough.
    tx_a.send(()).unwrap();

    tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .expect("server did not exit after first trigger")
        .expect("server task panicked")
        .expect("server returned an error");
}

#[tokio::test]
async fn shutdown_on_remaining_triggers_release_after_shutdown() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    let port = pick_free_port();
    let (tx_a, rx_a) = tokio::sync::oneshot::channel::<()>();
    // The second trigger never resolves on its own - it's a watchdog future.
    // The trigger task wraps it in a `select!` against the shared token,
    // so when trigger A cancels, this future is *dropped*, which fires
    // `dropped` via the `Drop` impl below.
    let dropped = Arc::new(AtomicBool::new(false));
    let dropped_for_future = Arc::clone(&dropped);

    struct DropFlag(Arc<AtomicBool>);
    impl Drop for DropFlag {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }

    let watchdog = async move {
        let _flag = DropFlag(dropped_for_future);
        std::future::pending::<()>().await;
    };

    let mut app = App::new()
        .bind(format!("127.0.0.1:{port}"))
        .without_greeter()
        .shutdown_on(async move {
            let _ = rx_a.await;
        })
        .shutdown_on(watchdog);
    app.map_get("/ping", || async { ok!("pong") });
    let task = tokio::spawn(async move { app.run().await });

    wait_until_listening(&local_client(), port).await;

    tx_a.send(()).unwrap();

    tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .expect("server did not exit after trigger")
        .expect("server task panicked")
        .expect("server returned an error");

    // Give the task scheduler a tick to drop the unresolved trigger.
    let deadline = Instant::now() + Duration::from_secs(2);
    while !dropped.load(Ordering::SeqCst) {
        assert!(
            Instant::now() < deadline,
            "remaining shutdown_on trigger was not dropped after shutdown"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

#[tokio::test]
async fn shutdown_on_composes_with_with_shutdown_handle() {
    let port = pick_free_port();
    let (signal_tx, signal_rx) = tokio::sync::oneshot::channel::<()>();
    let (app, handle) = App::with_shutdown();

    let mut app = app
        .bind(format!("127.0.0.1:{port}"))
        .without_greeter()
        .shutdown_on(async move {
            let _ = signal_rx.await;
        });
    app.map_get("/ping", || async { ok!("pong") });
    let task = tokio::spawn(async move { app.run().await });

    wait_until_listening(&local_client(), port).await;

    // Firing the trigger should cancel the handle's shared token.
    signal_tx.send(()).unwrap();

    tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .expect("server did not exit after shutdown_on trigger")
        .expect("server task panicked")
        .expect("server returned an error");

    assert!(handle.is_shutdown_requested());
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

    wait_until_listening(&local_client(), port).await;

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

    let client = local_client();
    wait_until_listening(&client, port).await;

    let request = tokio::spawn(async move {
        client
            .get(format!("http://127.0.0.1:{port}/slow"))
            .send()
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

/// A request still running when the accept loop breaks holds the app environment for as long
/// as its handler runs. `run` must wait for its connection all the same: returning earlier
/// lets whoever owns the runtime drop it under a response that is still being written
#[tokio::test]
async fn run_does_not_return_before_a_request_in_flight_is_answered() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    let port = pick_free_port();
    let (app, handle) = App::with_shutdown();
    let answered = Arc::new(AtomicBool::new(false));
    let answered_for_handler = Arc::clone(&answered);

    let mut app = app.bind(format!("127.0.0.1:{port}")).without_greeter();
    app.map_get("/ping", || async { ok!("pong") });
    app.map_get("/slow", move || {
        let handle = handle.clone();
        let answered = Arc::clone(&answered_for_handler);
        async move {
            // Shutting down from inside the handler makes the accept loop break while this
            // request is certainly still in flight - no timing involved
            handle.shutdown();
            tokio::time::sleep(Duration::from_millis(300)).await;
            answered.store(true, Ordering::SeqCst);
            ok!("done")
        }
    });
    let task = tokio::spawn(async move { app.run().await });

    let client = local_client();
    wait_until_listening(&client, port).await;

    let request = tokio::spawn(async move {
        client
            .get(format!("http://127.0.0.1:{port}/slow"))
            .send()
            .await
            .unwrap()
    });

    tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .expect("server did not exit after drain")
        .expect("server task panicked")
        .expect("server returned an error");

    assert!(
        answered.load(Ordering::SeqCst),
        "run returned while a request was still in flight"
    );

    let response = tokio::time::timeout(Duration::from_secs(5), request)
        .await
        .expect("in-flight request did not finish")
        .expect("request task panicked");
    assert!(response.status().is_success());
    assert_eq!(response.text().await.unwrap(), "done");
}

/// Counts the SSE events a client receives, and records when the response ends.
struct EventCounter {
    events: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    ended: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl EventCounter {
    async fn open(client: &reqwest::Client, url: String) -> Self {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

        let mut response = client.get(url).send().await.unwrap();
        let events = Arc::new(AtomicUsize::new(0));
        let ended = Arc::new(AtomicBool::new(false));
        let (e, d) = (Arc::clone(&events), Arc::clone(&ended));
        tokio::spawn(async move {
            while let Ok(Some(chunk)) = response.chunk().await {
                let count = String::from_utf8_lossy(&chunk).matches("data:").count();
                e.fetch_add(count, Ordering::SeqCst);
            }
            d.store(true, Ordering::SeqCst);
        });
        Self { events, ended }
    }

    fn events(&self) -> usize {
        self.events.load(std::sync::atomic::Ordering::SeqCst)
    }

    fn ended(&self) -> bool {
        self.ended.load(std::sync::atomic::Ordering::SeqCst)
    }

    async fn wait_for_events(&self, n: usize) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while self.events() < n {
            assert!(Instant::now() < deadline, "the stream sent no events");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    async fn wait_until_ended(&self, within: Duration) {
        let deadline = Instant::now() + within;
        while !self.ended() {
            assert!(Instant::now() < deadline, "the stream did not end");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }
}

fn endless_sse() -> volga::http::sse::SseStream<
    impl futures_util::Stream<Item = Result<volga::http::sse::Message, volga::error::Error>>
    + Send
    + 'static,
> {
    volga::sse_stream! {
        loop {
            yield volga::http::sse::Message::new().data("tick");
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }
}

/// A stream that never ends holds its connection open through the whole shutdown. Once the
/// timeout runs out the connection has to be closed, not only stop being waited for:
/// otherwise it keeps serving past `run` for as long as the runtime lives (#254)
#[tokio::test]
async fn shutdown_timeout_closes_a_stream_that_never_ends() {
    let port = pick_free_port();
    let (app, handle) = App::with_shutdown();
    let mut app = app
        .bind(format!("127.0.0.1:{port}"))
        .without_greeter()
        .with_shutdown_timeout(Duration::from_millis(300));
    app.map_get("/ping", || async { ok!("pong") });
    app.map_get("/events", || async { endless_sse() });
    let task = tokio::spawn(async move { app.run().await });

    let client = local_client();
    wait_until_listening(&client, port).await;

    let stream = EventCounter::open(&client, format!("http://127.0.0.1:{port}/events")).await;
    stream.wait_for_events(1).await;

    let started = Instant::now();
    handle.shutdown();
    tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .expect("server did not exit after the shutdown timeout")
        .expect("server task panicked")
        .expect("server returned an error");
    let elapsed = started.elapsed();

    assert!(
        elapsed >= Duration::from_millis(300),
        "run returned before the shutdown timeout: {elapsed:?}"
    );
    assert!(
        elapsed < Duration::from_secs(3),
        "run returned long after the shutdown timeout: {elapsed:?}"
    );

    stream.wait_until_ended(Duration::from_secs(2)).await;
    let at_end = stream.events();
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(
        stream.events(),
        at_end,
        "the stream kept sending after run returned"
    );
}

/// A stream that waits on the extracted `ShutdownHandle` ends as soon as the shutdown starts,
/// so the shutdown does not wait out its timeout
#[tokio::test]
async fn a_stream_ended_by_the_shutdown_handle_does_not_hold_up_the_shutdown() {
    use futures_util::StreamExt;
    use volga::{ShutdownHandle, http::sse::SseStream};

    let port = pick_free_port();
    let (app, handle) = App::with_shutdown();
    let mut app = app.bind(format!("127.0.0.1:{port}")).without_greeter();
    app.map_get("/ping", || async { ok!("pong") });
    app.map_get("/events", |shutdown: ShutdownHandle| async move {
        SseStream::new(endless_sse().take_until(shutdown.cancelled()))
    });
    let task = tokio::spawn(async move { app.run().await });

    let client = local_client();
    wait_until_listening(&client, port).await;

    let stream = EventCounter::open(&client, format!("http://127.0.0.1:{port}/events")).await;
    stream.wait_for_events(1).await;

    let started = Instant::now();
    handle.shutdown();
    tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .expect("server did not exit")
        .expect("server task panicked")
        .expect("server returned an error");

    let elapsed = started.elapsed();
    assert!(
        elapsed < Duration::from_secs(2),
        "the shutdown waited on a stream that should have ended: {elapsed:?}"
    );
    stream.wait_until_ended(Duration::from_secs(2)).await;
}

/// The handle is extractable from an app that was not given one, and it is the one the
/// server shuts down on
#[tokio::test]
async fn shutdown_handle_is_extractable_without_with_shutdown() {
    use volga::ShutdownHandle;

    let port = pick_free_port();
    let mut app = App::new()
        .bind(format!("127.0.0.1:{port}"))
        .without_greeter();
    app.map_get("/ping", || async { ok!("pong") });
    app.map_post("/stop", |shutdown: ShutdownHandle| async move {
        shutdown.shutdown();
        ok!("stopping")
    });
    let task = tokio::spawn(async move { app.run().await });

    let client = local_client();
    wait_until_listening(&client, port).await;

    let response = client
        .post(format!("http://127.0.0.1:{port}/stop"))
        .send()
        .await
        .unwrap();
    assert!(response.status().is_success());

    tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .expect("server did not exit")
        .expect("server task panicked")
        .expect("server returned an error");
}

/// A shutdown that has just started still answers requests in flight, so it does not cancel
/// their `CancellationToken` - a handler honouring it would fail them for no reason
#[tokio::test]
async fn shutdown_start_does_not_cancel_the_request_token() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use volga::CancellationToken;

    let port = pick_free_port();
    let (app, handle) = App::with_shutdown();
    let started = Arc::new(AtomicBool::new(false));
    let started_for_handler = Arc::clone(&started);

    let mut app = app.bind(format!("127.0.0.1:{port}")).without_greeter();
    app.map_get("/ping", || async { ok!("pong") });
    app.map_get("/slow", move |token: CancellationToken| {
        let started = Arc::clone(&started_for_handler);
        async move {
            started.store(true, Ordering::SeqCst);
            tokio::time::sleep(Duration::from_millis(300)).await;
            ok!("cancelled: {}", token.is_cancelled())
        }
    });
    let task = tokio::spawn(async move { app.run().await });

    let client = local_client();
    wait_until_listening(&client, port).await;

    let request = {
        let client = client.clone();
        tokio::spawn(async move {
            client
                .get(format!("http://127.0.0.1:{port}/slow"))
                .send()
                .await
                .unwrap()
        })
    };

    let deadline = Instant::now() + Duration::from_secs(5);
    while !started.load(Ordering::SeqCst) {
        assert!(Instant::now() < deadline, "/slow handler did not start");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    handle.shutdown();

    let response = request.await.unwrap();
    assert_eq!(response.text().await.unwrap(), "cancelled: false");

    tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .expect("server did not exit")
        .expect("server task panicked")
        .expect("server returned an error");
}

/// When the shutdown runs out of time, the request's `CancellationToken` is cancelled, so
/// work a handler handed off with it learns that its connection is gone
#[tokio::test]
async fn shutdown_timeout_cancels_the_request_token() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use volga::CancellationToken;

    let port = pick_free_port();
    let (app, handle) = App::with_shutdown();
    let started = Arc::new(AtomicBool::new(false));
    let cancelled = Arc::new(AtomicBool::new(false));
    let (started_for_handler, cancelled_for_handler) =
        (Arc::clone(&started), Arc::clone(&cancelled));

    let mut app = app
        .bind(format!("127.0.0.1:{port}"))
        .without_greeter()
        .with_shutdown_timeout(Duration::from_millis(200));
    app.map_get("/ping", || async { ok!("pong") });
    app.map_get("/stuck", move |token: CancellationToken| {
        let started = Arc::clone(&started_for_handler);
        let cancelled = Arc::clone(&cancelled_for_handler);
        async move {
            tokio::spawn(async move {
                token.cancelled().await;
                cancelled.store(true, Ordering::SeqCst);
            });
            started.store(true, Ordering::SeqCst);
            std::future::pending::<()>().await;
            ok!()
        }
    });
    let task = tokio::spawn(async move { app.run().await });

    let client = local_client();
    wait_until_listening(&client, port).await;

    let request = {
        let client = client.clone();
        tokio::spawn(async move {
            client
                .get(format!("http://127.0.0.1:{port}/stuck"))
                .send()
                .await
        })
    };

    let deadline = Instant::now() + Duration::from_secs(5);
    while !started.load(Ordering::SeqCst) {
        assert!(Instant::now() < deadline, "/stuck handler did not start");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    handle.shutdown();

    tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .expect("server did not exit after the shutdown timeout")
        .expect("server task panicked")
        .expect("server returned an error");

    let deadline = Instant::now() + Duration::from_secs(2);
    while !cancelled.load(Ordering::SeqCst) {
        assert!(
            Instant::now() < deadline,
            "the request token was not cancelled"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    // The connection was closed under the request, so it gets no response
    let result = tokio::time::timeout(Duration::from_secs(2), request)
        .await
        .expect("the client was left waiting")
        .unwrap();
    assert!(result.is_err());
}
