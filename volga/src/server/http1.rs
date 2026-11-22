use super::Server;
use crate::Limit;
use crate::app::{AppEnv, scope::Scope};
use hyper::rt::{Read, Write};
use hyper_util::{rt::TokioTimer, server::graceful::Watcher};
use std::sync::Arc;

#[cfg(feature = "ws")]
use hyper_util::{rt::TokioExecutor, server::conn::auto::Builder};

#[cfg(not(feature = "ws"))]
use hyper::server::conn::http1::Builder;

/// HTTP/1 impl
impl<I: Send + Read + Write + Unpin + 'static> Server<I> {
    #[inline]
    pub(super) async fn serve_core(self, scope: Scope, env: Arc<AppEnv>, watcher: Watcher) {
        let scoped_cancellation_token = scope.cancellation_token.clone();

        #[cfg(feature = "ws")]
        {
            let mut connection_builder = Builder::new(TokioExecutor::new());

            connection_builder
                .http1()
                .timer(TokioTimer::new())
                .header_read_timeout(env.header_read_timeout);

            if let Limit::Limited(max_header_count) = env.max_header_count {
                connection_builder.http1().max_headers(max_header_count);
            }

            let connection = connection_builder.serve_connection_with_upgrades(self.io, scope);
            let connection = watcher.watch(connection);

            drop(env);

            if let Err(_err) = connection.await {
                #[cfg(feature = "tracing")]
                super::log_connection_error(&*_err);
                scoped_cancellation_token.cancel();
            }
        }
        #[cfg(not(feature = "ws"))]
        {
            let mut connection_builder = Builder::new();

            connection_builder
                .timer(TokioTimer::new())
                .header_read_timeout(env.header_read_timeout);

            if let Limit::Limited(max_header_count) = env.max_header_count {
                connection_builder.max_headers(max_header_count);
            }

            let connection = connection_builder.serve_connection(self.io, scope);
            let connection = watcher.watch(connection);

            drop(env);

            if let Err(_err) = connection.await {
                #[cfg(feature = "tracing")]
                super::log_connection_error(&_err);
                scoped_cancellation_token.cancel();
            }
        }
    }
}
