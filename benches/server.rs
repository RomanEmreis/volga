use volga::{status, App};
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use futures_util::future::join_all;
use reqwest::Client;
use tokio::{runtime::Runtime, time::Instant};

use std::{
    io::{Error, ErrorKind},
    time::Duration
};

async fn routing(iters: u64, url: &str) -> Duration {
    #[cfg(all(feature = "http1", not(feature = "http2")))]
    let client = Client::builder().http1_only().build().unwrap();
    
    #[cfg(feature = "http2")]
    let client = Client::builder().http2_prior_knowledge().build().unwrap();

    let url = format!("http://localhost:7878{}", url);
    
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
            let mut app = App::new();
            app.map_get("/", || async { "Hello, World!" });
            app.map_get("/err", || async { Error::new(ErrorKind::Other, "error") });
            app.map_err(|err| async move { status!(500, err.to_string()) });
            _ = app.run().await;
        });
    });
    
    c.bench_function("ok", |b| b.iter_custom(
        |iters| rt.block_on(routing(iters, black_box("/")))
    ));
    c.bench_function("err", |b| b.iter_custom(
        |iters| rt.block_on(routing(iters, black_box("/err")))
    ));
}

criterion_group!(benches, benchmark);
criterion_main!(benches);