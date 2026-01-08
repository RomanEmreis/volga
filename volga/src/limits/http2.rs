//! HTTP/2 resource and backpressure limits.
//!
//! This module provides configuration for HTTP/2 resource usage
//! and backpressure management. 
//!
//! [`Http2Limits`] allows you to safely control concurrency and memory
//! usage per connection.
//!
//! Defaults are chosen to be safe for most workloads. 
//! Disabling limits ([`Limit::Unlimited`]) should only be done in
//! trusted environments.
//!
//! # Example
//!
//! ```no_run
//! use volga::{App, Limit};
//!
//! App::new()
//!     .with_http2_limits(|limits| limits
//!         .with_max_concurrent_streams(Limit::Limited(100))
//!         .with_max_frame_size(Limit::Unlimited));
//! ```

use crate::App;
use super::Limit;

/// HTTP/2 resource and backpressure limits.
///
/// These limits control protocol-level concurrency and memory usage
/// for HTTP/2 connections.
///
/// Defaults are inherited from the underlying transport implementation
/// and are suitable for most production workloads.
#[derive(Debug, Clone, Copy)]
pub struct Http2Limits {
    /// Maximum number of concurrent streams per connection.
    pub(crate) max_concurrent_streams: Limit<u32>,

    /// Maximum allowed HTTP/2 frame size.
    pub(crate) max_frame_size: Limit<u32>,

    /// Maximum number of pending reset streams.
    pub(crate) max_pending_reset_streams: Limit<usize>,

    /// Maximum number of local reset streams allowed before a `GOAWAY` will be sent.
    pub(crate) max_local_error_reset_streams: Limit<usize>
}

impl Default for Http2Limits {
    #[inline]
    fn default() -> Self {
        Self {
            max_concurrent_streams: Limit::Default,
            max_frame_size: Limit::Default,
            max_pending_reset_streams: Limit::Default,
            max_local_error_reset_streams: Limit::Default
        }
    }
}

impl Http2Limits {
    /// Creates a new [`Http2Limits`] with default values
    #[inline]
    pub fn new() -> Self { 
        Self::default()
    }

    /// Sets the maximum number of concurrent streams per HTTP/2 connection.
    ///
    /// This controls how many simultaneous requests a client can open on a single
    /// HTTP/2 connection. Limiting streams helps prevent resource exhaustion
    /// under high load.
    ///
    /// # Parameters
    /// - `limit` — a [`Limit<u32>`] specifying how the limit is applied:
    ///   - `Limit::Default`: uses framework default (recommended)
    ///   - `Limit::Limited(n)`: enforces an explicit upper bound
    ///   - `Limit::Unlimited`: disables the limit entirely (⚠️ may allow unbounded concurrency)
    ///
    /// # Example
    /// ```no_run
    /// use volga::{limits::Http2Limits, Limit};
    ///
    /// let limits = Http2Limits::new()
    ///     .with_max_concurrent_streams(Limit::Limited(100));
    /// ```
    #[inline]
    pub fn with_max_concurrent_streams(mut self, limit: Limit<u32>) -> Self {
        self.max_concurrent_streams = limit;
        self
    }

    /// Sets the maximum allowed size of an HTTP/2 frame.
    ///
    /// Limits frame size to prevent memory blow-up from very large frames.
    ///
    /// # Parameters
    /// - `limit` — a [`Limit<u32>`]:
    ///   - `Limit::Default`: uses framework default
    ///   - `Limit::Limited(n)`: enforces an explicit upper bound
    ///   - `Limit::Unlimited`: disables the limit (⚠️ may increase memory usage)
    ///
    /// # Example
    /// ```no_run
    /// use volga::{limits::Http2Limits, Limit};
    ///
    /// let limits = Http2Limits::new()
    ///     .with_max_frame_size(Limit::Limited(16 * 1024));
    /// ```
    #[inline]
    pub fn with_max_frame_size(mut self, limit: Limit<u32>) -> Self {
        self.max_frame_size = limit;
        self
    }

