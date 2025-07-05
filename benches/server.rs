use volga::{status, App};

#[cfg(feature = "di")]
use volga::di::Dc;

use std::hint::black_box;
use criterion::{criterion_group, criterion_main, Criterion};
use futures_util::future::join_all;
use reqwest::Client;
use tokio::{runtime::Runtime, time::Instant};

use std::{
    io::Error,
    time::Duration
};

#[cfg(feature = "di")]
use std::sync::{Arc, RwLock};

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
            let mut app = App::new().without_body_limit();
            
            #[cfg(feature = "di")]
            let mut app = {
                app.add_singleton(Counter::default());
                app.add_scoped::<Cache>();
                app.add_transient::<Transient>();
                app.map_post("/singleton", |c: Dc<Counter>| async move { *c.0.write().unwrap() += 1; });
                app.map_post("/scoped", |c: Dc<Cache>| async move { c.0.write().unwrap().push(1); });
                app.map_put("/transient", |c: Dc<Transient>| async move { let _ = c; });
                app
            };
            
            #[cfg(feature = "middleware")]
            let mut app = {
                app.map_get("/valid", || async {}).filter(|| async { true });
                app.map_get("/invalid", || async {}).filter(|| async { false });
                app
            };
            
            app.map_get("/", || async { "Hello, World!" });
            app.map_get("/err", || async { Error::other("error") });
            app.map_err(|err| async move { status!(500, err.to_string()) });
            app.map_fallback(|| async { status!(404) });
            _ = app.run().await;
        });
    });
    
    c.bench_function("ok", |b| b.iter_custom(
        |iters| rt.block_on(routing(iters, black_box("/")))
    ));
    c.bench_function("err", |b| b.iter_custom(
        |iters| rt.block_on(routing(iters, black_box("/err")))
    ));
    c.bench_function("fallback", |b| b.iter_custom(
        |iters| rt.block_on(routing(iters, black_box("/fall")))
    ));
    
    #[cfg(feature = "di")]
    {
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
    
    #[cfg(feature = "middleware")]
    {
        c.bench_function("valid filter", |b| b.iter_custom(
            |iters| rt.block_on(routing(iters, black_box("/valid")))
        ));
        c.bench_function("invalid filter", |b| b.iter_custom(
            |iters| rt.block_on(routing(iters, black_box("/invalid")))
        ));
    }
}

criterion_group!(benches, benchmark);
criterion_main!(benches);

#[cfg(feature = "di")]
#[derive(Default, Clone, Debug)]
struct Counter(Arc<RwLock<i32>>);

#[cfg(feature = "di")]
#[derive(Default, Clone, Debug)]
struct Cache(Arc<RwLock<Vec<i32>>>);

#[cfg(feature = "di")]
#[derive(Default, Clone, Debug)]
struct Transient;