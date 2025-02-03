use crate::http::endpoints::{
    handlers::RouteHandler,
    args::FromRequestRef
};

use crate::{
    error::{Error, handler::WeakErrorHandler},
    headers::{Header, FromHeaders},
    HttpRequest, 
    HttpResult
};

#[cfg(feature = "di")]
use crate::di::Inject;

/// Describes current HTTP context which consists of the current HTTP request data 
/// and the reference to the method handler fot this request
pub struct HttpContext {
    /// Current HTTP request
    pub request: HttpRequest,
    /// Global Request/Middleware error handler
    pub(crate) error_handler: WeakErrorHandler,
    /// Current handler that mapped to handle the HTTP request
    handler: RouteHandler,
}

impl HttpContext {
    /// Creates a new [`HttpContext`]
    #[inline]
    pub(crate) fn new(
        request: HttpRequest,
        handler: RouteHandler, 
        error_handler: WeakErrorHandler
    ) -> Self {
        Self { request, handler, error_handler }
    }
    
    #[inline]
    #[allow(dead_code)]
    pub(super) fn into_parts(self) -> (HttpRequest, RouteHandler, WeakErrorHandler) {
        (self.request, self.handler, self.error_handler)
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

    /// Resolves a service from Dependency Container
    #[inline]
    #[cfg(feature = "di")]
    pub async fn resolve<T: Inject + 'static>(&self) -> Result<T, Error> {
        self.request.resolve::<T>().await
    }

    /// Resolves a service from Dependency Container and returns a reference
    #[inline]
    #[cfg(feature = "di")]
    pub async fn resolve_ref<T: Inject + 'static>(&self) -> Result<&T, Error> {
        self.request.resolve_ref::<T>().await
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
}
