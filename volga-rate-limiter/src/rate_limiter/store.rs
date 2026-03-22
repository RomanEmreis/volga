//! Store traits and parameter types for pluggable rate-limiting backends.
//!
//! Each rate-limiting algorithm defines:
//! - A `XxxParams` struct carrying all inputs the store operation needs.
//! - A `XxxStore` trait with one atomic operation that checks and updates state.
//!
//! The `#[non_exhaustive]` attribute on params structs keeps the API
//! forward-compatible: new fields can be added without breaking existing
//! store implementations.
//!
//! ## Note for external store implementors
//!
//! Params structs are `#[non_exhaustive]`, which means they cannot be
//! constructed or exhaustively destructured outside this crate. When
//! implementing a custom store, access fields by name:
//!
//! ```rust,ignore
//! fn check_and_count(&self, params: FixedWindowParams) -> bool {
//!     let key = params.key;
//!     let window = params.window;
//!     // ...
//! }
//! ```

/// Parameters for a single [`FixedWindowStore`] operation.
///
/// Constructed by [`FixedWindowRateLimiter`] on every `check` call and
/// passed to the store. External implementors receive this struct but
/// never construct it.
///
/// [`FixedWindowRateLimiter`]: super::FixedWindowRateLimiter
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub struct FixedWindowParams {
    /// Partition key identifying the client.
    pub key: u64,
    /// Start timestamp of the current window (seconds, precomputed by the limiter).
    pub window: u64,
    /// Maximum allowed requests per window.
    pub max_requests: u32,
    /// Current time in seconds (used for eviction).
    pub now: u64,
    /// Eviction grace period in seconds.
    pub grace_secs: u64,
}

/// A pluggable storage backend for the fixed-window algorithm.
///
/// The single `check_and_count` operation must atomically:
/// 1. Evict stale entries (optional — a TTL-based store may skip this).
/// 2. Reset the counter if the stored window differs from `params.window`.
/// 3. Increment the counter.
/// 4. Return `true` if the pre-increment counter was below `params.max_requests`.
///
/// # Atomicity and eviction
///
/// The entire operation — check, reset, and increment — must be atomic per key
/// to avoid TOCTOU races under concurrent access. For distributed backends with
/// TTL support (e.g. Redis), eviction can be delegated to the backend's TTL
/// mechanism; the explicit eviction step in the algorithm above may then be omitted.
///
/// # Thread safety
///
/// Implementations must be `Send + Sync` and safe for concurrent access.
pub trait FixedWindowStore: Send + Sync {
    /// Atomically checks and increments the counter for the given window.
    fn check_and_count(&self, params: FixedWindowParams) -> bool;
}

// ---------------------------------------------------------------------------

/// Parameters for a single [`SlidingWindowStore`] operation.
///
/// [`SlidingWindowRateLimiter`]: super::SlidingWindowRateLimiter
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub struct SlidingWindowParams {
    /// Partition key identifying the client.
    pub key: u64,
    /// Start timestamp of the current window (seconds, precomputed by the limiter).
    pub window: u64,
    /// Window duration in seconds (needed to compute the sliding weight).
    pub window_size_secs: u64,
    /// Maximum allowed requests per window.
    pub max_requests: u32,
    /// Current time in seconds (used for progress and eviction).
    pub now: u64,
    /// Eviction grace period in seconds.
    pub grace_secs: u64,
}

/// A pluggable storage backend for the sliding-window algorithm.
///
/// The single `check_and_count` operation must atomically:
/// 1. Evict stale entries if applicable.
/// 2. Roll window counters if the window has advanced.
/// 3. Compute the weighted effective request count.
/// 4. If `effective < params.max_requests`: increment the current counter
///    and return `true`; otherwise return `false` without incrementing.
///
/// The weighted effective count formula is:
/// ```text
/// effective = prev_count * (1 - progress) + curr_count
/// ```
/// where `progress = (now - window_start) / window_size_secs`.
///
/// # Atomicity and eviction
///
/// Steps 3 and 4 (check and conditional increment) must be atomic per key to
/// avoid TOCTOU races. For distributed backends with TTL support (e.g. Redis),
/// eviction can be delegated to the backend's TTL mechanism.
pub trait SlidingWindowStore: Send + Sync {
    /// Atomically checks and (conditionally) increments the counter.
    fn check_and_count(&self, params: SlidingWindowParams) -> bool;
}

// ---------------------------------------------------------------------------

