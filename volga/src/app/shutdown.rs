//! Programmatic shutdown handle for [`crate::App`].
//!
//! Wraps a [`tokio_util::sync::CancellationToken`] so callers can trigger
//! a graceful server shutdown without sending an OS signal.

use std::{future::Future, time::Duration};

use hyper_util::server::graceful::GracefulShutdown;
use tokio_util::sync::CancellationToken;

/// How long a server that stopped accepting connections waits for the open ones to finish
/// before it closes them, unless [`crate::App::with_shutdown_timeout`] says otherwise
pub(crate) const DEFAULT_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(10);

/// A handle that triggers a graceful shutdown of running [`crate::App`].
///
/// Clones share the same shutdown signal - any clone calling
/// [`ShutdownHandle::shutdown`] cancels the shared token.
///
/// It is also an extractor: a handler or middleware that takes a `ShutdownHandle` gets the
/// running server's handle, whether or not the app was given one with
/// [`crate::App::with_shutdown`]. Its [`cancelled`](Self::cancelled) future resolves as soon
/// as the shutdown starts, which is what a response that never ends on its own - an SSE feed,
/// a proxied stream - needs to end itself, instead of holding the shutdown up until
/// [`crate::App::with_shutdown_timeout`] runs out and the connection is closed under it:
///
/// ```no_run
/// use std::time::Duration;
/// use futures_util::StreamExt;
/// use volga::{App, ShutdownHandle, http::sse::{Message, SseStream}, sse_stream};
///
/// # #[tokio::main]
/// # async fn main() -> std::io::Result<()> {
/// let mut app = App::new();
///
/// app.map_get("/events", |shutdown: ShutdownHandle| async move {
///     let events = sse_stream! {
///         loop {
///             yield Message::new().data("tick");
///             tokio::time::sleep(Duration::from_secs(1)).await;
///         }
///     };
///     SseStream::new(events.take_until(shutdown.cancelled()))
/// });
/// # app.run().await
/// # }
/// ```
///
/// This is a different signal from the request's [`crate::CancellationToken`], which says the
/// response is no longer wanted - the client went away, or the shutdown ran out of time and
/// the connection is being closed. A shutdown that has just started still waits for requests
/// in flight to be answered, so a handler cancelling its work on it would fail them for no
/// reason.
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
    /// Idempotent - repeated calls are no-ops. The server will stop
    /// accepting new connections and drain in-flight requests up to
    /// [`crate::App::with_shutdown_timeout`], then close the connections
    /// still open.
    pub fn shutdown(&self) {
        self.token.cancel();
    }

    /// Returns `true` if a shutdown has been requested.
    ///
    /// Note this reports only that the trigger fired - the server may
    /// still be draining in-flight requests.
    pub fn is_shutdown_requested(&self) -> bool {
        self.token.is_cancelled()
    }

    /// Returns a `'static` future that resolves when shutdown has been
    /// requested. Suitable for passing to [`tokio::spawn`] without
    /// cloning the handle.
    pub fn cancelled(&self) -> impl Future<Output = ()> + Send + 'static + use<> {
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

/// Signals every connection `graceful_shutdown` watches to finish what it is serving and close,
/// then waits until they have. What is still open once `timeout` runs out is closed: `force_close`
/// is cancelled - the token each connection, and each request's [`crate::CancellationToken`], is
/// a child of - and the connections, which select on it, drop what they were serving.
///
/// Returns `true` if every connection closed on its own in time.
pub(crate) async fn drain_connections(
    graceful_shutdown: GracefulShutdown,
    timeout: Duration,
    force_close: &CancellationToken,
) -> bool {
    let drained = graceful_shutdown.shutdown();
    tokio::pin!(drained);

    if tokio::time::timeout(timeout, &mut drained).await.is_ok() {
        return true;
    }

    force_close.cancel();
    // A connection lets go of its watcher as soon as its task is polled again,
    // so this is what makes `run` return with nothing of it still running
    drained.await;
    false
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
