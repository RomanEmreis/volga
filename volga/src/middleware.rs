//! Middleware tools

use crate::{
    App, HttpResult,
    http::{FromRequestRef, IntoResponse, MapErr, request::IntoTapResult},
    not_found,
    routing::{Route, RouteGroup},
};
use futures_util::future::BoxFuture;
use make_fn::*;
use std::sync::Arc;

#[cfg(feature = "di")]
use crate::di::FromContainer;

pub use handler::{Filter, MapOk, Middleware, Next, TapReq, With};
pub use http_context::HttpContext;
pub(crate) use make_fn::from_handler;

#[cfg(any(
    feature = "compression-brotli",
    feature = "compression-gzip",
    feature = "compression-zstd",
    feature = "compression-full"
))]
pub mod compress;
pub mod cors;
#[cfg(any(
    feature = "decompression-brotli",
    feature = "decompression-gzip",
    feature = "decompression-zstd",
    feature = "decompression-full"
))]
pub mod decompress;
pub mod handler;
pub mod http_context;
pub(super) mod make_fn;

const DEFAULT_MW_CAPACITY: usize = 8;

/// Points to the next middleware or request handler
pub type NextFn = Arc<dyn Fn(HttpContext) -> BoxFuture<'static, HttpResult> + Send + Sync>;

/// Point to a middleware function
pub(super) type MiddlewareFn =
    Arc<dyn Fn(HttpContext, NextFn) -> BoxFuture<'static, HttpResult> + Send + Sync>;

/// Middleware pipeline
#[derive(Clone)]
pub(super) struct Middlewares {
    pub(super) pipeline: Vec<MiddlewareFn>,
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
        Self {
            pipeline: Vec::with_capacity(DEFAULT_MW_CAPACITY),
        }
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
            // Allocate the placeholder once at compose time, not per-request
            let dummy: NextFn = Arc::new(|_| Box::pin(async { not_found!() }));
            Arc::new(move |ctx| handler(ctx, dummy.clone()))
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
    /// Wraps the application request pipeline with an inline middleware closure.
    ///
    /// `wrap` is ideal for simple inline middleware.
    /// For reusable or parameterized middleware types, use [`attach`](Self::attach).
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
    ///
    /// # Timeouts
    ///
    /// The pipeline does not enforce per-request timeouts. If your middleware
    /// performs a long-running or potentially unbounded operation, check the
    /// [`CancellationToken`](crate::CancellationToken) injected into each
    /// request's extensions to avoid holding connections open indefinitely:
    ///
    /// ```no_run
    /// use volga::{App, CancellationToken, error::Error};
    ///
    ///# #[tokio::main]
    ///# async fn main() -> std::io::Result<()> {
    /// let mut app = App::new();
    ///
    /// app.wrap(|ctx, next| async move {
    ///     let token = ctx.extract::<CancellationToken>()?;
    ///     tokio::select! {
    ///         res = next(ctx) => res,
    ///         _ = token.cancelled() => Err(Error::server_error("request cancelled")),
    ///     }
    /// });
    ///# app.run().await
    ///# }
    /// ```
    pub fn wrap<F, Fut>(&mut self, middleware: F) -> &mut Self
    where
        F: Fn(HttpContext, NextFn) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = HttpResult> + Send + 'static,
    {
        self.pipeline.middlewares_mut().add(make_fn(middleware));
        self
    }

