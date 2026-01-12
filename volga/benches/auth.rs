#![allow(missing_docs)]

use volga::{App, claims};

use std::hint::black_box;
use criterion::{criterion_group, criterion_main, Criterion};
use futures_util::future::join_all;
use reqwest::Client;
use tokio::{runtime::Runtime, time::Instant};

use std::time::Duration;
use jsonwebtoken::{DecodingKey, EncodingKey};
use serde::{Deserialize, Serialize};
use volga::auth::roles;

const TEST_TOKEN: &str = "eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJlbWFpbC5jb20iLCJjb21wYW55IjoiQXdlc29tZSBDby4iLCJyb2xlIjoiYWRtaW4iLCJleHAiOjE3NTMwMDEzODl9.5g6aE4KpRobZqsS8WFJndbHYakG6GydPtIpKt3L1X5o";

async fn routing(iters: u64, url: &str, token: &str) -> Duration {
    #[cfg(all(feature = "http1", not(feature = "http2")))]
    let client = Client::builder().http1_only().build().unwrap();
    #[cfg(feature = "http2")]
    let client = Client::builder().http2_prior_knowledge().build().unwrap();

    let url = format!("http://localhost:7878{url}");

    let start = Instant::now();

    let requests = (0..iters).map(|_| client.get(&url)
        .header(volga::headers::AUTHORIZATION, format!("Bearer {token}"))
        .send());
    
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
                .without_greeter()
                .with_bearer_auth(|auth| auth
                    .validate_exp(false)
                    .set_encoding_key(EncodingKey::from_secret(b"test secret"))
                    .set_decoding_key(DecodingKey::from_secret(b"test secret")));

            app.map_get("/protected", || async { "Hello, World!" })
                .authorize::<Claims>(roles(["admin", "user"]));
            
            _ = app.run().await;
        });
    });

    c.bench_function("unauthorized", |b| b.iter_custom(
        |iters| rt.block_on(routing(iters, black_box("/protected"), "invalid"))
    ));

    c.bench_function("protected", |b| b.iter_custom(
        |iters| rt.block_on(routing(iters, black_box("/protected"), TEST_TOKEN))
    ));
}

criterion_group!(benches, benchmark);
criterion_main!(benches);

claims! {
    #[derive(Clone, Serialize, Deserialize)]
    struct Claims {
        sub: String,
        company: String,
        role: String,
        permissions: Vec<String>,
        exp: u64,
    }
}