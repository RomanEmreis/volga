//! Tools and utilities for Rate Limiting

use std::time::Duration;
use crate::App;

pub use volga_rate_limiter::{
    FixedWindowRateLimiter,
    SlidingWindowRateLimiter,
    RateLimiter
};

/// Rate limiting strategy
#[derive(Debug, Clone, Copy)]
pub enum RateLimitingStrategy {
    /// Fixed window rate limiting strategy
    FixedWindow,

    /// Sliding window rate limiting strategy
    SlidingWindow
}

/// Global rate limiter
#[derive(Debug)]
pub struct GlobalRateLimiter {
    fixed_window: Option<FixedWindowRateLimiter>,
    sliding_window: Option<SlidingWindowRateLimiter>
}

impl App {
    /// Sets the fixed window rate limiter
    pub fn with_fixed_window(&mut self, max_requests: u32, window_size: Duration) -> &mut Self {
        //self.rate_limiter.fixed_window = Some(FixedWindowRateLimiter::new(max_requests, window_size));
        self
    }

    /// Sets the sliding window rate limiter
    pub fn with_sliding_window(&mut self, max_requests: u32, window_size: Duration) -> &mut Self {
        //self.rate_limiter.sliding_window = Some(SlidingWindowRateLimiter::new(max_requests, window_size));
        self
    }

    /// Adds the global middleware that limits all requests
    pub fn use_fixed_window(&mut self) -> &mut Self {
        //self.wrap(|ctx, next| async move {
        //    
        //})
        self
    }
}