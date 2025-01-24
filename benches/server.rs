use volga::App;

use criterion::{criterion_group, criterion_main, Criterion};
use futures_util::future::join_all;
use reqwest::Client;
use std::time::Duration;
use tokio::{runtime::Runtime, time::Instant};

async fn routing(iters: u64) -> Duration {
    #[cfg(all(feature = "http1", not(feature = "http2")))]
    let client = Client::builder().http1_only().build().unwrap();
    
    #[cfg(feature = "http2")]
    let client = Client::builder().http2_prior_knowledge().build().unwrap();

    let start = Instant::now();
    
    let requests = (0..iters).map(|_| client.get("http://localhost:7878/").send());
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
            _ = app.run().await;
        });
    });
    
    c.bench_function("parallel requests", |b| b.iter_custom(|iters| rt.block_on(routing(iters))));
}

criterion_group!(benches, benchmark);
criterion_main!(benches);