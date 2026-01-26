use super::Server;
use crate::limits::{Limit, Http2Limits};
use crate::app::{AppEnv, scope::Scope};
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
    pub(super) async fn serve_core(self, scope: Scope, env: Arc<AppEnv>) {
        let scoped_cancellation_token = scope.cancellation_token.clone();

        #[cfg(feature = "ws")]
        {
            let mut connection_builder = Builder::new(TokioExecutor::new());

            let http2_builder = &mut connection_builder.http2();
            http2_builder.enable_connect_protocol();
            
            if let Limit::Limited(max_header_size) = env.max_header_size {
                http2_builder.max_header_list_size(max_header_size as u32);
            }
            
            configure_http2(http2_builder, env.http2_limits);

            let connection = connection_builder.serve_connection_with_upgrades(self.io, scope);
            let connection = env.graceful_shutdown.watch(connection);
            
            drop(env);

            if let Err(_err) = connection.await {
                #[cfg(feature = "tracing")]
                tracing::error!("error serving connection: {_err:#}");
                scoped_cancellation_token.cancel();
            }
        }
        #[cfg(not(feature = "ws"))]
        {
            let mut connection_builder = Builder::new(TokioExecutor::new());
            if let Limit::Limited(max_header_size) = env.max_header_size {
                connection_builder.max_header_list_size(max_header_size as u32);
            }

            configure_http2(&mut connection_builder, env.http2_limits);

            let connection = connection_builder.serve_connection(self.io, scope);
            let connection = env.graceful_shutdown.watch(connection);

            drop(env);

            if let Err(_err) = connection.await {
                #[cfg(feature = "tracing")]
                tracing::error!("error serving connection: {_err:#}");
                scoped_cancellation_token.cancel();
            }   
        }
    }
}

#[inline]
#[cfg(feature = "ws")]
fn configure_http2<E>(
    builder: &mut hyper_util::server::conn::auto::Http2Builder<'_, E>,
    limits: Http2Limits,
) {
    match limits.max_concurrent_streams {
        Limit::Limited(limit) => builder.max_concurrent_streams(limit),
        Limit::Unlimited => builder.max_concurrent_streams(None),
        _ => builder
    };

    match limits.max_frame_size {
        Limit::Limited(limit) => builder.max_frame_size(limit),
        Limit::Unlimited => builder.max_frame_size(None),
        _ => builder
    };

    match limits.max_local_error_reset_streams {
        Limit::Limited(limit) => builder.max_local_error_reset_streams(limit),
        Limit::Unlimited => builder.max_local_error_reset_streams(None),
        _ => builder
    };

    match limits.max_pending_reset_streams {
        Limit::Limited(limit) => builder.max_pending_accept_reset_streams(limit),
        Limit::Unlimited => builder.max_pending_accept_reset_streams(None),
        _ => builder
    };
}

#[inline]
#[cfg(not(feature = "ws"))]
fn configure_http2<E>(
    builder: &mut Builder<E>,
    limits: Http2Limits,
) {
    match limits.max_concurrent_streams {
        Limit::Limited(limit) => builder.max_concurrent_streams(limit),
        Limit::Unlimited => builder.max_concurrent_streams(None),
        _ => builder
    };

    match limits.max_frame_size {
        Limit::Limited(limit) => builder.max_frame_size(limit),
        Limit::Unlimited => builder.max_frame_size(None),
        _ => builder
    };

    match limits.max_local_error_reset_streams {
        Limit::Limited(limit) => builder.max_local_error_reset_streams(limit),
        Limit::Unlimited => builder.max_local_error_reset_streams(None),
        _ => builder
    };

    match limits.max_pending_reset_streams {
        Limit::Limited(limit) => builder.max_pending_accept_reset_streams(limit),
        Limit::Unlimited => builder.max_pending_accept_reset_streams(None),
        _ => builder
    };
}