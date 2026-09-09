#![allow(missing_docs)]

mod common;

use common::{BODY, Harness};
use criterion::{Criterion, criterion_group, criterion_main};
use std::sync::{Arc, RwLock};
use volga::di::Dc;

fn benchmark(c: &mut Criterion) {
    let app = Harness::new(|app| {
        app.add_singleton(Counter::default());
        app.add_scoped_default::<Cache>();
        app.add_transient_default::<Transient>();

        // Control: the same route shape and response, resolving nothing.
        app.map_post("/plain", || async { BODY });

        app.map_post("/singleton", |c: Dc<Counter>| async move {
            *c.0.write().expect("poisoned") += 1;
            BODY
        });
        app.map_post("/scoped", |c: Dc<Cache>| async move {
            c.0.write().expect("poisoned").push(1);
            BODY
        });
        app.map_post("/transient", |c: Dc<Transient>| async move {
            let _ = c;
            BODY
        });
    });

    let baseline = Harness::baseline();

    let mut group = c.benchmark_group("di");
    group.bench_function("bare hyper", |b| baseline.get_saturated(b, "/", 200));
    group.bench_function("no di", |b| post(&app, b, "/plain"));
    group.bench_function("singleton", |b| post(&app, b, "/singleton"));
    group.bench_function("scoped", |b| post(&app, b, "/scoped"));
    group.bench_function("transient", |b| post(&app, b, "/transient"));
    group.finish();
}

fn post(app: &Harness, b: &mut criterion::Bencher<'_>, path: &str) {
    let url = app.url(path);
    app.run_saturated(b, 200, || app.client().post(&url));
}

criterion_group!(benches, benchmark);
criterion_main!(benches);

#[derive(Default, Clone, Debug)]
struct Counter(Arc<RwLock<i32>>);

#[derive(Default, Clone, Debug)]
struct Cache(Arc<RwLock<Vec<i32>>>);

#[derive(Default, Clone, Debug)]
struct Transient;
