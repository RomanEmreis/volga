//! HTTP Server tools

use crate::app::{AppEnv, scope::Scope};
use hyper::rt::{Read, Write};
use hyper_util::server::graceful::Watcher;
use std::net::SocketAddr;
use std::sync::Weak;
use tokio_util::sync::CancellationToken;

pub(super) use linger::LingeringStream;

#[cfg(all(feature = "http1", not(feature = "http2")))]
pub(super) mod http1;
#[cfg(any(
    all(feature = "http1", feature = "http2"),
    all(feature = "http2", not(feature = "http1"))
))]
pub(super) mod http2;
mod linger;

pub(super) struct Server<I: Read + Write + Unpin> {
    io: I,
    peer_addr: SocketAddr,
}

/// Logs the error a connection ended with.
///
/// A client that did not send the head of a request within the header read timeout is
/// routine - every HTTP/1 keep-alive connection left idle that long ends that way - so it is
/// logged at `debug` rather than `error`.
#[cfg(feature = "tracing")]
fn log_connection_error(err: &(dyn std::error::Error + 'static)) {
    if is_timeout(err) {
        tracing::debug!("connection closed: {err:#}");
    } else {
        tracing::error!("error serving connection: {err:#}");
    }
}

/// Whether `err`, or an error it was caused by, is a timeout reported by hyper
#[cfg(feature = "tracing")]
fn is_timeout(err: &(dyn std::error::Error + 'static)) -> bool {
    std::iter::successors(Some(err), |err| err.source())
        .filter_map(|err| err.downcast_ref::<hyper::Error>())
        .any(hyper::Error::is_timeout)
}

impl<I: Send + Read + Write + Unpin + 'static> Server<I> {
    #[inline]
    pub(super) fn new(io: I, peer_addr: SocketAddr) -> Self {
        Self { io, peer_addr }
    }

    #[inline]
    pub(super) async fn serve(
        self,
        env: Weak<AppEnv>,
        watcher: Watcher,
        cancellation_token: CancellationToken,
    ) {
        if let Some(instance) = env.upgrade() {
            let scope = Scope::new(&instance, self.peer_addr, cancellation_token);
            self.serve_core(scope, instance, watcher).await;
        } else {
            #[cfg(feature = "tracing")]
            tracing::warn!("app instance could not be upgraded; aborting...");
        }
    }
}
