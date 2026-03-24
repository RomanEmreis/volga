# Volga Rate Limiter

A lightweight and efficient rate-limiting library for Rust.

This crate provides rate limiting algorithms with pluggable storage backends,
designed for high-performance HTTP services and middleware.

## Overview

Rate limiting is used to control the number of requests that a client
(or a group of clients) can perform within a given time window.
Typical use cases include:

- Protecting APIs from abuse or accidental overload
- Enforcing fair usage policies
- Applying different limits for anonymous users, authenticated users,
  tenants, or API keys

## Algorithms

The following rate-limiting algorithms are provided:

- `FixedWindowRateLimiter`
  - Counts requests in discrete, fixed-size time windows
  - Very fast and simple
  - May allow short bursts at window boundaries

- `SlidingWindowRateLimiter`
  - Uses a sliding time window with linear weighting
  - Provides smoother request distribution
  - Slightly more expensive than a fixed window

- `TokenBucketRateLimiter`
  - Allows bursts up to a token bucket capacity
  - Enforces a steady average refill rate
  - Simple and flexible for bursty traffic

- `GcraRateLimiter`
  - Uses the Generic Cell Rate Algorithm (GCRA)
  - Smooths traffic with explicit burst tolerance
  - Accurate average rate enforcement

## Pluggable Storage Backends

Each algorithm is generic over a **store trait**, allowing you to swap the
default in-memory backend for an external one (e.g. Redis) without changing
the rate limiting logic.

| Algorithm | Store trait | Operation |
|---|---|---|
| Fixed Window | `FixedWindowStore` | `check_and_count` |
| Sliding Window | `SlidingWindowStore` | `check_and_count` |
| Token Bucket | `TokenBucketStore` | `try_consume` |
| GCRA | `GcraStore` | `check_and_advance` |

Each rate limiter provides constructors for all combinations:

- `::new()` — system clock + default in-memory store
- `::with_time_source()` — custom clock + in-memory store
- `::with_store()` — system clock + custom store
- `::with_time_source_and_store()` — both custom

The default in-memory stores are backed by `DashMap` and use lock-free
atomic operations on the hot path.

### Implementing a custom store

Store traits require a single atomic operation. Parameter structs are
`#[non_exhaustive]` for forward compatibility — access fields by name:

```rust
use volga_rate_limiter::store::{TokenBucketParams, TokenBucketStore};

struct MyRedisStore { /* ... */ }

impl TokenBucketStore for MyRedisStore {
    fn try_consume(&self, params: TokenBucketParams) -> bool {
        let key = params.key;
        let capacity = params.capacity_scaled;
        // ... your Redis logic here
        true
    }
}
```

> **Note:** Backends with built-in TTL support (like Redis) can skip manual
> eviction — the eviction grace parameters are designed for in-memory stores
> that perform lazy cleanup.

## Time Source Abstraction

All rate limiters are built on top of a pluggable `TimeSource` abstraction.
This allows:

- Deterministic and fast unit testing
- Custom time implementations if needed

The default implementation, `SystemTimeSource`, is based on
`std::time::Instant`.

## Concurrency Model

The rate limiters are designed to be:

- Thread-safe
- Lock-free or minimally locking on the hot path
- Safe to share between async tasks and threads

Internal state is optimized for frequent reads and updates under
high contention.

## Usage

The rate limiters are intended to be embedded into higher-level
frameworks or middleware layers.

## License
Volga is licensed under the MIT License. Contributions welcome!
