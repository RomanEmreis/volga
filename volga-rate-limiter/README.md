# Volga Rate Limiter

A lightweight and efficient rate-limiting library for Rust.

This crate provides in-memory rate limiting algorithms designed
for high-performance HTTP services and middleware.

[![latest](https://img.shields.io/badge/latest-0.8.2-blue)](https://crates.io/crates/volga)
[![latest](https://img.shields.io/badge/rustc-1.90+-964B00)](https://crates.io/crates/volga)
[![License: MIT](https://img.shields.io/badge/License-MIT-violet.svg)](https://github.com/RomanEmreis/volga/blob/main/LICENSE)
[![Build](https://github.com/RomanEmreis/volga/actions/workflows/rust.yml/badge.svg)](https://github.com/RomanEmreis/volga/actions/workflows/rust.yml)
[![Release](https://github.com/RomanEmreis/volga/actions/workflows/release.yml/badge.svg)](https://github.com/RomanEmreis/volga/actions/workflows/release.yml)

> 💡 **Status**: Volga is currently in preview.  
> The public API may change while core abstractions are being finalized.

## Overview

Rate limiting is used to control the number of requests that a client
(or a group of clients) can perform within a given time window.
Typical use cases include:

- Protecting APIs from abuse or accidental overload
- Enforcing fair usage policies
- Applying different limits for anonymous users, authenticated users,
  tenants, or API keys

This crate focuses on **per-node, in-memory** rate limiting.
It is intentionally simple and fast, and does **not** attempt to
synchronize state across multiple processes or machines.

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


## Time Source Abstraction

All rate limiters are built on top of a pluggable [`TimeSource`] abstraction.
This allows:

- Deterministic and fast unit testing
- Custom time implementations if needed

The default implementation, [`SystemTimeSource`], is based on
`std::time::Instant`.

## Concurrency Model

The rate limiters are designed to be:

- Thread-safe
- Lock-free or minimally locking on the hot path
- Safe to share between async tasks and threads

Internal state is optimized for frequent reads and updates under
high contention.

## Scope and Limitations

- This crate implements **in-memory** rate limiting only
- It does **not** provide distributed coordination
- For multi-node systems, rate limiting should be combined with
  external storage or coordination mechanisms (e.g. Redis, gateways)

## Usage

The rate limiters are intended to be embedded into higher-level
frameworks or middleware layers.