/// Parameters for a single [`TokenBucketStore`] operation.
///
/// All token values use a fixed-point representation:
/// `actual_tokens = scaled_tokens / scale`.
///
/// [`TokenBucketRateLimiter`]: super::TokenBucketRateLimiter
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub struct TokenBucketParams {
    /// Partition key identifying the client.
    pub key: u64,
    /// Current time in microseconds.
    pub now_us: u64,
    /// Maximum tokens in fixed-point representation (`capacity * scale`).
    pub capacity_scaled: u64,
    /// Refill rate in fixed-point tokens per second (`rate * scale`).
    pub refill_rate_scaled_per_sec: u64,
    /// Fixed-point scaling factor.
    pub scale: u64,
    /// Eviction grace period in microseconds.
    pub eviction_grace_us: u64,
}

/// A pluggable storage backend for the token-bucket algorithm.
///
/// The single `try_consume` operation must atomically:
/// 1. Evict stale entries if applicable.
/// 2. Refill tokens based on elapsed time since last refill.
/// 3. If at least one scaled token is available, consume it and return `true`.
/// 4. Otherwise return `false`.
///
/// # Atomicity and eviction
///
/// The refill and consume steps must be atomic per key to prevent races where two
/// concurrent requests both see sufficient tokens and both consume, exceeding the
/// limit. For distributed backends with TTL support, eviction can be delegated to
/// the backend's TTL mechanism.
pub trait TokenBucketStore: Send + Sync {
    /// Atomically refills and tries to consume one token.
    fn try_consume(&self, params: TokenBucketParams) -> bool;
}

// ---------------------------------------------------------------------------

/// Parameters for a single [`GcraStore`] operation.
///
/// [`GcraRateLimiter`]: super::GcraRateLimiter
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub struct GcraParams {
    /// Partition key identifying the client.
    pub key: u64,
    /// Current time in microseconds.
    pub now_us: u64,
    /// Emission interval (τ) in microseconds: `ceil(1_000_000 / rate_per_second)`.
    pub emission_interval_us: u64,
    /// Burst allowance in microseconds: `emission_interval_us * (burst - 1)`.
    pub burst_allowance_us: u64,
    /// Eviction grace period in microseconds.
    pub eviction_grace_us: u64,
}

/// A pluggable storage backend for the GCRA algorithm.
///
/// The single `check_and_advance` operation must atomically:
/// 1. Evict stale entries if applicable.
/// 2. Load the stored theoretical arrival time (`tat`).
/// 3. Allow the request if `now_us + burst_allowance_us >= tat`.
/// 4. If allowed, update `tat = max(now_us, tat) + emission_interval_us` via CAS.
/// 5. Return `true` if allowed, `false` otherwise.
///
/// # Atomicity and eviction
///
/// The check-and-update of `tat` must be atomic per key (CAS loop or equivalent)
/// to prevent two concurrent requests from both reading the same `tat` and both
/// succeeding. For distributed backends with TTL support, eviction can be delegated
/// to the backend's TTL mechanism.
pub trait GcraStore: Send + Sync {
    /// Atomically checks and advances the theoretical arrival time.
    fn check_and_advance(&self, params: GcraParams) -> bool;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_window_params_fields_are_accessible() {
        let p = FixedWindowParams {
            key: 1,
            window: 100,
            max_requests: 10,
            now: 150,
            grace_secs: 300,
        };
        assert_eq!(p.key, 1);
        assert_eq!(p.max_requests, 10);
    }

    #[test]
    fn sliding_window_params_fields_are_accessible() {
        let p = SlidingWindowParams {
            key: 2,
            window: 200,
            window_size_secs: 60,
            max_requests: 5,
            now: 230,
            grace_secs: 120,
        };
        assert_eq!(p.window_size_secs, 60);
    }

    #[test]
    fn token_bucket_params_fields_are_accessible() {
        let p = TokenBucketParams {
            key: 3,
            now_us: 1_000_000,
            capacity_scaled: 10_000_000,
            refill_rate_scaled_per_sec: 1_000_000,
            scale: 1_000_000,
            eviction_grace_us: 60_000_000,
        };
        assert_eq!(p.scale, 1_000_000);
    }

    #[test]
    fn gcra_params_fields_are_accessible() {
        let p = GcraParams {
            key: 4,
            now_us: 2_000_000,
            emission_interval_us: 100_000,
            burst_allowance_us: 300_000,
            eviction_grace_us: 60_000_000,
        };
        assert_eq!(p.emission_interval_us, 100_000);
    }
}
