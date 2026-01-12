use super::Server;
use crate::Limit;
use crate::app::{AppInstance, scope::Scope};
use std::sync::Arc;
use hyper::rt::{Read, Write};

#[cfg(feature = "ws")]
use hyper_util::{
    rt::TokioExecutor, 
    server::conn::auto::Builder
};

#[cfg(not(feature = "ws"))]
use hyper::server::conn::http1::Builder;

/// HTTP/1 impl
impl<I: Send + Read + Write + Unpin + 'static> Server<I> {
    #[inline]
    pub(super) async fn serve_core(self, scope: Scope, app_instance: Arc<AppInstance>) {
        let scoped_cancellation_token = scope.cancellation_token.clone();

        #[cfg(feature = "ws")]
        {
            let mut connection_builder = Builder::new(TokioExecutor::new());
            if let Limit::Limited(max_header_count) = app_instance.max_header_count {
                connection_builder.http1().max_headers(max_header_count);
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
            let mut connection_builder = Builder::new();
            if let Limit::Limited(max_header_count) = app_instance.max_header_count {
                connection_builder.max_headers(max_header_count);
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
