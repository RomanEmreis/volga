# Volga DI
A standalone, flexible, and easy-to-configure DI container.

[![latest](https://img.shields.io/badge/latest-0.8.0-blue)](https://crates.io/crates/volga)
[![latest](https://img.shields.io/badge/rustc-1.90+-964B00)](https://crates.io/crates/volga)
[![License: MIT](https://img.shields.io/badge/License-MIT-violet.svg)](https://github.com/RomanEmreis/volga/blob/main/LICENSE)
[![Build](https://github.com/RomanEmreis/volga/actions/workflows/rust.yml/badge.svg)](https://github.com/RomanEmreis/volga/actions/workflows/rust.yml)
[![Release](https://github.com/RomanEmreis/volga/actions/workflows/release.yml/badge.svg)](https://github.com/RomanEmreis/volga/actions/workflows/release.yml)

> 💡 **Status**: Volga is currently in preview.  
> The public API may change while core abstractions are being finalized.

## Getting Started
### Dependencies
#### Standalone
```toml
[dependencies]
volga-di = "0.8.0"
```
#### Part of Volga Web Framework
```toml
[dependencies]
volga = { version = "0.8.0", features = ["di"] }
```

### Example
```rust
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use volga_di::ContainerBuilder;

#[derive(Default, Clone)]
struct InMemoryCache {
    inner: Arc<Mutex<HashMap<String, String>>>
}

fn main() {
    let mut container = ContainerBuilder::new();
    container.register_singleton(InMemoryCache::default());

    let container = container.build();

    let Ok(cache) = container.resolve::<InMemoryCache>() else { 
        panic!("Unable to resolve InMemoryCache")
    };

    // Do work...
}
```

