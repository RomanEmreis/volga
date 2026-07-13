//! Client configuration
//!
//! [`ClientConfig`] carries the transport-level policy shared by all client
//! operations (discovery, token requests, registration): HTTPS enforcement,
//! timeouts and redirect limits. The defaults are safe for production use.

use std::time::Duration;

/// Default total timeout for a single client request
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// Default maximum number of redirects followed per request
pub const DEFAULT_MAX_REDIRECTS: u8 = 5;

/// Transport-level configuration for OAuth client operations
///
/// # Example
/// ```
/// use std::time::Duration;
/// use volga_oauth_client::ClientConfig;
///
/// let config = ClientConfig::new()
///     .with_timeout(Duration::from_secs(5))
///     .with_max_redirects(0);
///
/// assert!(config.enforce_https());
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClientConfig {
    enforce_https: bool,
    timeout: Duration,
    max_redirects: u8,
}

impl Default for ClientConfig {
    #[inline]
    fn default() -> Self {
        Self {
            enforce_https: true,
            timeout: DEFAULT_TIMEOUT,
            max_redirects: DEFAULT_MAX_REDIRECTS,
        }
    }
}

impl ClientConfig {
    /// Creates a configuration with the default policy: HTTPS enforced,
    /// [`DEFAULT_TIMEOUT`] and [`DEFAULT_MAX_REDIRECTS`]
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    /// Controls whether plain `http://` URLs are rejected
    ///
    /// Enabled by default. Disable only for local development against a
    /// plaintext authorization server; requests to `http://` endpoints
    /// otherwise fail with
    /// [`ClientError::InsecureUrl`](crate::ClientError::InsecureUrl).
    #[inline]
    pub fn require_https(mut self, required: bool) -> Self {
        self.enforce_https = required;
        self
    }

    /// Sets the total timeout for a single request
    #[inline]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Sets the maximum number of redirects followed per request;
    /// `0` disables redirects entirely
    #[inline]
    pub fn with_max_redirects(mut self, max_redirects: u8) -> Self {
        self.max_redirects = max_redirects;
        self
    }

    /// Returns whether plain `http://` URLs are rejected
    #[inline]
    pub fn enforce_https(&self) -> bool {
        self.enforce_https
    }

    /// Returns the total timeout for a single request
    #[inline]
    pub fn timeout(&self) -> Duration {
        self.timeout
    }

    /// Returns the maximum number of redirects followed per request
    #[inline]
    pub fn max_redirects(&self) -> u8 {
        self.max_redirects
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_defaults_to_safe_policy() {
        let config = ClientConfig::new();
        assert!(config.enforce_https());
        assert_eq!(config.timeout(), DEFAULT_TIMEOUT);
        assert_eq!(config.max_redirects(), DEFAULT_MAX_REDIRECTS);
    }

    #[test]
    fn it_builds_custom_policy() {
        let config = ClientConfig::new()
            .require_https(false)
            .with_timeout(Duration::from_secs(5))
            .with_max_redirects(0);
        assert!(!config.enforce_https());
        assert_eq!(config.timeout(), Duration::from_secs(5));
        assert_eq!(config.max_redirects(), 0);
    }
}
