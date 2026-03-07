# Fuzzing Volga

Volga uses `cargo-fuzz` (libFuzzer) with deterministic, bounded fuzz targets.

> Note: the `volga` crate feature used by the harnesses is `__fuzzing`, which is **internal-only** and not a supported public API surface.

## Targets

- `fuzz_router_match`: exercises route matching with random method/path/optional host.
- `fuzz_query_decode`: exercises percent-decoding + query parsing through `serde_urlencoded` and `Query<T>`.
- `fuzz_extractor_typed`: exercises typed JSON extraction using bounded random headers/body.
- `fuzz_openapi_gen`: exercises OpenAPI route registration/document generation using bounded selectors.

## Local usage

```bash
cargo install cargo-fuzz
cargo +nightly fuzz build

ASAN_OPTIONS=quarantine_size_mb=1:malloc_context_size=0 \
  cargo +nightly fuzz run fuzz_router_match -- \
  -max_len=512 -max_total_time=20 -rss_limit_mb=512

ASAN_OPTIONS=quarantine_size_mb=1:malloc_context_size=0 \
  cargo +nightly fuzz run fuzz_query_decode -- \
  -max_len=1024 -max_total_time=20 -rss_limit_mb=512

ASAN_OPTIONS=quarantine_size_mb=1:malloc_context_size=0 \
  cargo +nightly fuzz run fuzz_extractor_typed -- \
  -max_len=4096 -max_total_time=20 -rss_limit_mb=512

ASAN_OPTIONS=quarantine_size_mb=1:malloc_context_size=0 \
  cargo +nightly fuzz run fuzz_openapi_gen -- \
  -max_len=512 -max_total_time=20 -rss_limit_mb=512
```

## CI behavior

- PRs: `cargo fuzz build` + smoke fuzz runs for `fuzz_router_match` and `fuzz_query_decode`.
- Nightly schedule: all four targets run for longer windows.
- All jobs set `ASAN_OPTIONS=quarantine_size_mb=1:malloc_context_size=0` and `-rss_limit_mb=512`.

## Corpus

Seed corpora live in `fuzz/corpus/<target>` and include common router/query edge cases, extractor payloads, and OpenAPI selectors.
