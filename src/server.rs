//! HTTP Server tools

use std::sync::Weak;
use hyper::rt::{Read, Write};
use crate::app::{AppInstance, scope::Scope};

#[cfg(all(feature = "http1", not(feature = "http2")))]
pub(super) mod http1;
#[cfg(any(
    all(feature = "http1", feature = "http2"),
    all(feature = "http2", not(feature = "http1"))
))]
pub(super) mod http2;

pub(super) struct Server<I: Read + Write + Unpin> {
    io: I
}

impl<I: Send + Read + Write + Unpin + 'static> Server<I> {
    #[inline]
    pub(super) fn new(io: I) -> Self {
        Self { io }
    }

    #[inline]
    pub(super) async fn serve(self, app_instance: Weak<AppInstance>) {
        if let Some(instance) = app_instance.upgrade() {
            let scope = Scope::new(app_instance);
            self.serve_core(scope, instance).await;
        } else {
            #[cfg(feature = "tracing")]
            tracing::warn!("app instance could not be upgraded; aborting...");
        }
    }
}

