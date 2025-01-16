use super::Server;

#[cfg(feature = "tls")]
use super::TlsServer;

use crate::app::scope::Scope;
use hyper::server::conn::http2;
use hyper_util::rt::{TokioExecutor, TokioIo};
use tokio::net::TcpStream;

#[cfg(feature = "tls")]
use tokio_rustls::server::TlsStream;

impl Server {
    #[inline]
    pub(crate) fn new(io: TokioIo<TcpStream>) -> Self {
        Self { io }
    }

    #[inline]
    pub(crate) async fn serve(self, scope: Scope) {
        let scoped_cancellation_token = scope.cancellation_token.clone();
        let connection_builder = http2::Builder::new(TokioExecutor::new());
        let connection = connection_builder.serve_connection(self.io, scope);
        if let Err(err) = connection.await {
            eprintln!("Error serving connection: {:?}", err);
            scoped_cancellation_token.cancel();
        }
    }
}

#[cfg(feature = "tls")]
impl TlsServer {
    #[inline]
    pub(crate) fn new(io: TokioIo<TlsStream<TcpStream>>) -> Self {
        Self { io }
    }

    #[inline]
    pub(crate) async fn serve(self, scope: Scope) {
        let scoped_cancellation_token = scope.cancellation_token.clone();
        let connection_builder = http2::Builder::new(TokioExecutor::new());
        let connection = connection_builder.serve_connection(self.io, scope);
        if let Err(err) = connection.await {
            eprintln!("Error serving connection: {:?}", err);
            scoped_cancellation_token.cancel();
        }
    }
}