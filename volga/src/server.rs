//! HTTP Server tools

use crate::app::{AppEnv, scope::Scope};
use hyper_util::{rt::TokioIo, server::graceful::Watcher};
use std::net::SocketAddr;
use std::sync::Weak;
use tokio::io::{AsyncRead, AsyncWrite};
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
// The builds that serve through `hyper_util::server::conn::auto` or the HTTP/2 builder, where
// the first bytes of a connection are read before hyper's HTTP/1 timer runs, and that have a
// header read timeout to bound them with. A build of HTTP/1 alone serves through the HTTP/1
// builder, whose timer runs from the first byte
#[cfg(all(
    any(feature = "http1", feature = "tls"),
    any(feature = "ws", feature = "http2")
))]
mod preface;

/// A connection to serve: a Tokio stream before [`Server::serve`], and the hyper I/O over it
/// that `serve_core` hands to hyper
pub(super) struct Server<I> {
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

/// Bounds the bytes that tell HTTP/1 from HTTP/2 with the header read timeout: see `preface`
#[inline]
#[cfg(all(
    any(feature = "http1", feature = "tls"),
    any(feature = "ws", feature = "http2")
))]
fn bound_preface<S>(io: S, env: &AppEnv) -> preface::PrefaceDeadline<S> {
    preface::PrefaceDeadline::new(io, env.header_read_timeout)
}

/// Leaves the stream as it is: hyper's HTTP/1 timer runs from the first byte, or the build has
/// no header read timeout
#[inline]
#[cfg(not(all(
    any(feature = "http1", feature = "tls"),
    any(feature = "ws", feature = "http2")
)))]
fn bound_preface<S>(io: S, _env: &AppEnv) -> S {
    io
}

/// Whether `err`, or an error it was caused by, is a timeout: hyper's header read timeout,
/// or the deadline on telling the protocol (`preface`)
#[cfg(feature = "tracing")]
fn is_timeout(err: &(dyn std::error::Error + 'static)) -> bool {
    std::iter::successors(Some(err), |err| err.source()).any(|err| {
        err.downcast_ref::<hyper::Error>()
            .is_some_and(hyper::Error::is_timeout)
            || err
                .downcast_ref::<std::io::Error>()
                .is_some_and(|err| err.kind() == std::io::ErrorKind::TimedOut)
    })
}

impl<S: AsyncRead + AsyncWrite + Send + Unpin + 'static> Server<S> {
    #[inline]
    pub(super) fn new(io: S, peer_addr: SocketAddr) -> Self {
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

            Server {
                io: TokioIo::new(bound_preface(self.io, &instance)),
                peer_addr: self.peer_addr,
            }
            .serve_core(scope, instance, watcher)
            .await;
        } else {
            #[cfg(feature = "tracing")]
            tracing::warn!("app instance could not be upgraded; aborting...");
        }
    }
}
