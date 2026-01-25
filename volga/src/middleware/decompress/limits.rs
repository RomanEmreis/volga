//! Decompression limits.
//!
//! This module defines configurable guardrails for decompressing request/response bodies.
//! Decompression is a common DoS vector: small compressed inputs may expand into very large
//! outputs (zip bomb / gzip bomb). These limits help you fail fast while streaming data.
//!
//! # What is limited
//!
//! The limiter can enforce up to three independent constraints:
//!
//! - **Max decompressed bytes**: hard cap for the total number of bytes produced by the decoder.
//! - **Max compressed bytes**: hard cap for the total number of bytes read from the compressed input.
//! - **Max expansion ratio**: bounds decompressed growth relative to what has been consumed from
//!   the compressed stream.
//!
//! The expansion ratio check is implemented as:
//!
//! ```text
//! allowed_decompressed = compressed_so_far * ratio + slack_bytes
//! ```
//!
//! `slack_bytes` allows small bodies to decompress without being penalized by a strict ratio early
//! in the stream (when `compressed_so_far` is still small).
//!
//! # Defaults
//!
//! The default configuration is intentionally conservative:
//!
//! - max decompressed bytes: 16 MiB
//! - max compressed bytes: 5 MiB
//! - max expansion ratio: 100x (+ 1 MiB slack)
//!
//! # Limit semantics
//!
//! The [`Limit`] type supports:
//! - `Default`   — use the module default
//! - `Limited(n)`— enforce the provided value
//! - `Unlimited` — disable the check
//!
//! ⚠️ Setting limits to `Unlimited` removes safety rails and may allow memory / CPU exhaustion.
//! Use with care and only when the surrounding system provides other protections.

use crate::Limit;

const DEFAULT_MAX_DECOMPRESSED_BYTES: usize = 16 * 1024 * 1024; // 16 MiB
const DEFAULT_MAX_COMPRESSED_BYTES: usize = 5 * 1024 * 1024; // 5 MiB
const DEFAULT_MAX_EXPANSION_RATIO: usize = 100;
const DEFAULT_EXPANSION_SLACK_BYTES: usize = 1024 * 1024; // 1 MiB

/// Decompressed-to-compressed growth constraints.
///
/// The decompressor enforces an upper bound for how much decompressed data is allowed
/// relative to the amount of compressed input consumed so far:
///
/// ```text
/// allowed_decompressed = compressed_so_far * ratio + slack_bytes
/// ```
///
/// - `ratio` is a multiplicative factor (e.g. `100` means "up to 100x expansion").
/// - `slack_bytes` is a constant allowance to prevent false positives for small payloads
///   early in the stream.
///
/// This limit is optional (see [`DecompressionLimits::with_max_expansion_ratio`]).
///
/// # Examples
///
/// Allow up to 50x expansion plus 256 KiB of slack:
///
/// ```no_run
/// # use volga::middleware::decompress::ExpansionRatio;
/// let ratio = ExpansionRatio::new(50, 256 * 1024);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ExpansionRatio {
    pub(super) ratio: usize,
    pub(super) slack_bytes: usize,
}

impl ExpansionRatio {
    /// Creates a new expansion ratio limit.
    ///
    /// `ratio` is a multiplier for `compressed_so_far`, and `slack_bytes` is an additional
    /// constant allowance.
    ///
    /// # Panics / Validation
    ///
    /// This constructor does not validate inputs. Consider validating at the call site
    /// (e.g. ratio must be > 0) if your API accepts user-provided values.
    #[inline]
    pub fn new(ratio: usize, slack_bytes: usize) -> Self {
        debug_assert!(ratio > 0, "expansion ratio must be greater than zero");

        Self { ratio, slack_bytes }
    }
}

/// User-facing decompression limits configuration.
///
/// This type is typically configured once and then "resolved" into concrete numeric limits
/// (see [`DecompressionLimits::resolved`]).
///
/// Each limit uses [`Limit`] semantics:
/// - `Default` uses a module default,
/// - `Limited(n)` enforces a specific value,
/// - `Unlimited` disables the corresponding check.
///
/// # Examples
///
/// Use stricter decompressed output cap and disable ratio checks:
///
/// ```no_run
/// # use volga::middleware::decompress::{DecompressionLimits, ExpansionRatio};
/// # use volga::Limit;
/// let limits = DecompressionLimits::default()
///     .with_max_decompressed(Limit::Limited(8 * 1024 * 1024)) // 8 MiB
///     .without_max_expansion_ratio();
/// ```
#[derive(Debug, Clone, Copy)]
pub struct DecompressionLimits {
    pub(super) max_decompressed_bytes: Limit<usize>,
    pub(super) max_compressed_bytes: Limit<usize>,
    pub(super) max_expansion_ratio: Option<ExpansionRatio>,
}

