//! Programmatic shutdown handle for [`crate::App`].
//!
//! Wraps a [`tokio_util::sync::CancellationToken`] so callers can trigger
//! a graceful server shutdown without sending an OS signal.

use std::future::Future;

use tokio_util::sync::CancellationToken;

/// A handle that triggers a graceful shutdown of running [`crate::App`].
///
/// Clones share the same shutdown signal — any clone calling
/// [`ShutdownHandle::shutdown`] cancels the shared token.
#[derive(Debug, Clone, Default)]
pub struct ShutdownHandle {
    token: CancellationToken,
}

impl ShutdownHandle {
    /// Creates a new handle backed by a fresh [`CancellationToken`].
    pub fn new() -> Self {
        Self {
            token: CancellationToken::new(),
        }
    }

    /// Wraps an existing [`CancellationToken`].
    ///
    /// Useful for sharing a single shutdown signal with other subsystems
    /// that already use a `CancellationToken`.
    pub fn from_token(token: CancellationToken) -> Self {
        Self { token }
    }

    /// Triggers a graceful shutdown of the associated server.
    ///
    /// Idempotent — repeated calls are no-ops. The server will stop
    /// accepting new connections and drain in-flight requests up to
    /// the configured graceful-shutdown timeout.
    pub fn shutdown(&self) {
        self.token.cancel();
    }

    /// Returns `true` if a shutdown has been requested.
    ///
    /// Note this reports only that the trigger fired — the server may
    /// still be draining in-flight requests.
    pub fn is_shutdown_requested(&self) -> bool {
        self.token.is_cancelled()
    }

    /// Returns a `'static` future that resolves when shutdown has been
    /// requested. Suitable for passing to [`tokio::spawn`] without
    /// cloning the handle.
    pub fn cancelled(&self) -> impl Future<Output = ()> + Send + 'static {
        self.token.clone().cancelled_owned()
    }

    /// Returns a clone of the underlying [`CancellationToken`] for
    /// interop with the `tokio-util` ecosystem.
    pub fn token(&self) -> CancellationToken {
        self.token.clone()
    }
}

impl From<CancellationToken> for ShutdownHandle {
    fn from(token: CancellationToken) -> Self {
        Self::from_token(token)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_starts_in_not_shutdown_state() {
        let handle = ShutdownHandle::new();
        assert!(!handle.is_shutdown_requested());
    }

    #[test]
    fn it_reports_shutdown_after_trigger() {
        let handle = ShutdownHandle::new();
        handle.shutdown();
        assert!(handle.is_shutdown_requested());
    }

    #[test]
    fn it_is_idempotent_on_repeated_shutdown() {
        let handle = ShutdownHandle::new();
        handle.shutdown();
        handle.shutdown();
        assert!(handle.is_shutdown_requested());
    }

    #[test]
    fn it_shares_state_across_clones() {
        let original = ShutdownHandle::new();
        let cloned = original.clone();
        cloned.shutdown();
        assert!(original.is_shutdown_requested());
    }

    #[tokio::test]
    async fn it_resolves_cancelled_after_shutdown() {
        let handle = ShutdownHandle::new();
        let waiter = handle.clone();
        let task = tokio::spawn(async move { waiter.cancelled().await });
        handle.shutdown();
        task.await.unwrap();
    }

    #[test]
    fn it_returns_a_clone_of_the_underlying_token() {
        let handle = ShutdownHandle::new();
        let token = handle.token();
        token.cancel();
        assert!(handle.is_shutdown_requested());
    }

    #[test]
    fn it_constructs_from_existing_token() {
        let token = CancellationToken::new();
        let handle = ShutdownHandle::from_token(token.clone());
        token.cancel();
        assert!(handle.is_shutdown_requested());
    }

    #[test]
    fn it_constructs_via_from_impl() {
        let token = CancellationToken::new();
        let handle: ShutdownHandle = token.clone().into();
        token.cancel();
        assert!(handle.is_shutdown_requested());
    }

    #[test]
    fn it_yields_a_fresh_handle_when_defaulted() {
        let handle = ShutdownHandle::default();
        assert!(!handle.is_shutdown_requested());
    }
}
