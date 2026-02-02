# Volga
Fast, simple, and high-performance web framework for Rust, built on top of
[Tokio](https://tokio.rs/) and [hyper](https://hyper.rs/).

Volga is designed to make building HTTP services straightforward and explicit,
while keeping performance predictable and overhead minimal.

[![latest](https://img.shields.io/badge/latest-0.8.3-blue)](https://crates.io/crates/volga)
[![latest](https://img.shields.io/badge/rustc-1.90+-964B00)](https://crates.io/crates/volga)
[![License: MIT](https://img.shields.io/badge/License-MIT-violet.svg)](https://github.com/RomanEmreis/volga/blob/main/LICENSE)
[![Build](https://github.com/RomanEmreis/volga/actions/workflows/rust.yml/badge.svg)](https://github.com/RomanEmreis/volga/actions/workflows/rust.yml)
[![Release](https://github.com/RomanEmreis/volga/actions/workflows/release.yml/badge.svg)](https://github.com/RomanEmreis/volga/actions/workflows/release.yml)

> 💡 **Status**: Volga is currently in preview.  
> The public API may change while core abstractions are being finalized.

[Tutorial](https://romanemreis.github.io/volga-docs/) | [API Docs](https://docs.rs/volga/latest/volga/) | [Examples](https://github.com/RomanEmreis/volga/tree/main/examples) | [Roadmap](https://github.com/RomanEmreis/volga/milestone/1)

## Why Volga?

Volga focuses on clarity and control without sacrificing performance.

It avoids hidden behavior and framework-driven magic.
Macros are used sparingly and primarily to reduce boilerplate. Handlers, middleware, and routing behave exactly as they look in code.

Volga is a good fit if you:

- Want simple and readable handler signatures
- Care about predictable performance and low overhead
- Need fine-grained control over the HTTP request/response lifecycle
- Work with streaming, WebSockets, or long-lived connections
- Prefer explicit APIs over code generation

## Features
- HTTP/1 and HTTP/2 support
- Explicit and robust routing
- Composable async middlewares
- Dependency Injection without derive macros
- Typed request extraction
- WebSockets and WebSocket-over-HTTP/2
- Streaming-friendly HTTP
- Full **Tokio** compatibility
- Runs on stable Rust **1.90+**

## Getting Started
### Dependencies
```toml
[dependencies]
volga = "0.8.3"
tokio = { version = "1", features = ["full"] }
```
### Simple request handler
```rust
use volga::{App, ok};

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let mut app = App::new();
    
    app.map_get("/hello/{name}", async |name: String| {
        ok!("Hello {name}!")
    });
    
    app.run().await
}
```
This example demonstrates:

* typed path parameter extraction
* async request handlers
* minimal setup with zero boilerplate

More advanced examples (middleware, DI, auth, rate limiting) can be found in the
[documentation](https://romanemreis.github.io/volga-docs/) and [here](https://github.com/RomanEmreis/volga/tree/main/examples).

## Performance
Tested on a single instance with 4 threads and 500 concurrent connections:

```
OS: Arch Linux
CPU: Intel i7-8665U (8) @ 4.800GHz
RAM: 31686MiB
```
### Results
```
Running 10s test @ http://127.0.0.1:7878/hello
  4 threads and 500 connections
  Thread Stats   Avg      Stdev     Max   +/- Stdev
    Latency     1.39ms    1.05ms  18.45ms   81.47%
    Req/Sec     89.69k    18.07k  126.91k   57.50%
  3575551 requests in 10.07s, 395.55MB read
Requests/sec: 355053.82
Transfer/sec: 39.28MB
```

> ⚠️ Benchmark results are provided for reference only.
> Actual performance depends on workload, middleware, and handler logic.

## License
Volga is licensed under the MIT License. Contributions welcome!
