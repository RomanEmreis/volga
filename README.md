# Volga
Fast, Easy, and very flexible Web Framework for Rust based on [Tokio](https://tokio.rs/) runtime and [hyper](https://hyper.rs/) for fun and painless microservices crafting.

[![latest](https://img.shields.io/badge/latest-0.6.4-blue)](https://crates.io/crates/volga)
[![latest](https://img.shields.io/badge/rustc-1.80+-964B00)](https://crates.io/crates/volga)
[![License: MIT](https://img.shields.io/badge/License-MIT-violet.svg)](https://github.com/RomanEmreis/volga/blob/main/LICENSE)
[![Build](https://github.com/RomanEmreis/volga/actions/workflows/rust.yml/badge.svg)](https://github.com/RomanEmreis/volga/actions/workflows/rust.yml)
[![Release](https://github.com/RomanEmreis/volga/actions/workflows/release.yml/badge.svg)](https://github.com/RomanEmreis/volga/actions/workflows/release.yml)

> 💡 **Note**: This project is currently in preview. Breaking changes can be introduced without prior notice.

[Tutorial](https://romanemreis.github.io/volga-docs/) | [API Docs](https://docs.rs/volga/latest/volga/) | [Examples](https://github.com/RomanEmreis/volga/tree/main/examples) | [Roadmap](https://github.com/RomanEmreis/volga/milestone/1)

## Features
* Supports HTTP/1 and HTTP/2
* Robust routing
* Custom middlewares
* Dependency Injection
* WebSockets and WebTransport
* Full [Tokio](https://tokio.rs/) compatibility
* Runs on stable Rust 1.80+
## Getting Started
### Dependencies
```toml
[dependencies]
volga = "0.6.5"
tokio = { version = "1", features = ["full"] }
```
### Simple request handler
```rust
use volga::{App, ok};

#[tokio::main]
async fn main() -> std::io::Result<()> {
    // Start the server
    let mut app = App::new();

    // Example of a request handler
    app.map_get("/hello/{name}", async |name: String| {
        ok!("Hello {name}!")
    });
    
    app.run().await
}
```
## Performance
Tested a single instance on a laptop using 4 threads and 500 connections and under configuration:
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