    /// Sets the maximum number of pending reset streams.
    ///
    /// Controls how many reset streams may be pending per connection.
    /// Helps manage memory and backpressure for badly-behaved clients.
    ///
    /// # Parameters
    /// - `limit` — a [`Limit<usize>`]:
    ///   - `Limit::Default`: uses framework default
    ///   - `Limit::Limited(n)`: enforces an explicit upper bound
    ///   - `Limit::Unlimited`: disables the limit (⚠️ may increase memory usage)
    ///
    /// # Example
    /// ```no_run
    /// use volga::{limits::Http2Limits, Limit};
    ///
    /// let limits = Http2Limits::new()
    ///     .with_max_pending_reset_streams(Limit::Limited(1024));
    /// ```
    #[inline]
    pub fn with_max_pending_reset_streams(mut self, limit: Limit<usize>) -> Self {
        self.max_pending_reset_streams = limit;
        self
    }

    /// Sets the maximum number of local error reset streams per HTTP/2 connection.
    ///
    /// This limit controls how many streams can be reset due to local errors
    /// simultaneously. It helps prevent excessive memory usage and protects
    /// the server from badly-behaved clients.
    ///
    /// # Parameters
    /// - `limit` — a [`Limit<usize>`]:
    ///   - `Limit::Default`: uses framework default
    ///   - `Limit::Limited(n)`: enforces an explicit upper bound
    ///   - `Limit::Unlimited`: disables the limit (⚠️ may increase memory usage)
    ///
    /// # Example
    /// ```no_run
    /// use volga::{limits::Http2Limits, Limit};
    ///
    /// let limits = Http2Limits::new()
    ///     .with_max_local_error_reset_streams(Limit::Limited(1024));
    /// ```
    #[inline]
    pub fn with_max_local_error_reset_streams(mut self, limit: Limit<usize>) -> Self {
        self.max_local_error_reset_streams = limit;
        self
    }
}

impl App {
    /// Configures HTTP/2-specific limits for the server.
    ///
    /// This method allows you to customize various HTTP/2 limits such as:
    /// - `max_concurrent_streams`
    /// - `max_pending_reset_streams`
    /// - `max_frame_size`
    /// - `max_local_error_reset_streams`
    ///
    /// # Example
    ///
    /// ```rust
    /// use volga::{App, Limit};
    /// 
    /// let app = App::new()
    ///     .with_http2_limits(|limits| limits
    ///         .with_max_concurrent_streams(Limit::Limited(200))
    ///         .with_max_frame_size(Limit::Unlimited)
    ///     );
    /// ```
    pub fn with_http2_limits<F>(mut self, config: F) -> Self
    where
        F: FnOnce(Http2Limits) -> Http2Limits
    {
        self.http2_limits = config(self.http2_limits);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_sets_http2_limits() {
        let app = App::new()
            .with_http2_limits(|limits| limits
                .with_max_concurrent_streams(Limit::Limited(100))
                .with_max_frame_size(Limit::Default)
                .with_max_pending_reset_streams(Limit::Limited(1024))
                .with_max_local_error_reset_streams(Limit::Unlimited));

        assert_eq!(app.http2_limits.max_concurrent_streams, Limit::Limited(100));
        assert_eq!(app.http2_limits.max_pending_reset_streams, Limit::Limited(1024));
        assert_eq!(app.http2_limits.max_frame_size, Limit::Default);
        assert_eq!(app.http2_limits.max_local_error_reset_streams, Limit::Unlimited);
    }

        #[test]
    fn it_creates_and_configures_http2_limits() {
        let limits = Http2Limits::new()
            .with_max_concurrent_streams(Limit::Limited(100))
            .with_max_frame_size(Limit::Default)
            .with_max_pending_reset_streams(Limit::Limited(1024))
            .with_max_local_error_reset_streams(Limit::Unlimited);

        assert_eq!(limits.max_concurrent_streams, Limit::Limited(100));
        assert_eq!(limits.max_pending_reset_streams, Limit::Limited(1024));
        assert_eq!(limits.max_frame_size, Limit::Default);
        assert_eq!(limits.max_local_error_reset_streams, Limit::Unlimited);
    }
}