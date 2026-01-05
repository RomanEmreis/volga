use super::Server;
use crate::app::{AppInstance, scope::Scope};
use std::sync::Arc;
use hyper::rt::{Read, Write};
use hyper_util::rt::TokioExecutor;

#[cfg(feature = "ws")]
use hyper_util::server::conn::auto::Builder;

#[cfg(not(feature = "ws"))]
use hyper::server::conn::http2::Builder;

/// HTTP/2 impl
impl<I: Send + Read + Write + Unpin + 'static> Server<I> {
    #[inline]
    pub(super) async fn serve_core(self, scope: Scope, app_instance: Arc<AppInstance>) {
        let scoped_cancellation_token = scope.cancellation_token.clone();

        #[cfg(feature = "ws")]
        {
            let mut connection_builder = Builder::new(TokioExecutor::new());
            connection_builder.http2().enable_connect_protocol();
            
            if let Some(max_header_size) = app_instance.max_header_size {
                connection_builder.http2().max_header_list_size(max_header_size as u32);
            }
            
            let connection = connection_builder.serve_connection_with_upgrades(self.io, scope);
            let connection = app_instance.graceful_shutdown.watch(connection);
            
            drop(app_instance);

            if let Err(_err) = connection.await {
                #[cfg(feature = "tracing")]
                tracing::error!("error serving connection: {_err:#}");
                scoped_cancellation_token.cancel();
            }
        }
        #[cfg(not(feature = "ws"))]
        {
            let mut connection_builder = Builder::new(TokioExecutor::new());
            if let Some(max_header_size) = app_instance.max_header_size {
                connection_builder.max_header_list_size(max_header_size as u32);
            }

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
}