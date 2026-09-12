#![allow(missing_docs)]

mod common;

use common::{BODY, Harness};
use criterion::{Criterion, criterion_group, criterion_main};
use std::io::Error;
use volga::status;

fn benchmark(c: &mut Criterion) {
    let app = Harness::new(|app| {
        // Each route shape lives in its own subtree. Mixing them (a `{param}`
        // next to a static sibling that shares its first segment) does not just
        // muddy the comparison - it changes which route matches.
        app.map_get("/", || async { BODY });
        app.map_get("/s", || async { BODY });
        app.map_get("/s/a/b/c", || async { BODY });
        app.map_get("/s/a/b/c/d/e/f", || async { BODY });
        app.map_get("/p/{a}", |_a: String| async { BODY });
        app.map_get("/q/{a}/{b}", |_a: i32, _b: String| async { BODY });

        // A literal sitting where a parameter also sits: `/m/lit` has to walk into
        // `lit`, find nothing mapped there, and come back out to read `{a}`.
        app.map_get("/m/lit/deep", || async { BODY });
        app.map_get("/m/{a}", |_a: String| async { BODY });

        app.map_put("/put", || async { BODY });
        app.map_get("/empty", || async {});
        app.map_get("/err", || async { Error::other("error") });
        app.map_err(|err: volga::error::Error| async move { status!(500, err.to_string()) });
        app.map_fallback(|| async { status!(404) });
    });

    // The floor: same client, same runtimes, same loopback, no Volga in the
    // path. Every number below is only meaningful as a delta over this one.
    let baseline = Harness::baseline();

    // End-to-end round trip, one request at a time. Stable, but loopback RTT
    // dominates it, so treat it as a whole-stack regression guard rather than as
    // a measure of framework cost.
    let mut group = c.benchmark_group("latency");
    group.bench_function("bare hyper", |b| baseline.get(b, "/", 200));
    group.bench_function("volga", |b| app.get(b, "/", 200));
    group.finish();

    // Saturated: the RTT is overlapped away and the single-worker server is the
    // bottleneck, so these are per-request service times.
    let mut group = c.benchmark_group("routing");
    group.bench_function("bare hyper", |b| baseline.get_saturated(b, "/", 200));
    group.bench_function("root", |b| app.get_saturated(b, "/", 200));
    group.bench_function("static 1 segment", |b| app.get_saturated(b, "/s", 200));
    group.bench_function("static 4 segments", |b| {
        app.get_saturated(b, "/s/a/b/c", 200)
    });
    group.bench_function("static 7 segments", |b| {
        app.get_saturated(b, "/s/a/b/c/d/e/f", 200)
    });
    group.bench_function("1 param", |b| app.get_saturated(b, "/p/x", 200));
    group.bench_function("2 params", |b| app.get_saturated(b, "/q/1/x", 200));
    group.bench_function("1 param after unwind", |b| {
        app.get_saturated(b, "/m/lit", 200)
    });
    group.finish();

    // Same route shape, different responses.
    //
    // `text body` deliberately repeats `routing/root`: two timings of an
    // identical request are the report's own noise gauge. Treat any difference
    // smaller than the gap between those two as noise, not as a regression.
    let mut group = c.benchmark_group("response");
    group.bench_function("text body", |b| app.get_saturated(b, "/", 200));
    group.bench_function("empty body", |b| app.get_saturated(b, "/empty", 200));
    group.bench_function("405", |b| app.get_saturated(b, "/put", 405));
    group.bench_function("404 fallback", |b| app.get_saturated(b, "/fall", 404));
    group.bench_function("500 error handler", |b| app.get_saturated(b, "/err", 500));
    group.finish();
}

criterion_group!(benches, benchmark);
criterion_main!(benches);
