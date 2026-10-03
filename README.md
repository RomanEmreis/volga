# Volga
A fast, explicit web framework for Rust, built on [Tokio](https://tokio.rs/) and
[hyper](https://hyper.rs/).

Volga makes HTTP services straightforward to write and easy to read, with predictable
performance and minimal overhead.

[![latest](https://img.shields.io/badge/latest-0.13.1-blue)](https://crates.io/crates/volga)
[![latest](https://img.shields.io/badge/rustc-1.90+-964B00)](https://releases.rs/docs/1.90.0/)
[![License: MIT](https://img.shields.io/badge/License-MIT-violet.svg)](https://github.com/RomanEmreis/volga/blob/main/LICENSE)
[![Build](https://github.com/RomanEmreis/volga/actions/workflows/rust.yml/badge.svg)](https://github.com/RomanEmreis/volga/actions/workflows/rust.yml)
[![Release](https://github.com/RomanEmreis/volga/actions/workflows/release.yml/badge.svg)](https://github.com/RomanEmreis/volga/actions/workflows/release.yml)

> 💡 **Status**: Volga is in preview. The public API may still change.

[Tutorial](https://romanemreis.github.io/volga-docs/) | [API Docs](https://docs.rs/volga/latest/volga/) | [Examples](https://github.com/RomanEmreis/volga/tree/main/examples) | [Roadmap](https://github.com/RomanEmreis/volga/milestone/1)

## Why Volga?

Volga favors clarity and control without giving up performance. Handlers, middleware and
routing do what they look like they do, and macros are used sparingly, only to cut
boilerplate.

Volga is a good fit if you:

- Want handler signatures that read like plain functions
- Care about predictable performance and low overhead
- Need fine-grained control over the request/response lifecycle
- Work with streaming, WebSockets or long-lived connections
- Prefer explicit APIs to code generation

## Features
- HTTP/1 and HTTP/2
- Explicit routing: route groups, typed path parameters, catch-all segments
- Async and synchronous handlers, composable middleware
- Typed request extraction, with validation
- Dependency injection without derive macros
- WebSockets, including WebSocket over HTTP/2
- Streaming responses and Server-Sent Events
- Full **Tokio** compatibility, stable Rust **1.90+**

### Batteries included
Each of these is a Cargo feature, so only what you enable is compiled in:

- OpenAPI 3 documents and Swagger UI
- Authentication: JWT bearer, Basic, and end-to-end OAuth 2.1/OIDC
- TLS with HSTS and HTTPS redirection
- Rate limiting, CORS, response compression and request decompression
- Static files, with a fallback file for single-page apps
- Cookies (signed and private), multipart, TOML configuration
- RFC 9457 problem details and `tracing` integration

## Getting Started
```toml
[dependencies]
volga = "0.13.1"
tokio = { version = "1", features = ["full"] }
```
```rust
use volga::{App, ok};

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let mut app = App::new();

    app.map_get("/hello/{name}", |name: String| ok!("Hello {name}!"));

    app.run().await
}
```
`name` is read from the path by its type, and a handler with nothing to await returns
its response directly. An `async` handler is mapped the same way.

Middleware, dependency injection, auth, rate limiting, blocking handlers and more are
covered in the [tutorial](https://romanemreis.github.io/volga-docs/) and the
[examples](https://github.com/RomanEmreis/volga/tree/main/examples).

## Performance
Volga is benchmarked with a minimal plaintext endpoint, which measures baseline HTTP
throughput. The benchmark harness is at
[volga-benchmark](https://github.com/RomanEmreis/volga-benchmark).

### Benchmark environment

Tested on a single machine:
```
Platform: Apple Silicon MacBook Pro
Runtime: Linux containers (Colima)
```

### Results
```
Running 30s test @ http://volga:7878/plaintext
  8 threads and 512 connections
  Thread Stats   Avg      Stdev     Max   +/- Stdev
    Latency   539.89us    3.00ms 213.60ms   99.94%
    Req/Sec   131.04k    14.14k 296.06k    95.76%
  31080913 requests in 29.83s, 3.76GB read
  Socket errors: connect 0, read 0, write 0, timeout 987
Requests/sec: 1,041,952.40
Transfer/sec: 129.18MB
```

> ⚠️ Benchmark results are provided for reference only.
> Actual performance depends on workload, middleware, and handler logic.

## License
Volga is licensed under the MIT License. Contributions welcome!
