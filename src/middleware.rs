//! Middleware tools

use futures_util::future::BoxFuture;
use std::{future::Future, sync::Arc};
use crate::{
    http::{
        endpoints::handlers::RouteHandler,
        IntoResponse,
        FromRequest,
        GenericHandler,
        FilterResult,
    }, 
    App, 
    app::router::{Route, RouteGroup}, 
    HttpResult, 
    HttpRequest, 
    not_found
};

pub use http_context::HttpContext;

#[cfg(any(
    feature = "compression-brotli",
    feature = "compression-gzip",
    feature = "compression-zstd",
    feature = "compression-full"
))]
pub mod compress;
#[cfg(any(
    feature = "decompression-brotli",
    feature = "decompression-gzip",
    feature = "decompression-zstd",
    feature = "decompression-full"
))]
pub mod decompress;
pub mod http_context;
pub mod cors;

const DEFAULT_MW_CAPACITY: usize = 8;

/// Points to the next middleware or request handler
pub type Next = Arc<
    dyn Fn(HttpContext) -> BoxFuture<'static, HttpResult>
    + Send
    + Sync
>;

/// Point to a middleware function
pub(super) type MiddlewareFn = Arc<
    dyn Fn(HttpContext, Next) -> BoxFuture<'static, HttpResult>
    + Send
    + Sync
>;

pub(super) fn from_handler(handler: RouteHandler) -> MiddlewareFn {
    let handler = Arc::new(handler);
    Arc::new(move |ctx: HttpContext, _| {
        let handler = handler.clone();
        Box::pin(async move { handler.call(ctx.request).await })
    })
}

#[inline]
pub(crate) fn make_fn<F, Fut>(middleware: F) -> MiddlewareFn
where
    F: Fn(HttpContext, Next) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = HttpResult> + Send
{
    let middleware = Arc::new(middleware);
    Arc::new(move |ctx: HttpContext, next: Next| {
        let middleware = middleware.clone();
        Box::pin(async move { middleware(ctx, next).await })
    })
}

#[inline]
pub(crate) fn make_filter_fn<F, R, Args>(filter: F) -> MiddlewareFn
where
    F: GenericHandler<Args, Output = R>,
    R: Into<FilterResult> + 'static,
    Args: FromRequest + Send + Sync + 'static
{
    let middleware_fn = move |ctx: HttpContext, next: Next| {
        let filter = filter.clone();
        async move {
            let (req, pipeline) = ctx.into_parts();
            let (parts, body) = req.into_parts();

            let args = Args::from_request(HttpRequest::slim(&parts)).await.unwrap();
            let result = filter
                .call(args)
                .await
                .into();

            let req = HttpRequest::from_parts(parts, body);
            let ctx = HttpContext::new(req, pipeline);
            match result.into_inner() {
                Ok(_) => next(ctx).await,
                Err(err) => err.into_response()
            }
        }
    };
    make_fn(middleware_fn)
}

#[derive(Clone)]
pub(super) struct Middlewares {
    pub(super) pipeline: Vec<MiddlewareFn>
}

impl From<MiddlewareFn> for Middlewares {
    #[inline]
    fn from(mw: MiddlewareFn) -> Self {
        let mut middlewares = Self::new();
        middlewares.add(mw);
        middlewares
    }
}

impl Middlewares {
    /// Initializes a new middleware pipeline
    pub(super) fn new() -> Self {
        Self { pipeline: Vec::with_capacity(DEFAULT_MW_CAPACITY) }
    }

    /// Returns `true` if there are no middlewares,
    /// otherwise `false`
    pub(super) fn is_empty(&self) -> bool {
        self.pipeline.is_empty()
    }
    
    /// Adds middleware function to the pipeline
    #[inline]
    pub(super) fn add(&mut self, middleware: MiddlewareFn) {
        self.pipeline.push(middleware);
    }

    /// Composes middlewares into a "Linked List" and returns head
    pub(super) fn compose(&self) -> Option<Next> {
        if self.pipeline.is_empty() {
            return None;
        }

        // Fetching the last middleware which is the request handler to be the initial `next`.
        let request_handler = self.pipeline.last().unwrap().clone();
        let mut next: Next = Arc::new(move |ctx| {
            let handler = request_handler.clone();
            // Call the last middleware, ignoring its `next` argument with an empty placeholder
            Box::pin(async move {
                handler(ctx, Arc::new(|_| Box::pin(async { not_found!() }))).await
            })
        });

        for mw in self.pipeline.iter().rev().skip(1) {
            let current_mw: MiddlewareFn = mw.clone();
            let prev_next: Next = next.clone();

            next = Arc::new(move |ctx| {
                let current_mw = current_mw.clone();
                let prev_next = prev_next.clone();
                Box::pin(async move {
                    current_mw(ctx, prev_next).await
                })
            });
        }
        Some(next)
    }
}