impl Default for DecompressionLimits {
    /// Returns the default decompression safety limits.
    ///
    /// Defaults are conservative and intended to provide baseline protection against
    /// decompression bombs.
    #[inline]
    fn default() -> Self {
        Self {
            max_decompressed_bytes: Limit::Limited(DEFAULT_MAX_DECOMPRESSED_BYTES),
            max_compressed_bytes: Limit::Limited(DEFAULT_MAX_COMPRESSED_BYTES),
            max_expansion_ratio: Some(ExpansionRatio::new(
                DEFAULT_MAX_EXPANSION_RATIO,
                DEFAULT_EXPANSION_SLACK_BYTES
            )),
        }
    }
}

impl DecompressionLimits {
    /// Sets the maximum allowed number of bytes produced by the decompressor.
    ///
    /// If set to `Unlimited`, the decompressed output cap is disabled.
    #[inline]
    pub fn with_max_decompressed(mut self, limit: Limit<usize>) -> Self {
        warn_if_unlimited("decompression max_decompressed_bytes", limit);
        self.max_decompressed_bytes = limit;
        self
    }

    /// Sets the maximum allowed number of bytes consumed from the compressed input.
    ///
    /// If set to `Unlimited`, the compressed input cap is disabled.
    #[inline]
    pub fn with_max_compressed(mut self, limit: Limit<usize>) -> Self {
        warn_if_unlimited("decompression max_compressed_bytes", limit);
        self.max_compressed_bytes = limit;
        self
    }

    /// Enables the expansion ratio guard.
    #[inline]
    pub fn with_max_expansion_ratio(mut self, ratio: ExpansionRatio) -> Self {
        self.max_expansion_ratio = Some(ratio);
        self
    }

    /// Disables the expansion ratio guard.
    #[inline]
    pub fn without_max_expansion_ratio(mut self) -> Self {
        self.max_expansion_ratio = None;
        self
    }

    /// Resolves [`Limit`] values into concrete numeric limits.
    ///
    /// - `Default` becomes `Some(DEFAULT_*)`
    /// - `Limited(n)` becomes `Some(n)`
    /// - `Unlimited` becomes `None` (meaning "no limit")
    #[inline]
    pub(crate) fn resolved(self) -> ResolvedDecompressionLimits {
        ResolvedDecompressionLimits {
            max_decompressed_bytes: resolve_limit(self.max_decompressed_bytes, DEFAULT_MAX_DECOMPRESSED_BYTES),
            max_compressed_bytes: resolve_limit(self.max_compressed_bytes, DEFAULT_MAX_COMPRESSED_BYTES),
            max_expansion_ratio: self.max_expansion_ratio,
        }
    }
}

/// Concrete, internal representation of limits.
///
/// This is derived from [`DecompressionLimits`] by resolving `Default` and `Unlimited`
/// into `Option<usize>` for fast checks in hot paths.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ResolvedDecompressionLimits {
    /// Maximum total decompressed bytes, or `None` if disabled.
    pub(super) max_decompressed_bytes: Option<usize>,

    /// Maximum total compressed bytes, or `None` if disabled.
    pub(super) max_compressed_bytes: Option<usize>,

    /// Optional expansion ratio guard.
    pub(super) max_expansion_ratio: Option<ExpansionRatio>,
}

/// Converts [`Limit`] into an `Option<usize>`.
///
/// `None` means "no limit".
#[inline]
fn resolve_limit(limit: Limit<usize>, default: usize) -> Option<usize> {
    match limit {
        Limit::Default => Some(default),
        Limit::Limited(value) => Some(value),
        Limit::Unlimited => None,
    }
}

