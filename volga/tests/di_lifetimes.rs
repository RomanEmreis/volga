#![allow(missing_docs)]
#![cfg(all(feature = "di", feature = "test"))]

//! What the server does with the lifetimes of the services it hands out: a scope per
//! request, and singletons released when the server stops.

use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
use volga::{di::Dc, test::TestServer};

static NEXT_ID: AtomicUsize = AtomicUsize::new(0);

struct RequestId(usize);

#[tokio::test]
async fn it_gives_every_request_on_a_connection_a_scope_of_its_own() {
    let server = TestServer::spawn(|app| {
        app.add_scoped_factory(|| RequestId(NEXT_ID.fetch_add(1, Ordering::SeqCst)));
        app.map_get("/", |a: Dc<RequestId>, b: Dc<RequestId>| async move {
            format!("{}-{}", a.0, b.0)
        });
    })
    .await;

    // One client, so the two requests go over the same pooled keep-alive connection
    let client = server.client();
    let mut ids = Vec::new();
    for _ in 0..2 {
        let body = client
            .get(server.url("/"))
            .send()
            .await
            .expect("the request failed")
            .text()
            .await
            .expect("no body");
        let (a, b) = body.split_once('-').expect("two ids");
        // Within a request, both arguments reach the one scoped instance
        assert_eq!(a, b, "{body}");
        ids.push(a.to_string());
    }
    // Across requests, even on one connection, they do not
    assert_ne!(ids[0], ids[1]);

    server.shutdown().await;
}

struct Flush(Arc<AtomicBool>);

impl Drop for Flush {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

#[tokio::test]
async fn it_drops_singletons_when_the_server_shuts_down() {
    let dropped = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&dropped);
    let server = TestServer::spawn(move |app| {
        app.add_singleton(Flush(flag));
        app.map_get("/", |_: Dc<Flush>| async { "ok" });
    })
    .await;

    let response = server
        .client()
        .get(server.url("/"))
        .send()
        .await
        .expect("the request failed");
    assert_eq!(response.text().await.expect("no body"), "ok");
    assert!(!dropped.load(Ordering::SeqCst));

    // Stopping the server releases its registrations, and the singletons with them: a
    // singleton that flushes or closes something on drop gets to do so
    server.shutdown().await;
    assert!(dropped.load(Ordering::SeqCst));
}