/// Middleware specific impl
impl App {
    /// Adds a middleware handler to the application request pipeline
    /// 
    /// # Examples
    /// ```no_run
    /// use volga::{App, Results};
    ///
    ///# #[tokio::main]
    ///# async fn main() -> std::io::Result<()> {
    /// let mut app = App::new();
    /// 
    /// app.use_middleware(|ctx, next| async move {
    ///     next(ctx).await
    /// });
    ///# app.run().await
    ///# }
    /// ```
    pub fn use_middleware<F, Fut>(&mut self, middleware: F) -> &mut Self
    where
        F: Fn(HttpContext, Next) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = HttpResult> + Send,
    {
        self.pipeline
            .middlewares_mut()
            .add(make_fn(middleware));
        self
    }

    /// Registers default middleware
    pub(super) fn use_endpoints(&mut self) {
        if self.pipeline.has_middleware_pipeline() {
            self.use_middleware(|ctx, _| async move {
                ctx.execute().await
            });
        }
    }
}

impl<'a> Route<'a> {
    /// Adds a middleware handler to this route request pipeline
    /// 
    /// # Examples
    /// ```no_run
    /// use volga::{App, Results};
    ///
    ///# #[tokio::main]
    ///# async fn main() -> std::io::Result<()> {
    /// let mut app = App::new();
    /// 
    /// app
    ///     .map_get("/hello", || async { "Hello, World!" })
    ///     .use_middleware(|ctx, next| async move {
    ///         next(ctx).await
    ///     });
    /// 
    ///# app.run().await
    ///# }
    /// ```
    pub fn use_middleware<F, Fut>(self, middleware: F) -> Self
    where
        F: Fn(HttpContext, Next) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = HttpResult> + Send,
    {
        self.map_middleware(make_fn(middleware))
    }
    
    /// Adds a filter middleware handler for this route that would return 
    /// either `bool`, [`Result`] or [`FilterResult`]
    /// and breaks the middleware chain if it's a `false` or [`Err`] values
    /// 
    /// # Example
    /// ```no_run
    /// use volga::{App, Results};
    ///
    ///# #[tokio::main]
    ///# async fn main() -> std::io::Result<()> {
    /// let mut app = App::new();
    /// 
    /// app
    ///     .map_get("/sum", |x: i32, y: i32| async move { x + y })
    ///     .filter(|x: i32, y: i32| async move { x > 0 && y > 0 });
    /// 
    ///# app.run().await
    ///# }
    /// ```
    pub fn filter<F, R, Args>(self, filter: F) -> Self
    where
        F: GenericHandler<Args, Output = R>,
        R: Into<FilterResult> + 'static,
        Args: FromRequest + Send + Sync + 'static
    {
        let filter_fn = make_filter_fn(filter);
        self.map_middleware(filter_fn)
    }
    
    #[inline]
    pub(crate) fn map_middleware(self, mw: MiddlewareFn) -> Self {
        self.app
            .pipeline
            .endpoints_mut()
            .map_layer(self.method.clone(), self.pattern, mw.into());
        self
    }
}

impl<'a> RouteGroup<'a> {
    /// Adds a middleware handler to this group of routes
    /// 
    /// # Examples
    /// ```no_run
    /// use volga::{App, Results};
    ///
    ///# #[tokio::main]
    ///# async fn main() -> std::io::Result<()> {
    /// let mut app = App::new();
    /// 
    /// app.map_group("/hello")
    ///     .use_middleware(|ctx, next| async move {
    ///         next(ctx).await
    ///     })
    ///     .map_get("/world", || async { "Hello, World!" });
    /// 
    ///# app.run().await
    ///# }
    /// ```
    pub fn use_middleware<F, Fut>(mut self, middleware: F) -> Self
    where
        F: Fn(HttpContext, Next) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = HttpResult> + Send,
    {
        self.middleware.push(make_fn(middleware));
        self
    }

    /// Adds a filter middleware handler for a group of routes that would return 
    /// either `bool`, [`Result`] or [`FilterResult`]
    /// and breaks the middleware chain if it's a `false` or [`Err`] values
    /// 
    /// # Example
    /// ```no_run
    /// use volga::{App, Results};
    ///
    ///# #[tokio::main]
    ///# async fn main() -> std::io::Result<()> {
    /// let mut app = App::new();
    /// 
    /// app.map_group("/positive")
    ///     .filter(|x: i32, y: i32| async move { x > 0 && y > 0 })
    ///     .map_get("/sum", |x: i32, y: i32| async move { x + y })
    ///     .map_get("/mul", |x: i32, y: i32| async move { x * y });
    /// 
    ///# app.run().await
    ///# }
    /// ```
    pub fn filter<F, R, Args>(mut self, filter: F) -> Self
    where
        F: GenericHandler<Args, Output = R>,
        R: Into<FilterResult> + 'static,
        Args: FromRequest + Send + Sync + 'static
    {
        let filter_fn = make_filter_fn(filter);
        self.middleware.push(filter_fn);
        self
    }
}

