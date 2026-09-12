#![allow(missing_docs)]

mod common;

use common::{BODY, Harness};
use criterion::{Bencher, Criterion, criterion_group, criterion_main};
use jsonwebtoken::{EncodingKey as JwtEncodingKey, Header, encode};
use serde::{Deserialize, Serialize};
use volga::{
    auth::{DecodingKey, EncodingKey, roles},
    claims,
};

const SECRET: &[u8] = b"test secret";

/// Mints a token that actually satisfies `Claims` and the role check.
///
/// A hardcoded token is easy to get subtly wrong - one missing claim and every
/// "authorized" request quietly takes the 403 path, benchmarking the rejection
/// instead of the success path.
fn valid_token() -> String {
    let claims = Claims {
        sub: "email.com".into(),
        company: "Awesome Co.".into(),
        role: "admin".into(),
        permissions: vec!["read".into(), "write".into()],
        exp: 1_753_001_389,
    };
    encode(
        &Header::default(),
        &claims,
        &JwtEncodingKey::from_secret(SECRET),
    )
    .expect("failed to sign the benchmark token")
}

fn benchmark(c: &mut Criterion) {
    let app = Harness::with_config(
        |app| {
            app.with_bearer_auth(|auth| {
                auth.validate_exp(false)
                    .set_encoding_key(EncodingKey::from_secret(SECRET))
                    .set_decoding_key(DecodingKey::from_secret(SECRET))
            })
        },
        |app| {
            // Control: same route shape and response, no authorization.
            app.map_get("/open", || async { BODY });

            app.map_get("/protected", || async { BODY })
                .authorize::<Claims>(roles(["admin", "user"]));
        },
    );

    let baseline = Harness::baseline();
    let token = valid_token();

    let mut group = c.benchmark_group("auth");
    group.bench_function("bare hyper", |b| baseline.get_saturated(b, "/", 200));
    group.bench_function("no auth", |b| bearer(&app, b, "/open", &token, 200));
    group.bench_function("authorized", |b| bearer(&app, b, "/protected", &token, 200));
    group.bench_function("malformed token", |b| {
        bearer(&app, b, "/protected", "invalid", 401)
    });
    group.bench_function("missing token", |b| {
        let url = app.url("/protected");
        app.run_saturated(b, 401, || app.client().get(&url));
    });
    group.finish();
}

fn bearer(app: &Harness, b: &mut Bencher<'_>, path: &str, token: &str, expect: u16) {
    let url = app.url(path);
    let header = format!("Bearer {token}");
    app.run_saturated(b, expect, || {
        app.client()
            .get(&url)
            .header(volga::headers::AUTHORIZATION, &header)
    });
}

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

criterion_group!(benches, benchmark);
criterion_main!(benches);
