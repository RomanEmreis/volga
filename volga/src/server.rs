//! HTTP Server tools

use std::sync::Weak;
use std::net::SocketAddr;
use hyper::rt::{Read, Write};
use crate::app::{AppEnv, scope::Scope};

#[cfg(all(feature = "http1", not(feature = "http2")))]
pub(super) mod http1;
#[cfg(any(
    all(feature = "http1", feature = "http2"),
    all(feature = "http2", not(feature = "http1"))
))]
pub(super) mod http2;

pub(super) struct Server<I: Read + Write + Unpin> {
    io: I,
    peer_addr: SocketAddr,
}

impl<I: Send + Read + Write + Unpin + 'static> Server<I> {
    #[inline]
    pub(super) fn new(io: I, peer_addr: SocketAddr) -> Self {
        Self { io, peer_addr }
    }

    #[inline]
    pub(super) async fn serve(self, env: Weak<AppEnv>) {
        if let Some(instance) = env.upgrade() {
            let scope = Scope::new(env, self.peer_addr);
            self.serve_core(scope, instance).await;
        } else {
            #[cfg(feature = "tracing")]
            tracing::warn!("app instance could not be upgraded; aborting...");
        }
    }
}
