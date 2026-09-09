#![allow(missing_docs)]

mod common;

use common::{BODY, Harness};
use criterion::{Criterion, criterion_group, criterion_main};

fn benchmark(c: &mut Criterion) {
    let app = Harness::new(|app| {
        // Control: the same route with no middleware attached, so the numbers
        // below can be read as the cost the pipeline stage adds.
        app.map_get("/plain", || async { BODY });

        app.map_get("/valid", || async { BODY })
            .filter(|| async { true });
        app.map_get("/invalid", || async { BODY })
            .filter(|| async { false });
    });

    let baseline = Harness::baseline();

    let mut group = c.benchmark_group("middleware");
    group.bench_function("bare hyper", |b| baseline.get_saturated(b, "/", 200));
    group.bench_function("no middleware", |b| app.get_saturated(b, "/plain", 200));
    group.bench_function("valid filter", |b| app.get_saturated(b, "/valid", 200));
    group.bench_function("invalid filter", |b| app.get_saturated(b, "/invalid", 400));
    group.finish();
}

criterion_group!(benches, benchmark);
criterion_main!(benches);
