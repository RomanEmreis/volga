#![allow(missing_docs)]
#![cfg(all(feature = "di", feature = "tracing"))]

//! The greeter and the `listening on` line are what an operator - and a readiness check
//! tailing the log - read as "the server is up", so an application that refuses to start
//! must not say either, and one that starts must still say both.
//!
//! This suite has a file of its own because it captures the global tracing subscriber: a
//! server started by a neighbouring test would write into the same capture.

use std::{
    io,
    net::TcpListener,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use volga::{App, di::Dc};

struct Unregistered;
struct A;

/// A tracing writer that keeps what was written
#[derive(Clone, Default)]
struct Capture(Arc<Mutex<Vec<u8>>>);

impl Capture {
    fn text(&self) -> String {
        let logged = self.0.lock().expect("the capture lock is poisoned");
        String::from_utf8_lossy(&logged).into_owned()
    }
}

impl io::Write for Capture {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0
            .lock()
            .expect("the capture lock is poisoned")
            .extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Capture {
    type Writer = Self;

    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

#[tokio::test]
async fn it_announces_only_a_server_that_started() {
    let capture = Capture::default();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(capture.clone())
        .with_max_level(tracing::Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber).expect("the subscriber is set once");

    // A graph that does not resolve: the app says nothing about listening
    let listener = TcpListener::bind("127.0.0.1:0").expect("failed to bind");
    let mut app = App::new().without_greeter();
    app.add_scoped_factory(|_: Dc<Unregistered>| Ok(A));

    let err = tokio::time::timeout(Duration::from_secs(5), app.run_with_std_listener(listener))
        .await
        .expect("the app started and kept serving")
        .expect_err("the app started");

    assert!(err.to_string().contains("not registered"), "{err}");
    assert!(
        !capture.text().contains("listening on"),
        "a start that failed announced a listening server: {}",
        capture.text()
    );

    // One that resolves still announces itself
    let listener = TcpListener::bind("127.0.0.1:0").expect("failed to bind");
    let (app, handle) = App::with_shutdown();
    let app = app.without_greeter();
    let server = tokio::spawn(async move { app.run_with_std_listener(listener).await });

    let deadline = Instant::now() + Duration::from_secs(5);
    while !capture.text().contains("listening on") {
        assert!(
            Instant::now() < deadline,
            "a server that started announced nothing"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    handle.shutdown();
    server
        .await
        .expect("the server task panicked")
        .expect("the server returned an error");
}
