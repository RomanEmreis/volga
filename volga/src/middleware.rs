//! Middleware tools

use futures_util::future::BoxFuture;
use std::{future::Future, sync::Arc};
use make_fn::*;
use crate::{
    http::{
        IntoResponse,
        FromRequest,
        FromRequestRef,
        GenericHandler,
        MapErrHandler,
        FilterResult,
    },
    routing::{Route, RouteGroup},
    App,
    HttpResult, 
    HttpRequest,
    not_found, 
};

pub use http_context::HttpContext;
pub use handler::{Next, MiddlewareHandler, TapReqHandler, MapOkHandler};
pub(crate) use make_fn::from_handler;

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
pub mod handler;
pub(super) mod make_fn;

const DEFAULT_MW_CAPACITY: usize = 8;

/// Points to the next middleware or request handler
pub type NextFn = Arc<
    dyn Fn(HttpContext) -> BoxFuture<'static, HttpResult>
    + Send
    + Sync
>;

/// Point to a middleware function
pub(super) type MiddlewareFn = Arc<
    dyn Fn(HttpContext, NextFn) -> BoxFuture<'static, HttpResult>
    + Send
    + Sync
>;

/// Middleware pipeline
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
    pub(super) fn compose(&self) -> Option<NextFn> {
        let mut iter = self.pipeline.iter().rev();
        // Fetching the last middleware which is the request handler to be the initial `next`
        let last = iter.next()?;
        let mut next: NextFn = {
            let handler = last.clone();
            // Call the last middleware, ignoring its `next` argument with an empty placeholder
            Arc::new(move |ctx| 
                handler(ctx, Arc::new(|_| Box::pin(async { not_found!() }))))
        };

        for mw in iter {
            let current_mw = mw.clone();
            let prev_next = next.clone();
            next = Arc::new(move |ctx| current_mw(ctx, prev_next.clone()));
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
    /// use volga::App;
    ///
    ///# #[tokio::main]
    ///# async fn main() -> std::io::Result<()> {
    /// let mut app = App::new();
    /// 
    /// app.wrap(|ctx, next| async move {
    ///     next(ctx).await
    /// });
    ///# app.run().await
    ///# }
    /// ```
    pub fn wrap<F, Fut>(&mut self, middleware: F) -> &mut Self
    where
        F: Fn(HttpContext, NextFn) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = HttpResult> + Send + 'static,
    {
        self.pipeline
            .middlewares_mut()
            .add(make_fn(middleware));
        self
    }

    /// Adds a middleware called when [`HttpResult`] is [`Ok`]
    /// 
    /// # Example
    /// ```no_run
    /// use volga::{App, HttpResponse, headers::HeaderValue};
    ///
    ///# #[tokio::main]
    ///# async fn main() -> std::io::Result<()> {
    /// let mut app = App::new();
    /// 
    /// app.map_ok(|mut resp: HttpResponse| async move { 
    ///     resp.headers_mut()
    ///         .insert("X-Custom-Header", HeaderValue::from_static("Custom Value"));
    ///     resp
    /// });
    /// 
    /// app.map_get("/sum", |x: i32, y: i32| async move { x + y });
    /// 
    ///# app.run().await
    ///# }
    /// ```
    pub fn map_ok<F, R, Args>(&mut self, map: F) -> &mut Self
    where
        F: MapOkHandler<Args, Output = R>,
        R: IntoResponse + 'static,
        Args: FromRequestRef + Send + Sync + 'static,
    {
        self.pipeline
            .middlewares_mut()
            .add(make_map_ok_fn(map));
        self
    }

    /// Adds a middleware that handles incoming [`HttpRequest`]
    /// 
    /// # Example
    /// ```no_run
    /// use volga::{App, HttpRequest, headers::HeaderValue};
    ///
    ///# #[tokio::main]
    ///# async fn main() -> std::io::Result<()> {
    /// let mut app = App::new();
    /// 
    /// app.tap_req(|mut req: HttpRequest| async move { 
    ///     req.headers_mut()
    ///         .insert("X-Custom-Header", HeaderValue::from_static("Custom Value"));
    ///     req
    /// });
    /// 
    /// app.map_get("/sum", |x: i32, y: i32| async move { x + y });
    /// 
    ///# app.run().await
    ///# }
    /// ```
    pub fn tap_req<F, Args>(&mut self, map: F) -> &mut Self
    where
        F: TapReqHandler<Args, Output = HttpRequest>,
        Args: FromRequestRef + Send + Sync + 'static,
    {
        self.pipeline
            .middlewares_mut()
            .add(make_tap_req_fn(map));
        self
    }
    
    /// Adds a middleware that can take any parameters that implement [`FromRequestRef`]
    /// and the reference to the [`Next`] future; awaiting this `next` calls 
    /// the next middleware in the pipeline
    /// 
    /// Unlike the [`wrap`], this method doesn't provide direct access to the [`HttpRequest`] and [`HttpBody`]
    /// 
    /// # Example
    /// ```no_run
    /// use volga::{App, headers::HttpHeaders};
    ///
    ///# #[tokio::main]
    ///# async fn main() -> std::io::Result<()> {
    /// let mut app = App::new();
    /// 
    /// app.with(|headers: HttpHeaders, next| async move {
    ///     // do something with headers
    ///     // ...
    ///     next.await
    /// });
    ///# app.run().await
    ///# }
    /// ```
    pub fn with<F, R, Args>(&mut self, middleware: F) -> &mut Self
    where 
        F: MiddlewareHandler<Args, Output = R>,
        R: IntoResponse + 'static,
        Args: FromRequestRef + Send + Sync + 'static,
    {
        self.pipeline
            .middlewares_mut()
            .add(make_with_fn(middleware));
        self
    }

    /// Registers default middleware
    pub(super) fn use_endpoints(&mut self) {
        if self.pipeline.has_middleware_pipeline() {
            self.wrap(|ctx, _| async move {
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
    /// use volga::App;
    ///
    ///# #[tokio::main]
    ///# async fn main() -> std::io::Result<()> {
    /// let mut app = App::new();
    /// 
    /// app
    ///     .map_get("/hello", || async { "Hello, World!" })
    ///     .wrap(|ctx, next| async move {
    ///         next(ctx).await
    ///     });
    /// 
    ///# app.run().await
    ///# }
    /// ```
    pub fn wrap<F, Fut>(self, middleware: F) -> Self
    where
        F: Fn(HttpContext, NextFn) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = HttpResult> + Send +'static,
    {
        self.map_middleware(make_fn(middleware))
    }
    
    /// Adds a filter middleware handler for this route that would return 
    /// either `bool`, [`Result`] or [`FilterResult`]
    /// and breaks the middleware chain if it's a `false` or [`Err`] values
    /// 
    /// # Example
    /// ```no_run
    /// use volga::App;
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
    
    /// Adds a middleware called for this route when [`HttpResult`] is [`Ok`]
    /// 
    /// # Example
    /// ```no_run
    /// use volga::{App, HttpResponse, headers::HeaderValue};
    ///
    ///# #[tokio::main]
    ///# async fn main() -> std::io::Result<()> {
    /// let mut app = App::new();
    /// 
    /// app
    ///     .map_get("/sum", |x: i32, y: i32| async move { x + y })
    ///     .map_ok(|mut resp: HttpResponse| async move { 
    ///         resp.headers_mut()
    ///             .insert("X-Custom-Header", HeaderValue::from_static("Custom Value"));
    ///         resp
    ///     });
    /// 
    ///# app.run().await
    ///# }
    /// ```
    pub fn map_ok<F, R, Args>(self, map: F) -> Self
    where
        F: MapOkHandler<Args, Output = R>,
        R: IntoResponse + 'static,
        Args: FromRequestRef + Send + Sync + 'static,
    {
        let map_ok_fn = make_map_ok_fn(map);
        self.map_middleware(map_ok_fn)
    }

    /// Adds a middleware that called for this route when [`HttpResult`] is [`Err`]
    /// 
    /// # Example
    /// ```no_run
    /// use volga::{App, error::Error};
    ///
    ///# #[tokio::main]
    ///# async fn main() -> std::io::Result<()> {
    /// let mut app = App::new();
    /// 
    /// app
    ///     .map_get("/sum", |x: i32, y: i32| async move { x + y })
    ///     .map_err(|err: Error| async move { 
    ///         println!("{err:?}");
    ///         err
    ///     });
    /// 
    ///# app.run().await
    ///# }
    /// ```
    pub fn map_err<F, R, Args>(self, map: F) -> Self
    where
        F: MapErrHandler<Args, Output = R>,
        R: IntoResponse + 'static,
        Args: FromRequestRef + Send + Sync + 'static,
    {
        let map_err_fn = make_map_err_fn(map);
        self.map_middleware(map_err_fn)
    }

    /// Adds a middleware that handles incoming [`HttpRequest`] for this route
    /// 
    /// # Example
    /// ```no_run
    /// use volga::{App, HttpRequest, headers::HeaderValue};
    ///
    ///# #[tokio::main]
    ///# async fn main() -> std::io::Result<()> {
    /// let mut app = App::new();
    /// 
    /// app
    ///     .map_get("/sum", |x: i32, y: i32| async move { x + y })
    ///     .tap_req(|mut req: HttpRequest| async move { 
    ///         req.headers_mut()
    ///             .insert("X-Custom-Header", HeaderValue::from_static("Custom Value"));
    ///         req
    ///     });
    /// 
    ///# app.run().await
    ///# }
    /// ```
    pub fn tap_req<F, Args>(self, map: F) -> Self
    where
        F: TapReqHandler<Args, Output = HttpRequest>,
        Args: FromRequestRef + Send + Sync + 'static,
    {
        let map_err_fn = make_tap_req_fn(map);
        self.map_middleware(map_err_fn)
    }

    /// Adds a middleware for this route that can take any parameters that implement [`FromRequestRef`]
    /// and the reference to the [`Next`] future; awaiting this `next` calls 
    /// the next middleware in the pipeline
    /// 
    /// Unlike the [`wrap`], this method doesn't provide direct access to the [`HttpRequest`] and [`HttpBody`]
    /// 
    /// # Example
    /// ```no_run
    /// use volga::{App, headers::HttpHeaders};
    ///
    ///# #[tokio::main]
    ///# async fn main() -> std::io::Result<()> {
    /// let mut app = App::new();
    /// 
    /// app.with(|headers: HttpHeaders, next| async move {
    ///     // do something with headers
    ///     // ...
    ///     next.await
    /// });
    ///# app.run().await
    ///# }
    /// ```
    pub fn with<F, R, Args>(self, middleware: F) -> Self
    where
        F: MiddlewareHandler<Args, Output = R>,
        R: IntoResponse + 'static,
        Args: FromRequestRef + Send + Sync + 'static,
    {
        let with_fn = make_with_fn(middleware);
        self.map_middleware(with_fn)
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
    /// use volga::App;
    ///
    ///# #[tokio::main]
    ///# async fn main() -> std::io::Result<()> {
    /// let mut app = App::new();
    /// 
    /// app.group("/hello", |api| {
    ///     api.wrap(|ctx, next| async move { next(ctx).await });
    ///     api.map_get("/world", || async { "Hello, World!" });
    /// });
    ///# app.run().await
    ///# }
    /// ```
    pub fn wrap<F, Fut>(&mut self, middleware: F) -> &mut Self
    where
        F: Fn(HttpContext, NextFn) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = HttpResult> + Send + 'static,
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
    /// use volga::App;
    ///
    ///# #[tokio::main]
    ///# async fn main() -> std::io::Result<()> {
    /// let mut app = App::new();
    /// 
    /// app.group("/positive", |api| {
    ///     api.filter(|x: i32, y: i32| async move { x > 0 && y > 0 });
    /// 
    ///     api.map_get("/sum", |x: i32, y: i32| async move { x + y });
    ///     api.map_get("/mul", |x: i32, y: i32| async move { x * y });
    /// });
    ///# app.run().await
    ///# }
    /// ```
    pub fn filter<F, R, Args>(&mut self, filter: F) -> &mut Self
    where
        F: GenericHandler<Args, Output = R>,
        R: Into<FilterResult> + 'static,
        Args: FromRequest + Send + Sync + 'static
    {
        let filter_fn = make_filter_fn(filter);
        self.middleware.push(filter_fn);
        self
    }

    /// Adds middleware called for this group of routes when [`HttpResult`] is [`Ok`]
    /// 
    /// # Example
    /// ```no_run
    /// use volga::{App, HttpResponse, headers::HeaderValue};
    ///
    ///# #[tokio::main]
    ///# async fn main() -> std::io::Result<()> {
    /// let mut app = App::new();
    /// 
    /// app.group("/positive", |api| {
    ///     api.map_ok(|mut resp: HttpResponse| async move { 
    ///         resp.headers_mut()
    ///             .insert("X-Custom-Header", HeaderValue::from_static("Custom Value"));
    ///         resp
    ///     });
    ///     api.map_get("/sum", |x: i32, y: i32| async move { 
    ///         x + y
    ///     });
    /// });
    ///# app.run().await
    ///# }
    /// ```
    pub fn map_ok<F, R, Args>(&mut self, map: F) -> &mut Self
    where
        F: MapOkHandler<Args, Output = R>,
        R: IntoResponse + 'static,
        Args: FromRequestRef + Send + Sync + 'static,
    {
        let map_ok_fn = make_map_ok_fn(map);
        self.middleware.push(map_ok_fn);
        self
    }

    /// Adds a middleware that called for this group of routes when [`HttpResult`] is [`Err`]
    /// 
    /// # Example
    /// ```no_run
    /// use volga::{App, error::Error};
    ///
    ///# #[tokio::main]
    ///# async fn main() -> std::io::Result<()> {
    /// let mut app = App::new();
    /// 
    /// app.group("/positive", |api| {
    ///     api.map_err(|err: Error| async move { 
    ///         println!("{err:?}");
    ///         err
    ///     });
    ///     api.map_get("/sum", |x: i32, y: i32| async move { 
    ///         x + y
    ///     });
    /// });
    ///# app.run().await
    ///# }
    /// ```
    pub fn map_err<F, R, Args>(&mut self, map: F) -> &mut Self
    where
        F: MapErrHandler<Args, Output = R>,
        R: IntoResponse + 'static,
        Args: FromRequestRef + Send + Sync + 'static,
    {
        let map_err_fn = make_map_err_fn(map);
        self.middleware.push(map_err_fn);
        self
    }

    /// Adds a middleware that handles incoming [`HttpRequest`] for this group of routes
    /// 
    /// # Example
    /// ```no_run
    /// use volga::{App, HttpRequest, headers::HeaderValue};
    ///
    ///# #[tokio::main]
    ///# async fn main() -> std::io::Result<()> {
    /// let mut app = App::new();
    /// 
    /// app.group("/positive", |api| {
    ///     api.tap_req(|mut req: HttpRequest| async move { 
    ///         req.headers_mut()
    ///             .insert("X-Custom-Header", HeaderValue::from_static("Custom Value"));
    ///         req
    ///     });
    ///     api.map_get("/sum", |x: i32, y: i32| async move { 
    ///         x + y
    ///     });
    /// });
    ///# app.run().await
    ///# }
    /// ```
    pub fn tap_req<F, Args>(&mut self, map: F) -> &mut Self
    where
        F: TapReqHandler<Args, Output = HttpRequest>,
        Args: FromRequestRef + Send + Sync + 'static,
    {
        let map_err_fn = make_tap_req_fn(map);
        self.middleware.push(map_err_fn);
        self
    }

    /// Adds middleware for this group of routes that can take any parameters that implement [`FromRequestRef`]
    /// and the reference to the [`Next`] future; awaiting this `next` calls 
    /// the next middleware in the pipeline
    /// 
    /// Unlike the [`wrap`], this method doesn't provide direct access to the [`HttpRequest`] and [`HttpBody`]
    /// 
    /// # Example
    /// ```no_run
    /// use volga::{App, headers::HttpHeaders};
    ///
    ///# #[tokio::main]
    ///# async fn main() -> std::io::Result<()> {
    /// let mut app = App::new();
    /// 
    /// app.group("/hello", |api| {
    ///     api.with(|headers: HttpHeaders, next| async move {
    ///         // do something with headers
    ///         // ...
    ///         next.await
    ///     });
    /// 
    ///     api.map_get("/world", || async { "Hello, World!" });
    /// });
    ///# app.run().await
    ///# }
    /// ```
    pub fn with<F, R, Args>(&mut self, middleware: F) -> &mut Self
    where
        F: MiddlewareHandler<Args, Output = R>,
        R: IntoResponse + 'static,
        Args: FromRequestRef + Send + Sync + 'static,
    {
        let with_fn = make_with_fn(middleware);
        self.middleware.push(with_fn);
        self
    }
}

