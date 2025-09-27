# Volga DI
A standalone, flexible, and easy-to-configure DI container.

[![latest](https://img.shields.io/badge/latest-0.6.3-blue)](https://crates.io/crates/volga)
[![latest](https://img.shields.io/badge/rustc-1.80+-964B00)](https://crates.io/crates/volga)
[![License: MIT](https://img.shields.io/badge/License-MIT-violet.svg)](https://github.com/RomanEmreis/volga/blob/main/LICENSE)
[![Build](https://github.com/RomanEmreis/volga/actions/workflows/rust.yml/badge.svg)](https://github.com/RomanEmreis/volga/actions/workflows/rust.yml)
[![Release](https://github.com/RomanEmreis/volga/actions/workflows/release.yml/badge.svg)](https://github.com/RomanEmreis/volga/actions/workflows/release.yml)

> 💡 **Note**: This project is currently in preview. Breaking changes can be introduced without prior notice.

## Getting Started
### Dependencies
#### Standalone
```toml
[dependencies]
volga-di = "0.6.5"
```
#### Part of Volga Web Framework
```toml
[dependencies]
volga = { version = "0.6.5", features = ["di"] }
```
#### Derive-macro support
```toml
[dependencies]
volga = { version = "0.6.4", features = ["di-full"] }
```

### Example
```rust
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct InMemoryCache {
    inner: Arc<Mutex<HashMap<String, String>>>
}

fn main() {
    let mut container = ContainerBuilder::new();
    container.register_singleton(InMemoryCache::default());

    let container = container.build();

    let Ok(cache) = container.resolve::<InMemoryCache>() else { 
        eprintln!("Unable to resolve InMemoryCache")
    };

    // Do work...
}
```

