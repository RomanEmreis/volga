﻿use super::Server;
use crate::app::{AppInstance, scope::Scope};
use std::sync::Arc;
use hyper::{server::conn::http1, rt::{Read, Write}};

/// HTTP/1 impl
impl<I: Read + Write + Unpin + 'static> Server<I> {
    #[inline]
    pub(super) async fn serve_core(self, scope: Scope, app_instance: Arc<AppInstance>) {
        let scoped_cancellation_token = scope.cancellation_token.clone();
        
        let connection_builder = http1::Builder::new();
        let connection = connection_builder.serve_connection(self.io, scope);
        let connection = app_instance.graceful_shutdown.watch(connection);
        
        drop(app_instance);
        
        if let Err(_err) = connection.await {
            #[cfg(feature = "tracing")]
            tracing::error!("error serving connection: {_err:#}");
            scoped_cancellation_token.cancel();
        }
    }
}
