//! Programmatic shutdown handle for [`crate::App`].
//!
//! Wraps a [`tokio_util::sync::CancellationToken`] so callers can trigger
//! a graceful server shutdown without sending an OS signal.

use std::fmt;
use std::future::Future;
use std::pin::Pin;

use tokio_util::sync::CancellationToken;

type PendingFuture = Pin<Box<dyn Future<Output = ()> + Send + 'static>>;

/// A handle that triggers a graceful shutdown of running [`crate::App`].
///
/// Clones share the same shutdown signal — any clone calling
/// [`ShutdownHandle::shutdown`] cancels the shared token. Each clone
/// carries its own empty trigger list, so register triggers via
/// [`ShutdownHandle::shutdown_on`] (or [`ShutdownHandle::on_signal`])
/// *before* passing the handle to [`crate::App::with_shutdown_signal`].
#[derive(Default)]
pub struct ShutdownHandle {
    token: CancellationToken,
    /// Async triggers registered while no Tokio runtime was active.
    /// Spawned by [`ShutdownHandle::arm_pending`] once a runtime is available.
    pending: Vec<PendingFuture>,
}

impl Clone for ShutdownHandle {
    fn clone(&self) -> Self {
        Self {
            token: self.token.clone(),
            pending: Vec::new(),
        }
    }
}

impl fmt::Debug for ShutdownHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ShutdownHandle")
            .field("token", &self.token)
            .field("pending", &self.pending.len())
            .finish()
    }
}

impl ShutdownHandle {
    /// Creates a new handle backed by a fresh [`CancellationToken`].
    pub fn new() -> Self {
        Self {
            token: CancellationToken::new(),
            pending: Vec::new(),
        }
    }

    /// Wraps an existing [`CancellationToken`].
    ///
    /// Useful for sharing a single shutdown signal with other subsystems
    /// that already use a `CancellationToken`.
    pub fn from_token(token: CancellationToken) -> Self {
        Self {
            token,
            pending: Vec::new(),
        }
    }

    /// Builds a handle whose shutdown is triggered when the given
    /// future resolves. Spawns a background task internally.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use volga::ShutdownHandle;
    ///
    /// let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    /// let handle = ShutdownHandle::on_signal(async move {
    ///     let _ = rx.await;
    /// });
    /// // Sending on `tx` later triggers shutdown.
    /// # let _ = tx;
    /// # let _ = handle;
    /// ```
    pub fn on_signal<F>(future: F) -> Self
    where
        F: Future<Output = ()> + Send + 'static,
    {
        let mut handle = Self::new();
        handle.shutdown_on(future);
        handle
    }

    /// Adds an additional async trigger to an existing handle.
    ///
    /// Multiple `shutdown_on` calls compose — any of the futures
    /// resolving will trigger shutdown. The underlying [`CancellationToken::cancel`]
    /// is idempotent, so triggers that fire after shutdown was already
    /// requested are no-ops.
    ///
    /// Safe to call outside a Tokio runtime: when no runtime is active
    /// the future is queued and spawned automatically once [`crate::App::run`]
    /// (or [`crate::App::run_blocking`]) enters async context. Register
    /// triggers *before* cloning or moving the handle into the app —
    /// each clone starts with an empty trigger list.
    pub fn shutdown_on<F>(&mut self, future: F)
    where
        F: Future<Output = ()> + Send + 'static,
    {
        if tokio::runtime::Handle::try_current().is_ok() {
            let token = self.token.clone();
            tokio::spawn(async move {
                future.await;
                token.cancel();
            });
        } else {
            self.pending.push(Box::pin(future));
        }
    }

    /// Spawns any futures registered via [`ShutdownHandle::shutdown_on`]
    /// while no Tokio runtime was active. Must be called from within a
    /// runtime; called once by [`crate::App::run`] before the shutdown
    /// signal task is spawned.
    pub(crate) fn arm_pending(&mut self) {
        for future in std::mem::take(&mut self.pending) {
            let token = self.token.clone();
            tokio::spawn(async move {
                future.await;
                token.cancel();
            });
        }
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

    #[tokio::test]
    async fn it_triggers_shutdown_when_on_signal_future_resolves() {
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        let handle = ShutdownHandle::on_signal(async move {
            let _ = rx.await;
        });
        assert!(!handle.is_shutdown_requested());
        tx.send(()).unwrap();
        handle.cancelled().await;
        assert!(handle.is_shutdown_requested());
    }

    #[tokio::test]
    async fn it_composes_multiple_shutdown_on_triggers() {
        let mut handle = ShutdownHandle::new();
        let (tx_a, rx_a) = tokio::sync::oneshot::channel::<()>();
        let (_tx_b, rx_b) = tokio::sync::oneshot::channel::<()>();
        handle.shutdown_on(async move {
            let _ = rx_a.await;
        });
        handle.shutdown_on(async move {
            let _ = rx_b.await;
        });
        assert!(!handle.is_shutdown_requested());
        // Firing only one trigger is enough.
        tx_a.send(()).unwrap();
        handle.cancelled().await;
        assert!(handle.is_shutdown_requested());
    }

    #[tokio::test]
    async fn it_treats_shutdown_on_after_cancel_as_noop() {
        let mut handle = ShutdownHandle::new();
        handle.shutdown();
        // The trigger future is allowed to run but cancel() is a no-op.
        handle.shutdown_on(async {});
        // Yield to let the spawned task execute and call cancel() on the already-cancelled token.
        tokio::task::yield_now().await;
        assert!(handle.is_shutdown_requested());
    }

    #[test]
    fn it_does_not_panic_when_shutdown_on_is_called_without_runtime() {
        let mut handle = ShutdownHandle::new();
        handle.shutdown_on(async {});
        assert!(!handle.is_shutdown_requested());
    }

    #[test]
    fn it_does_not_panic_when_on_signal_is_called_without_runtime() {
        let _handle = ShutdownHandle::on_signal(async {});
    }

    #[test]
    fn it_arms_pending_triggers_once_runtime_starts() {
        let mut handle = ShutdownHandle::new();
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        // Registered outside a runtime — must be queued.
        handle.shutdown_on(async move {
            let _ = rx.await;
        });
        assert!(!handle.is_shutdown_requested());

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        rt.block_on(async {
            handle.arm_pending();
            tx.send(()).unwrap();
            handle.cancelled().await;
        });
        assert!(handle.is_shutdown_requested());
    }
}
