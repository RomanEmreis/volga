﻿//! Utilities for managing HTTP request scope

use crate::http::endpoints::{
    handlers::RouteHandler,
    args::FromRequestRef
};

use crate::{
    error::Error,
    headers::{Header, FromHeaders},
    HttpRequest, 
    HttpResult
};

#[cfg(any(feature = "tls", feature = "tracing"))]
use crate::error::handler::WeakErrorHandler;

#[cfg(feature = "di")]
use crate::di::Inject;
#[cfg(feature = "di")]
use std::sync::Arc;

/// Describes current HTTP context which consists of the current HTTP request data 
/// and the reference to the method handler fot this request
pub struct HttpContext {
    /// Current HTTP request
    pub request: HttpRequest,
    /// Current handler that mapped to handle the HTTP request
    handler: RouteHandler,
}

impl HttpContext {
    /// Creates a new [`HttpContext`]
    #[inline]
    pub(crate) fn new(
        request: HttpRequest,
        handler: RouteHandler
    ) -> Self {
        Self { request, handler }
    }
    
    #[inline]
    #[allow(dead_code)]
    pub(super) fn into_parts(self) -> (HttpRequest, RouteHandler) {
        (self.request, self.handler)
    }
    
    /// Extracts a payload from request parts
    ///
    /// # Example
    /// ```no_run
    /// use volga::{middleware::HttpContext, Query};
    /// use serde::Deserialize;
    ///
    /// #[derive(Deserialize)]
    /// struct Params {
    ///     id: u32,
    ///     key: String
    /// }
    /// 
    /// # fn docs(ctx: HttpContext) -> std::io::Result<()> {
    /// let params: Query<Params> = ctx.extract()?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline]
    pub fn extract<T: FromRequestRef>(&self) -> Result<T, Error> {
        self.request.extract()
    }

    /// Resolves a service from Dependency Container as a clone, service must implement [`Clone`]
    #[inline]
    #[cfg(feature = "di")]
    pub fn resolve<T: Inject + Clone + 'static>(&self) -> Result<T, Error> {
        self.request.resolve::<T>()
    }

    /// Resolves a service from Dependency Container
    #[inline]
    #[cfg(feature = "di")]
    pub fn resolve_shared<T: Inject + 'static>(&self) -> Result<Arc<T>, Error> {
        self.request.resolve_shared::<T>()
    }
    
    /// Inserts the [`Header<T>`] to HTTP request headers
    #[inline]
    pub fn insert_header<T: FromHeaders>(&mut self, header: Header<T>) {
        self.request.insert_header(header)
    }

    /// Executes the request handler for current HTTP request
    #[inline]
    pub(crate) async fn execute(self) -> HttpResult {
        self.handler.call(self.request).await
    }
    
    /// Returns a weak reference to global error handler
    #[inline]
    #[cfg(any(feature = "tls", feature = "tracing"))]
    pub(crate) fn error_handler(&self) -> WeakErrorHandler {
        self.request
            .extensions()
            .get::<WeakErrorHandler>()
            .expect("error handler must be provided")
            .clone()
    }
}
