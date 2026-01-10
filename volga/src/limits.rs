//! Resource limits and backpressure configuration.
//!
//! This module defines common abstractions used to control resource usage
//! and apply backpressure across the framework.
//!
//! Limits are used to protect the server from resource exhaustion and
//! denial-of-service scenarios by bounding memory usage, concurrency,
//! and protocol-level behavior.
//!
//! ## Design principles
//!
//! - Limits are **opt-in** and explicitly configured.
//! - Defaults are chosen to be safe and production-ready.
//! - Disabling a limit is always an explicit and intentional action.
//!
//! ## Usage
//!
//! Limits are typically configured at application startup:
//!
//! ```rust
//! use volga::{App, Limit};
//!
//! App::new()
//!     .with_body_limit(Limit::Limited(5 * 1024 * 1024))
//!     .with_max_connections(Limit::Limited(1000));
//! ```
//!
//! Some limits may also be applied at a more granular level,
//! such as per-route or per-protocol configuration.
//!
//! ⚠️ **Warning**
//!
//! Disabling limits (`Limit::Unlimited`) removes built-in safety guarantees
//! and should only be used in trusted environments or when external
//! backpressure mechanisms are in place.

#[cfg(feature = "http2")]
pub use http2::Http2Limits;

#[cfg(feature = "http2")]
mod http2;

/// Represents a configurable resource limit.
///
/// This enum is used throughout the framework to explicitly express
/// whether a limit:
/// - uses the framework default,
/// - is explicitly bounded,
/// - or is fully disabled.
///
/// # Variants
///
/// - [`Default`] — Uses the framework or transport default (recommended).
/// - [`Limited`] — Enforces an explicit upper bound.
/// - [`Unlimited`] — Disables the limit entirely.
///
/// ⚠️ Disabling limits may expose the server to resource exhaustion and
/// should only be done in trusted environments.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Limit<T> {
    /// Use the framework or transport default.
    Default,

    /// Enforce an explicit upper bound.
    Limited(T),

    /// Disable the limit entirely.
    Unlimited,
}

impl<T> Limit<T> {
    /// Returns `true` if this limit is disabled.
    #[inline(always)]
    pub fn is_unlimited(&self) -> bool {
        matches!(self, Limit::Unlimited)
    }

    /// Returns `true` if this limit enforces an explicit bound.
    #[inline(always)]
    pub fn is_limited(&self) -> bool {
        matches!(self, Limit::Limited(_))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_returns_true_for_unlimited_when_checks_is_unlimited() {
        let limit = Limit::<usize>::Unlimited;

        assert!(limit.is_unlimited())
    }

    #[test]
    fn it_returns_false_for_limited_when_checks_is_unlimited() {
        let limit = Limit::<usize>::Limited(100);

        assert!(!limit.is_unlimited())
    }

    #[test]
    fn it_returns_true_for_limited_when_checks_is_limited() {
        let limit = Limit::<usize>::Limited(100);

        assert!(limit.is_limited())
    }

    #[test]
    fn it_returns_false_for_unlimited_when_checks_is_limited() {
        let limit = Limit::<usize>::Unlimited;

        assert!(!limit.is_limited())
    }
}