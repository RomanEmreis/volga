# Volga DI
A standalone, flexible, and easy-to-configure DI container.

## Getting Started
### Dependencies
#### Standalone
```toml
[dependencies]
volga-di = "..."
```
#### Part of Volga Web Framework
```toml
[dependencies]
volga = { version = "...", features = ["di"] }
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

## License
Volga is licensed under the MIT License. Contributions welcome!