#[inline]
fn warn_if_unlimited(name: &str, limit: Limit<usize>) {
    if matches!(limit, Limit::Unlimited) {
        #[cfg(feature = "tracing")]
        tracing::warn!(
            "{name} is set to Unlimited; decompression safety checks are disabled."
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Limit;

    #[test]
    fn expansion_ratio_new_sets_fields() {
        let r = ExpansionRatio::new(123, 456);

        assert_eq!(r.ratio, 123);
        assert_eq!(r.slack_bytes, 456);
    }

    #[test]
    fn decompression_limits_default_values_match_constants() {
        let limits = DecompressionLimits::default();

        match limits.max_decompressed_bytes {
            Limit::Limited(v) => assert_eq!(v, DEFAULT_MAX_DECOMPRESSED_BYTES),
            other => panic!("expected Limited for max_decompressed_bytes, got: {:?}", other),
        }

        match limits.max_compressed_bytes {
            Limit::Limited(v) => assert_eq!(v, DEFAULT_MAX_COMPRESSED_BYTES),
            other => panic!("expected Limited for max_compressed_bytes, got: {:?}", other),
        }

        let ratio = limits
            .max_expansion_ratio
            .expect("default max_expansion_ratio must be Some");

        assert_eq!(ratio.ratio, DEFAULT_MAX_EXPANSION_RATIO);
        assert_eq!(ratio.slack_bytes, DEFAULT_EXPANSION_SLACK_BYTES);
    }

    #[test]
    fn with_max_decompressed_overrides_value() {
        let limits = DecompressionLimits::default()
            .with_max_decompressed(Limit::Limited(123));

        match limits.max_decompressed_bytes {
            Limit::Limited(v) => assert_eq!(v, 123),
            other => panic!("expected Limited, got: {:?}", other),
        }
    }

    #[test]
    fn with_max_compressed_overrides_value() {
        let limits = DecompressionLimits::default()
            .with_max_compressed(Limit::Limited(321));

        match limits.max_compressed_bytes {
            Limit::Limited(v) => assert_eq!(v, 321),
            other => panic!("expected Limited, got: {:?}", other),
        }
    }

    #[test]
    fn with_max_expansion_ratio_can_disable_ratio_guard() {
        let limits = DecompressionLimits::default()
            .without_max_expansion_ratio();

        assert!(limits.max_expansion_ratio.is_none());
    }

    #[test]
    fn with_max_expansion_ratio_can_set_custom_ratio() {
        let custom = ExpansionRatio::new(7, 999);

        let limits = DecompressionLimits::default()
            .with_max_expansion_ratio(custom);

        let r = limits.max_expansion_ratio.unwrap();
        assert_eq!(r.ratio, 7);
        assert_eq!(r.slack_bytes, 999);
    }

    #[test]
    fn resolved_maps_default_to_some_default_constant() {
        let limits = DecompressionLimits {
            max_decompressed_bytes: Limit::Default,
            max_compressed_bytes: Limit::Default,
            max_expansion_ratio: None,
        };

        let resolved = limits.resolved();

        assert_eq!(resolved.max_decompressed_bytes, Some(DEFAULT_MAX_DECOMPRESSED_BYTES));
        assert_eq!(resolved.max_compressed_bytes, Some(DEFAULT_MAX_COMPRESSED_BYTES));
        assert!(resolved.max_expansion_ratio.is_none());
    }

    #[test]
    fn resolved_maps_limited_to_some_value() {
        let limits = DecompressionLimits {
            max_decompressed_bytes: Limit::Limited(10),
            max_compressed_bytes: Limit::Limited(20),
            max_expansion_ratio: Some(ExpansionRatio::new(3, 4)),
        };

        let resolved = limits.resolved();

        assert_eq!(resolved.max_decompressed_bytes, Some(10));
        assert_eq!(resolved.max_compressed_bytes, Some(20));

        let r = resolved.max_expansion_ratio.unwrap();
        assert_eq!(r.ratio, 3);
        assert_eq!(r.slack_bytes, 4);
    }

    #[test]
    fn resolved_maps_unlimited_to_none() {
        let limits = DecompressionLimits {
            max_decompressed_bytes: Limit::Unlimited,
            max_compressed_bytes: Limit::Unlimited,
            max_expansion_ratio: Some(ExpansionRatio::new(1, 2)),
        };

        let resolved = limits.resolved();

        assert_eq!(resolved.max_decompressed_bytes, None);
        assert_eq!(resolved.max_compressed_bytes, None);
        assert!(resolved.max_expansion_ratio.is_some());
    }

    #[test]
    fn resolve_limit_behavior() {
        assert_eq!(
            resolve_limit(Limit::Default, 111),
            Some(111)
        );
        assert_eq!(
            resolve_limit(Limit::Limited(222), 111),
            Some(222)
        );
        assert_eq!(
            resolve_limit(Limit::Unlimited, 111),
            None
        );
    }

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "expansion ratio must be greater than zero")]
    fn expansion_ratio_zero_panics_in_debug() {
        let _ = ExpansionRatio::new(0, 123);
    }
}
