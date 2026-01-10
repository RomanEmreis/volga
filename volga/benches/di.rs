#![allow(missing_docs)]

use volga::App;

use volga::di::Dc;

use std::hint::black_box;
use criterion::{criterion_group, criterion_main, Criterion};
use futures_util::future::join_all;
use reqwest::Client;
use tokio::{runtime::Runtime, time::Instant};

use std::{
    sync::{Arc, RwLock},
    time::Duration
};

async fn routing(iters: u64, url: &str) -> Duration {
    #[cfg(all(feature = "http1", not(feature = "http2")))]
    let client = Client::builder().http1_only().build().unwrap();
    #[cfg(feature = "http2")]
    let client = Client::builder().http2_prior_knowledge().build().unwrap();

    let url = format!("http://localhost:7878{url}");

    let start = Instant::now();

    let requests = (0..iters).map(|_| client.get(&url).send());
    let responses = join_all(requests).await;

    let elapsed = start.elapsed();

    let failed = responses.iter().filter(|r| r.is_err()).count();
    if failed > 0 {
        eprintln!("failed {failed} requests");
    };
    elapsed
}

fn benchmark(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        tokio::spawn(async {
            let mut app = App::new()
                .with_no_delay()
                .without_body_limit()
                .without_implicit_head()
                .without_greeter();

            app.add_singleton(Counter::default());
            app.add_scoped_default::<Cache>();
            app.add_transient_default::<Transient>();
            app.map_post("/singleton", |c: Dc<Counter>| async move { *c.0.write().unwrap() += 1; });
            app.map_post("/scoped", |c: Dc<Cache>| async move { c.0.write().unwrap().push(1); });
            app.map_put("/transient", |c: Dc<Transient>| async move { let _ = c; });

            _ = app.run().await;
        });
    });

    c.bench_function("singleton", |b| b.iter_custom(
        |iters| rt.block_on(routing(iters, black_box("/singleton")))
    ));
    c.bench_function("scoped", |b| b.iter_custom(
        |iters| rt.block_on(routing(iters, black_box("/scoped")))
    ));
    c.bench_function("transient", |b| b.iter_custom(
        |iters| rt.block_on(routing(iters, black_box("/transient")))
    ));
}

criterion_group!(benches, benchmark);
criterion_main!(benches);

#[derive(Default, Clone, Debug)]
struct Counter(Arc<RwLock<i32>>);

#[derive(Default, Clone, Debug)]
struct Cache(Arc<RwLock<Vec<i32>>>);

#[derive(Default, Clone, Debug)]
struct Transient;