    /// Attaches a middleware to the application request pipeline.
    ///
    /// Unlike [`wrap`](Self::wrap), which is optimized for inline closures,
    /// `attach` is intended for reusable and parameterized middleware types.
    ///
    /// This allows defining middleware as structs with configuration and state,
    /// similar to middleware patterns found in other ecosystems.
    ///
    /// # Parameterized middleware
    /// ```no_run
    /// use std::time::Duration;
    /// use volga::{App, HttpResult, middleware::{HttpContext, NextFn, Middleware}};
    ///
    ///# #[tokio::main]
    ///# async fn main() -> std::io::Result<()> {
    /// let mut app = App::new();
    ///
    /// app.attach(Timeout {
    ///     duration: Duration::from_secs(1),
    /// });
    ///# app.run().await
    ///# }
    ///
    /// struct Timeout {
    ///     duration: Duration,
    /// }
    ///
    /// impl Middleware for Timeout {
    ///     fn call(&self, ctx: HttpContext, next: NextFn) -> impl Future<Output = HttpResult> + 'static {
    ///         let duration = self.duration;
    ///         async move {
    ///             tokio::time::sleep(duration).await;
    ///             next(ctx).await
    ///         }
    ///     }
    /// }
    /// ```
    ///
    /// # Closure middleware
    /// `attach` also accepts closures, but type annotations are required:
    ///
    /// ```no_run
    /// use volga::{App, middleware::{HttpContext, NextFn}};
    ///
    ///# #[tokio::main]
    ///# async fn main() -> std::io::Result<()> {
    /// let mut app = App::new();
    ///
    /// app.attach(|ctx: HttpContext, next: NextFn| async move {
    ///     next(ctx).await
    /// });
    ///# app.run().await
    ///# }
    /// ```
    ///
    /// For simpler inline middleware without type annotations,
    /// prefer [`wrap`](Self::wrap).
    pub fn attach<F>(&mut self, middleware: F) -> &mut Self
    where
        F: Middleware,
    {
        self.pipeline.middlewares_mut().add(make_fn(middleware));
        self
    }

    /// Adds a filter middleware handler for a request pipeline that would return
    /// either `bool`, [`Result`] or [`FilterResult`]
    /// and breaks the middleware chain if it's a `false` or [`Err`] values
    ///
    /// > **Note:** [`Path`] and [`NamedPath`] extractors are not meaningful in a global
    /// > filter context since they depend on route-specific parameters. Use
    /// > them only when registering a filter for a specific route.
    /// > Attempting to extract route parameters globally will result in an
    /// > extraction error for routes that don't define those parameters.
    ///
    /// # Example
    /// ```no_run
    /// use volga::{App, headers::HttpHeaders};
    ///
    ///# #[tokio::main]
    ///# async fn main() -> std::io::Result<()> {
    /// let mut app = App::new();
    ///
    /// app.filter(|headers: HttpHeaders| async move {
    ///     headers.get_raw("x-api-key").is_some()
    /// });
    ///
    /// app.map_get("/sum", |x: i32, y: i32| async move { x + y });
    ///# app.run().await
    ///# }
    /// ```
    pub fn filter<F, Args>(&mut self, filter: F) -> &mut Self
    where
        F: Filter<Args>,
        Args: FromRequestRef + Send + 'static,
    {
        self.pipeline.middlewares_mut().add(make_filter_fn(filter));
        self
    }

    /// Adds a middleware called when [`HttpResult`] is [`Ok`]
    ///
    /// # Example
    /// ```no_run
    /// use volga::{App, HttpResponse, headers::{Header, headers}};
    ///
    /// headers! {
    ///     (CustomHeader, "x-custom-header")
    /// }
    ///
    ///# #[tokio::main]
    ///# async fn main() -> std::io::Result<()> {
    /// let mut app = App::new();
    ///
    /// app.map_ok(|mut resp: HttpResponse| async move {
    ///     resp.insert_header(Header::<CustomHeader>::from_static("Custom Value"));
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
        F: MapOk<Args, Output = R>,
        R: IntoResponse + 'static,
        Args: FromRequestRef + Send + 'static,
    {
        self.pipeline.middlewares_mut().add(make_map_ok_fn(map));
        self
    }

