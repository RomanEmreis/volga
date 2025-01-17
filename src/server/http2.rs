﻿use super::Server;
use crate::app::scope::Scope;
use hyper::{server::conn::http2, rt::{Read, Write}};
use hyper_util::rt::TokioExecutor;

impl<I: Read + Write + Unpin> Server<I> {
    #[inline]
    pub(crate) fn new(io: I) -> Self {
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