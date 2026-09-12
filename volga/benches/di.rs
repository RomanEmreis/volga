#![allow(missing_docs)]

mod common;

use common::{BODY, Harness, Profile};
use criterion::{Criterion, criterion_group, criterion_main};
use std::sync::{Arc, RwLock};
use volga::di::Dc;

fn benchmark(c: &mut Criterion) {
    let app = Harness::new(routes);

    let baseline = Harness::baseline();

    let mut group = c.benchmark_group("di");
    group.bench_function("bare hyper", |b| baseline.get_saturated(b, "/", 200));
    group.bench_function("no di", |b| post(&app, b, "/plain"));
    group.bench_function("singleton", |b| post(&app, b, "/singleton"));
    group.bench_function("scoped", |b| post(&app, b, "/scoped"));
    group.bench_function("transient", |b| post(&app, b, "/transient"));
    group.finish();

    // Several workers taking requests at once. A write every request makes to state all of
    // them share - the request scope's handle on the registrations among others - only costs
    // anything once workers collide on it, which a single worker never does. The routes here
    // only read, so what they add is the framework's shared writes rather than the route's.
    // Whether that shows depends on the load: this in-process reqwest client tops out near 165k
    // requests a second, far below what the server serves, so the workers here rarely collide.
    // An external load generator driving the server hard is what makes them.
    let multi = Harness::with_profile(Profile::MULTI, |app| app, routes);
    let multi_baseline = Harness::baseline_with(Profile::MULTI);

    let mut group = c.benchmark_group("di, 8 server workers");
    group.bench_function("bare hyper", |b| multi_baseline.get_saturated(b, "/", 200));
    group.bench_function("no di", |b| post(&multi, b, "/plain"));
    group.bench_function("singleton, read only", |b| post(&multi, b, "/settings"));
    group.finish();
}

fn routes(app: &mut volga::App) {
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
    app.add_singleton(Settings { greeting: BODY });
    app.map_post("/settings", |s: Dc<Settings>| async move { s.greeting });
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

#[derive(Clone, Debug)]
struct Settings {
    greeting: &'static str,
}