    /// Registers request-tapping middleware.
    ///
    /// The middleware receives ownership of the incoming request and may
    /// transform it before it is passed to the next stage.
    ///
    /// The return type may be either:
    /// - `HttpRequestMut`
    /// - `Result<HttpRequestMut, Error>`
    ///
    /// See [`IntoTapResult`] for details.
    ///
    /// # Example
    /// ```no_run
    /// use volga::{App, HttpRequestMut, headers::{Header, headers}};
    ///
    /// headers! {
    ///     (CustomHeader, "X-Custom-Header")
    /// }
    ///
    ///# #[tokio::main]
    ///# async fn main() -> std::io::Result<()> {
    /// let mut app = App::new();
    ///
    /// app.tap_req(|mut req: HttpRequestMut| async move {
    ///     req.insert_header(Header::<CustomHeader>::from_static("Custom Value"));
    ///     req
    /// });
    ///
    /// app.map_get("/sum", |x: i32, y: i32| async move { x + y });
    ///
    ///# app.run().await
    ///# }
    /// ```
    ///
    /// # Security
    ///
    /// `tap_req` grants full mutable ownership of the incoming request, including
    /// all headers. Security-critical values such as `Authorization` can be
    /// stripped or overwritten before downstream middleware and handlers observe
    /// them. Only register trusted closures and be mindful that registration order
    /// determines which code sees the original request.
    #[cfg(feature = "di")]
    pub fn tap_req<F, Args, R>(&mut self, map: F) -> &mut Self
    where
        F: TapReq<Args, Output = R>,
        R: IntoTapResult,
        Args: FromContainer + Send + 'static,
    {
        self.pipeline.middlewares_mut().add(make_tap_req_fn(map));
        self
    }

    /// Registers request-tapping middleware.
    ///
    /// The middleware receives ownership of the incoming request and may
    /// transform it before it is passed to the next stage.
    ///
    /// The return type may be either:
    /// - `HttpRequestMut`
    /// - `Result<HttpRequestMut, Error>`
    ///
    /// See [`IntoTapResult`] for details.
    ///
    /// # Example
    /// ```no_run
    /// use volga::{App, HttpRequestMut, headers::{Header, headers}};
    ///
    /// headers! {
    ///     (CustomHeader, "X-Custom-Header")
    /// }
    ///
    ///# #[tokio::main]
    ///# async fn main() -> std::io::Result<()> {
    /// let mut app = App::new();
    ///
    /// app.tap_req(|mut req: HttpRequestMut| async move {
    ///     req.insert_header(Header::<CustomHeader>::from_static("Custom Value"));
    ///     req
    /// });
    ///
    /// app.map_get("/sum", |x: i32, y: i32| async move { x + y });
    ///
    ///# app.run().await
    ///# }
    /// ```
    ///
    /// # Security
    ///
    /// `tap_req` grants full mutable ownership of the incoming request, including
    /// all headers. Security-critical values such as `Authorization` can be
    /// stripped or overwritten before downstream middleware and handlers observe
    /// them. Only register trusted closures and be mindful that registration order
    /// determines which code sees the original request.
    #[cfg(not(feature = "di"))]
    pub fn tap_req<F, R>(&mut self, map: F) -> &mut Self
    where
        F: TapReq<Output = R>,
        R: IntoTapResult,
    {
        self.pipeline.middlewares_mut().add(make_tap_req_fn(map));
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
        F: With<Args, Output = R>,
        R: IntoResponse + 'static,
        Args: FromRequestRef + Send + 'static,
    {
        self.pipeline
            .middlewares_mut()
            .add(make_with_fn(middleware));
        self
    }

    /// Registers default middleware
    pub(super) fn use_endpoints(&mut self) {
        if self.pipeline.has_middleware_pipeline() {
            self.wrap(|ctx: HttpContext, _: NextFn| async move { ctx.execute().await });
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
        Fut: Future<Output = HttpResult> + Send + 'static,
    {
        self.map_middleware(make_fn(middleware))
    }

    /// Attaches a middleware to this route request pipeline.
    ///
    /// Unlike [`wrap`](Self::wrap), which is optimized for inline closures,
    /// `attach` is intended for reusable and parameterized middleware types.
    ///
    /// This allows defining middleware as structs with configuration and state,
    /// similar to middleware patterns found in other ecosystems.
    ///
    /// # Parameterized middleware
    /// ```no_run
    /// use std::time::Duration;
    /// use volga::{App, HttpResult, middleware::{HttpContext, NextFn, Middleware}};
    ///
    ///# #[tokio::main]
    ///# async fn main() -> std::io::Result<()> {
    /// let mut app = App::new();
    ///
    /// app
    ///     .map_get("/hello", || async { "Hello, World!" })
    ///     .attach(Timeout {
    ///         duration: Duration::from_secs(1),
    ///     });
    ///# app.run().await
    ///# }
    ///
    /// struct Timeout {
    ///     duration: Duration,
    /// }
    ///
    /// impl Middleware for Timeout {
    ///     fn call(&self, ctx: HttpContext, next: NextFn) -> impl Future<Output = HttpResult> + 'static {
    ///         let duration = self.duration;
    ///         async move {
    ///             tokio::time::sleep(duration).await;
    ///             next(ctx).await
    ///         }
    ///     }
    /// }
    /// ```
    ///
    /// # Closure middleware
    /// `attach` also accepts closures, but type annotations are required:
    ///
    /// ```no_run
    /// use volga::{App, middleware::{HttpContext, NextFn}};
    ///
    ///# #[tokio::main]
    ///# async fn main() -> std::io::Result<()> {
    /// let mut app = App::new();
    ///
    /// app
    ///     .map_get("/hello", || async { "Hello, World!" })
    ///     .attach(|ctx: HttpContext, next: NextFn| async move {
    ///         next(ctx).await
    ///     });
    ///# app.run().await
    ///# }
    /// ```
    ///
    /// For simpler inline middleware without type annotations,
    /// prefer [`wrap`](Self::wrap).
    pub fn attach<F>(self, middleware: F) -> Self
    where
        F: Middleware,
    {
        self.map_middleware(make_fn(middleware))
    }

    /// Adds a filter middleware handler for this route that would return
    /// either `bool`, [`Result`] or [`FilterResult`]
    /// and breaks the middleware chain if it's a `false` or [`Err`] values
    ///
    /// # Example
    /// ```no_run
    /// use volga::{App, Path};
    ///
    ///# #[tokio::main]
    ///# async fn main() -> std::io::Result<()> {
    /// let mut app = App::new();
    ///
    /// app
    ///     .map_get("/sum", |x: i32, y: i32| async move { x + y })
    ///     .filter(|Path((x, y)): Path<(i32, i32)>| async move { x > 0 && y > 0 });
    ///
    ///# app.run().await
    ///# }
    /// ```
    pub fn filter<F, Args>(self, filter: F) -> Self
    where
        F: Filter<Args>,
        Args: FromRequestRef + Send + 'static,
    {
        let filter_fn = make_filter_fn(filter);
        self.map_middleware(filter_fn)
    }

    /// Adds middleware called for this route when [`HttpResult`] is [`Ok`]
    ///
    /// # Example
    /// ```no_run
    /// use volga::{App, HttpResponse, headers::{Header, headers}};
    ///
    /// headers! {
    ///     (CustomHeader, "x-custom-header")
    /// }
    ///
    ///# #[tokio::main]
    ///# async fn main() -> std::io::Result<()> {
    /// let mut app = App::new();
    ///
    /// app
    ///     .map_get("/sum", |x: i32, y: i32| async move { x + y })
    ///     .map_ok(|mut resp: HttpResponse| async move {
    ///         resp.insert_header(Header::<CustomHeader>::from_static("Custom Value"));
    ///         resp
    ///     });
    ///
    ///# app.run().await
    ///# }
    /// ```
    pub fn map_ok<F, R, Args>(self, map: F) -> Self
    where
        F: MapOk<Args, Output = R>,
        R: IntoResponse + 'static,
        Args: FromRequestRef + Send + 'static,
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
        F: MapErr<Args, Output = R>,
        R: IntoResponse + 'static,
        Args: FromRequestRef + Send + 'static,
    {
        let map_err_fn = make_map_err_fn(map);
        self.map_middleware(map_err_fn)
    }

    /// Registers request-tapping middleware.
    ///
    /// The middleware receives ownership of the incoming request and may
    /// transform it before it is passed to the next stage.
    ///
    /// The return type may be either:
    /// - `HttpRequestMut`
    /// - `Result<HttpRequestMut, Error>`
    ///
    /// See [`IntoTapResult`] for details.
    ///
    /// # Example
    /// ```no_run
    /// use volga::{App, HttpRequestMut, headers::{Header, headers}};
    ///
    /// headers! {
    ///     (CustomHeader, "X-Custom-Header")
    /// }
    ///
    ///# #[tokio::main]
    ///# async fn main() -> std::io::Result<()> {
    /// let mut app = App::new();
    ///
    /// app
    ///     .map_get("/sum", |x: i32, y: i32| async move { x + y })
    ///     .tap_req(|mut req: HttpRequestMut| async move {
    ///         req.insert_header(Header::<CustomHeader>::from_static("Custom Value"));
    ///         req
    ///     });
    ///
    ///# app.run().await
    ///# }
    /// ```
    ///
    /// # Security
    ///
    /// `tap_req` grants full mutable ownership of the incoming request, including
    /// all headers. Security-critical values such as `Authorization` can be
    /// stripped or overwritten before downstream middleware and handlers observe
    /// them. Only register trusted closures and be mindful that registration order
    /// determines which code sees the original request.
    #[cfg(feature = "di")]
    pub fn tap_req<F, Args, R>(self, map: F) -> Self
    where
        F: TapReq<Args, Output = R>,
        R: IntoTapResult,
        Args: FromContainer + Send + 'static,
    {
        let map_err_fn = make_tap_req_fn(map);
        self.map_middleware(map_err_fn)
    }

    /// Registers request-tapping middleware.
    ///
    /// The middleware receives ownership of the incoming request and may
    /// transform it before it is passed to the next stage.
    ///
    /// The return type may be either:
    /// - `HttpRequestMut`
    /// - `Result<HttpRequestMut, Error>`
    ///
    /// See [`IntoTapResult`] for details.
    ///
    /// # Example
    /// ```no_run
    /// use volga::{App, HttpRequestMut, headers::{Header, headers}};
    ///
    /// headers! {
    ///     (CustomHeader, "X-Custom-Header")
    /// }
    ///
    ///# #[tokio::main]
    ///# async fn main() -> std::io::Result<()> {
    /// let mut app = App::new();
    ///
    /// app
    ///     .map_get("/sum", |x: i32, y: i32| async move { x + y })
    ///     .tap_req(|mut req: HttpRequestMut| async move {
    ///         req.insert_header(Header::<CustomHeader>::from_static("Custom Value"));
    ///         req
    ///     });
    ///
    ///# app.run().await
    ///# }
    /// ```
    ///
    /// # Security
    ///
    /// `tap_req` grants full mutable ownership of the incoming request, including
    /// all headers. Security-critical values such as `Authorization` can be
    /// stripped or overwritten before downstream middleware and handlers observe
    /// them. Only register trusted closures and be mindful that registration order
    /// determines which code sees the original request.
    #[cfg(not(feature = "di"))]
    pub fn tap_req<F, R>(self, map: F) -> Self
    where
        F: TapReq<Output = R>,
        R: IntoTapResult,
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
        F: With<Args, Output = R>,
        R: IntoResponse + 'static,
        Args: FromRequestRef + Send + 'static,
    {
        let with_fn = make_with_fn(middleware);
        self.map_middleware(with_fn)
    }

    #[inline]
    pub(crate) fn map_middleware(self, mw: MiddlewareFn) -> Self {
        self.app.pipeline.endpoints_mut().map_layer(
            self.method.clone(),
            self.pattern.as_ref(),
            mw.into(),
        );
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

    /// Attaches a middleware to this route group request pipeline.
    ///
    /// Unlike [`wrap`](Self::wrap), which is optimized for inline closures,
    /// `attach` is intended for reusable and parameterized middleware types.
    ///
    /// This allows defining middleware as structs with configuration and state,
    /// similar to middleware patterns found in other ecosystems.
    ///
    /// # Parameterized middleware
    /// ```no_run
    /// use std::time::Duration;
    /// use volga::{App, HttpResult, middleware::{HttpContext, NextFn, Middleware}};
    ///
    ///# #[tokio::main]
    ///# async fn main() -> std::io::Result<()> {
    /// let mut app = App::new();
    ///
    /// app.group("/hello", |api| {
    ///     api.attach(Timeout {
    ///         duration: Duration::from_secs(1),
    ///     });
    ///     api.map_get("/world", || async { "Hello, World!" });
    /// });
    ///# app.run().await
    ///# }
    ///
    /// struct Timeout {
    ///     duration: Duration,
    /// }
    ///
    /// impl Middleware for Timeout {
    ///     fn call(&self, ctx: HttpContext, next: NextFn) -> impl Future<Output = HttpResult> + 'static {
    ///         let duration = self.duration;
    ///         async move {
    ///             tokio::time::sleep(duration).await;
    ///             next(ctx).await
    ///         }
    ///     }
    /// }
    /// ```
    ///
    /// # Closure middleware
    /// `attach` also accepts closures, but type annotations are required:
    ///
    /// ```no_run
    /// use volga::{App, middleware::{HttpContext, NextFn}};
    ///
    ///# #[tokio::main]
    ///# async fn main() -> std::io::Result<()> {
    /// let mut app = App::new();
    ///
    /// app.group("/hello", |api| {
    ///     api.attach(|ctx: HttpContext, next: NextFn| async move {
    ///         next(ctx).await
    ///     });
    ///     api.map_get("/world", || async { "Hello, World!" });
    /// });
    ///# app.run().await
    ///# }
    /// ```
    ///
    /// For simpler inline middleware without type annotations,
    /// prefer [`wrap`](Self::wrap).
    pub fn attach<F>(&mut self, middleware: F) -> &mut Self
    where
        F: Middleware,
    {
        self.middleware.push(make_fn(middleware));
        self
    }

    /// Adds a filter middleware handler for a group of routes that would return
    /// either `bool`, [`Result`] or [`FilterResult`]
    /// and breaks the middleware chain if it's a `false` or [`Err`] values
    ///
    /// > **Note:** [`Path`] and [`NamedPath`] extractors are not meaningful in a
    /// > route group filter context since they depend on route-specific parameters. Use
    /// > them only when registering a filter for a specific route.
    /// > Attempting to extract route parameters globally will result in an
    /// > extraction error for routes that don't define those parameters.
    ///
    /// # Example
    /// ```no_run
    /// use volga::{App, headers::HttpHeaders};
    ///
    ///# #[tokio::main]
    ///# async fn main() -> std::io::Result<()> {
    /// let mut app = App::new();
    ///
    /// app.group("/positive", |api| {
    ///     api.filter(|headers: HttpHeaders| async move {
    ///         headers.get_raw("x-api-key").is_some()
    ///     });
    ///
    ///     api.map_get("/sum", |x: i32, y: i32| async move { x + y });
    ///     api.map_get("/mul", |x: i32, y: i32| async move { x * y });
    /// });
    ///# app.run().await
    ///# }
    /// ```
    pub fn filter<F, Args>(&mut self, filter: F) -> &mut Self
    where
        F: Filter<Args>,
        Args: FromRequestRef + Send + 'static,
    {
        let filter_fn = make_filter_fn(filter);
        self.middleware.push(filter_fn);
        self
    }

    /// Adds middleware called for this group of routes when [`HttpResult`] is [`Ok`]
    ///
    /// # Example
    /// ```no_run
    /// use volga::{App, HttpResponse, headers::{Header, headers}};
    ///
    /// headers! {
    ///     (CustomHeader, "x-custom-header")
    /// }
    ///
    ///# #[tokio::main]
    ///# async fn main() -> std::io::Result<()> {
    /// let mut app = App::new();
    ///
    /// app.group("/positive", |api| {
    ///     api.map_ok(|mut resp: HttpResponse| async move {
    ///         resp.insert_header(Header::<CustomHeader>::from_static("Custom Value"));
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
        F: MapOk<Args, Output = R>,
        R: IntoResponse + 'static,
        Args: FromRequestRef + Send + 'static,
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
        F: MapErr<Args, Output = R>,
        R: IntoResponse + 'static,
        Args: FromRequestRef + Send + 'static,
    {
        let map_err_fn = make_map_err_fn(map);
        self.middleware.push(map_err_fn);
        self
    }

    /// Registers request-tapping middleware.
    ///
    /// The middleware receives ownership of the incoming request and may
    /// transform it before it is passed to the next stage.
    ///
    /// The return type may be either:
    /// - `HttpRequestMut`
    /// - `Result<HttpRequestMut, Error>`
    ///
    /// See [`IntoTapResult`] for details.
    ///
    /// # Example
    /// ```no_run
    /// use volga::{App, HttpRequestMut, headers::{Header, headers}};
    ///
    /// headers! {
    ///     (CustomHeader, "X-Custom-Header")
    /// }
    ///
    ///# #[tokio::main]
    ///# async fn main() -> std::io::Result<()> {
    /// let mut app = App::new();
    ///
    /// app.group("/positive", |api| {
    ///     api.tap_req(|mut req: HttpRequestMut| async move {
    ///         req.insert_header(Header::<CustomHeader>::from_static("Custom Value"));
    ///         req
    ///     });
    ///     api.map_get("/sum", |x: i32, y: i32| async move {
    ///         x + y
    ///     });
    /// });
    ///# app.run().await
    ///# }
    /// ```
    ///
    /// # Security
    ///
    /// `tap_req` grants full mutable ownership of the incoming request, including
    /// all headers. Security-critical values such as `Authorization` can be
    /// stripped or overwritten before downstream middleware and handlers observe
    /// them. Only register trusted closures and be mindful that registration order
    /// determines which code sees the original request.
    #[cfg(feature = "di")]
    pub fn tap_req<F, Args, R>(&mut self, map: F) -> &mut Self
    where
        F: TapReq<Args, Output = R>,
        R: IntoTapResult,
        Args: FromContainer + Send + 'static,
    {
        let tap_req_fn = make_tap_req_fn(map);
        self.middleware.push(tap_req_fn);
        self
    }

    /// Registers request-tapping middleware.
    ///
    /// The middleware receives ownership of the incoming request and may
    /// transform it before it is passed to the next stage.
    ///
    /// The return type may be either:
    /// - `HttpRequestMut`
    /// - `Result<HttpRequestMut, Error>`
    ///
    /// See [`IntoTapResult`] for details.
    ///
    /// # Example
    /// ```no_run
    /// use volga::{App, HttpRequestMut, headers::{Header, headers}};
    ///
    /// headers! {
    ///     (CustomHeader, "X-Custom-Header")
    /// }
    ///
    ///# #[tokio::main]
    ///# async fn main() -> std::io::Result<()> {
    /// let mut app = App::new();
    ///
    /// app.group("/positive", |api| {
    ///     api.tap_req(|mut req: HttpRequestMut| async move {
    ///         req.insert_header(Header::<CustomHeader>::from_static("Custom Value"));
    ///         req
    ///     });
    ///     api.map_get("/sum", |x: i32, y: i32| async move {
    ///         x + y
    ///     });
    /// });
    ///# app.run().await
    ///# }
    /// ```
    ///
    /// # Security
    ///
    /// `tap_req` grants full mutable ownership of the incoming request, including
    /// all headers. Security-critical values such as `Authorization` can be
    /// stripped or overwritten before downstream middleware and handlers observe
    /// them. Only register trusted closures and be mindful that registration order
    /// determines which code sees the original request.
    #[cfg(not(feature = "di"))]
    pub fn tap_req<F, R>(&mut self, map: F) -> &mut Self
    where
        F: TapReq<Output = R>,
        R: IntoTapResult,
    {
        let tap_req_fn = make_tap_req_fn(map);
        self.middleware.push(tap_req_fn);
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
        F: With<Args, Output = R>,
        R: IntoResponse + 'static,
        Args: FromRequestRef + Send + 'static,
    {
        let with_fn = make_with_fn(middleware);
        self.middleware.push(with_fn);
        self
    }
}
