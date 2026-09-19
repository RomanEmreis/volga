//! Route mapping helpers
//!
//! # Catch-all parameters
//!
//! A route's last segment can be a catch-all parameter, `{*name}`, which binds the rest of
//! the path as one value:
//!
//! ```
//!# use volga::App;
//! let mut app = App::new();
//!
//! // GET /files/docs/2026/report.pdf binds `path` as "docs/2026/report.pdf"
//! app.map_get("/files/{*path}", |path: String| async move { path });
//! ```
//!
//! - **It reads at least one segment.** `/files/{*path}` does not answer `/files` or
//!   `/files/`, so that position can carry a route of its own.
//! - **The value is the path as the request wrote it**, from the first segment the
//!   catch-all reads to the end: separators inside it and a trailing one are kept.
//!   `GET /files/a/b/` binds `"a/b/"`. It is decoded the way any parameter is: a positional
//!   extractor (`String`, `Path<T>`) reads it undecoded, while `NamedPath<T>` decodes its
//!   percent-escapes - so `GET /files/a%2Fb/c` reads as `"a%2Fb/c"` through the first and
//!   `"a/b/c"` through the second.
//! - **It is not a safe file system path.** Nothing in it is normalized, so a `..` segment
//!   reaches the handler as the request wrote it: `GET /files/../../etc/passwd` binds
//!   `"../../etc/passwd"`, and so does `GET /files/..%2F..%2Fetc/passwd` read through
//!   `NamedPath<T>`. A handler that joins the value onto a directory has to reject
//!   `..`, a root and a drive prefix itself, or resolve the joined path and check that it is
//!   still under that directory. The static file server (`use_static_files`) does this for
//!   the files it serves; a catch-all route does not.
//! - **It comes last in precedence.** At every position a literal segment is read first, a
//!   parameter second and a catch-all last, and the first position two routes differ at
//!   decides between them - whatever order they were mapped in, and however deep the path
//!   goes:
//!
//! ```
//!# use volga::App;
//! let mut app = App::new();
//!
//! app.map_get("/api/users/{id}", |id: u32| async move { id.to_string() });
//! app.map_get("/assets/{*path}", |path: String| async move { path });
//! app.map_get("/{lang}/{page}", |lang: String, page: String| async move { page });
//! app.map_get("/{*path}", |path: String| async move { path });
//!
//! // GET /api/users/7        -> /api/users/{id}
//! // GET /assets/app.js      -> /assets/{*path}, not /{lang}/{page}
//! // GET /en/home            -> /{lang}/{page}
//! // GET /api/users/7/extra  -> /{*path}, since nothing else reads all of it
//! ```
//!
//! - **It is the last segment.** A route continuing past one - including a route mapped
//!   inside a group whose prefix ends in one - panics where it is mapped.
//! - **It is named like any other parameter**, and [ambiguous routes](#ambiguous-routes)
//!   apply to it the same way: another verb may name the rest of the path something else,
//!   while one verb naming it twice panics.
//!
//! A catch-all is described in an OpenAPI document as the path parameter `{name}`, since
//! OpenAPI templates a path one segment at a time and has no spelling for a value spanning
//! several. A client generated from that document may percent-encode the `/` in the value
//! it sends, and a positional extractor reads that value undecoded, as `%2F`. The same
//! templating leaves no room for a catch-all beside a parameter route mapped for the same
//! verb at the same position - `/files/{name}` and `/files/{*path}` - in one document, so
//! where both are bound to a document the parameter route is described there and the
//! catch-all is left out, with a warning at startup in debug builds. A document only the
//! catch-all is bound to still describes it.
//!
//! # Ambiguous routes
//!
//! A route parameter is matched by the position it sits at rather than by what it is
//! called: a request for `/users/42` takes the route mapped for `/users/{id}` whatever the
//! placeholder is named, and the name decides only what the value is bound as. Every route
//! running through a position shares it, and each one binds its request under the names its
//! own pattern was written with - so two verbs may call one position two things, and both
//! be right:
//!
//! ```
//!# use volga::App;
//! let mut app = App::new();
//!
//! // reading a user by id, and creating one by name
//! app.map_get("/users/{id}", |id: String| async move { id });
//! app.map_post("/users/{name}", |name: String| async move { name });
//! ```
//!
//! Two cases cannot be told apart that way, and mapping one panics where it is written
//! rather than at the first request that shows a route is gone.
//!
//! **One verb, named twice.** The second registration replaces the first - a handler and
//! its middleware are written together, so mapping a handler where one is already mapped
//! takes the whole registration with it - and a different parameter name says that is not
//! what was meant:
//!
//! ```should_panic
//!# use volga::App;
//! let mut app = App::new();
//!
//! app.map_get("/users/{id}", |id: String| async move { id });
//! app.map_get("/users/{name}", |name: String| async move { name }); // panics
//! ```
//!
//! **`GET` and `HEAD`.** A `HEAD` request that has no route of its own is answered by the
//! `GET` route (RFC 9110 Section 9.3.2), so the two describe one resource and cannot
//! disagree about what identifies it. A `HEAD` mapped by hand - which takes the pattern
//! over from the endpoint standing in for the `GET` - is written under the name that route
//! already carries:
//!
//! ```
//!# use volga::{App, ok};
//! let mut app = App::new();
//!
//! app.map_get("/users/{id}", |id: String| async move { id });
//! app.map_head("/users/{id}", || async { ok!() }); // the GET route's own HEAD
//! ```
//!
//! A group prefix is a route pattern like any other, so a parameter it carries occupies a
//! position the same way one written on a route does. Two routes on one verb are told apart
//! by a literal segment - `/users/me` beside `/users/{id}`, matched first because it is
//! literal - rather than by giving one position two names.

use crate::App;
use crate::error::FallbackFunc;
use crate::http::endpoints::{
    args::FromRequest,
    handlers::{Func, GenericHandler, RouteHandler},
    route::{canonical_path, is_canonical_path, join_path},
};

#[cfg(any(feature = "middleware", feature = "openapi"))]
use crate::http::endpoints::route::same_position;
use crate::http::{FromRequestParts, IntoResponse};
use hyper::Method;
use std::borrow::Cow;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;

#[cfg(feature = "openapi")]
use crate::openapi::{OpenApiRouteConfig, RouteKey};

#[cfg(feature = "middleware")]
use {crate::http::cors::CorsOverride, crate::middleware::MiddlewareFn};

#[cfg(feature = "static-files")]
use crate::fs::static_files::StaticMount;

const QUERY: &[u8] = b"QUERY";

/// Routes mapping
impl App {
    /// Maps a group of request handlers combined by `prefix`
    ///
    /// # Ordering
    /// A group is a scope: its middleware, CORS policy and OpenAPI configuration are
    /// applied to every route the group registered once the closure returns, so they
    /// may be registered before or after the `map_*` calls they apply to.
    ///
    /// Middleware still runs in the order it was registered on the group, and a group's
    /// middleware runs before the middleware of a route or a sub-group inside it.
    ///
    /// # Examples
    /// ```no_run
    /// use volga::{App, Json, ok};
    ///# #[derive(serde::Deserialize, serde::Serialize)]
    ///# struct User;
    ///# #[tokio::main]
    ///# async fn main() -> std::io::Result<()> {
    /// let mut app = App::new();
    ///
    /// app.group("/user", |api| {
    ///     api.map_get("/{id}", |id: i32| async move {
    ///         // get the user from somewhere
    ///         let user: User = get_user();
    ///         ok!(user)
    ///     });
    ///     api.map_post("/create", |user: Json<User>| async move {
    ///         // create a user somewhere
    ///         let user_id = create_user(user);
    ///         ok!(user_id)
    ///     });
    /// });
    ///# app.run().await
    ///# }
    ///# fn get_user() -> User { unimplemented!() }
    ///# fn create_user(user: Json<User>) -> i32 { unimplemented!() }
    /// ```
    pub fn group<F>(&mut self, prefix: &str, f: F)
    where
        F: FnOnce(&mut RouteGroup<'_>),
    {
        let mut group = RouteGroup::new(self, prefix);

        #[cfg(feature = "openapi")]
        group.open_api(|cfg| cfg.with_tag(prefix));

        f(&mut group);
        group.apply();

        #[cfg(feature = "static-files")]
        group.mount_static();
    }

    /// Adds a request handler that matches HTTP GET requests for the specified pattern.
    ///
    /// # Examples
    /// ```no_run
    /// use volga::{App, ok};
    ///
    ///# #[tokio::main]
    ///# async fn main() -> std::io::Result<()> {
    /// let mut app = App::new();
    ///
    /// app.map_get("/hello", || async {
    ///    ok!("Hello World!")
    /// });
    ///# app.run().await
    ///# }
    /// ```
    ///
    /// # Panics
    /// if `pattern` is a second name for a route already mapped for this verb, or for the
    /// `GET` that a `HEAD` answers. See
    /// [Ambiguous routes](crate::app::router#ambiguous-routes).
    pub fn map_get<'a, F, R, Args, M>(&'a mut self, pattern: &'a str, handler: F) -> Route<'a>
    where
        F: GenericHandler<Args, M, Output = R>,
        R: IntoResponse + 'static,
        Args: FromRequest + Send + 'static,
        M: 'static,
    {
        self.map_route(Method::GET, pattern, handler)
    }

    /// Adds a request handler that matches HTTP POST requests for the specified pattern.
    ///
    /// # Examples
    /// ```no_run
    /// use volga::{App, File, ok};
    ///
    ///# #[tokio::main]
    ///# async fn main() -> std::io::Result<()> {
    /// let mut app = App::new();
    ///
    /// app.map_post("/upload", |file: File| async move {
    ///     file.save_as("example.txt").await?;
    ///     ok!()
    /// });
    ///# app.run().await
    ///# }
    /// ```
    ///
    /// # Panics
    /// if `pattern` is a second name for a route already mapped for this verb, or for the
    /// `GET` that a `HEAD` answers. See
    /// [Ambiguous routes](crate::app::router#ambiguous-routes).
    pub fn map_post<'a, F, R, Args, M>(&'a mut self, pattern: &'a str, handler: F) -> Route<'a>
    where
        F: GenericHandler<Args, M, Output = R>,
        R: IntoResponse + 'static,
        Args: FromRequest + Send + 'static,
        M: 'static,
    {
        self.map_route(Method::POST, pattern, handler)
    }

    /// Adds a request handler that matches HTTP PUT requests for the specified pattern.
    ///
    /// # Examples
    /// ```no_run
    /// use volga::{App, ok};
    ///
    ///# #[tokio::main]
    ///# async fn main() -> std::io::Result<()> {
    /// let mut app = App::new();
    ///
    /// app.map_put("/hello", || async {
    ///    ok!("Hello World!")
    /// });
    ///# app.run().await
    ///# }
    /// ```
    ///
    /// # Panics
    /// if `pattern` is a second name for a route already mapped for this verb, or for the
    /// `GET` that a `HEAD` answers. See
    /// [Ambiguous routes](crate::app::router#ambiguous-routes).
    pub fn map_put<'a, F, R, Args, M>(&'a mut self, pattern: &'a str, handler: F) -> Route<'a>
    where
        F: GenericHandler<Args, M, Output = R>,
        R: IntoResponse + 'static,
        Args: FromRequest + Send + 'static,
        M: 'static,
    {
        self.map_route(Method::PUT, pattern, handler)
    }

    /// Adds a request handler that matches HTTP PATCH requests for the specified pattern.
    ///
    /// # Examples
    /// ```no_run
    /// use volga::{App, ok};
    ///
    ///# #[tokio::main]
    ///# async fn main() -> std::io::Result<()> {
    /// let mut app = App::new();
    ///
    /// app.map_patch("/hello", || async {
    ///    ok!("Hello World!")
    /// });
    ///# app.run().await
    ///# }
    /// ```
    ///
    /// # Panics
    /// if `pattern` is a second name for a route already mapped for this verb, or for the
    /// `GET` that a `HEAD` answers. See
    /// [Ambiguous routes](crate::app::router#ambiguous-routes).
    pub fn map_patch<'a, F, R, Args, M>(&'a mut self, pattern: &'a str, handler: F) -> Route<'a>
    where
        F: GenericHandler<Args, M, Output = R>,
        R: IntoResponse + 'static,
        Args: FromRequest + Send + 'static,
        M: 'static,
    {
        self.map_route(Method::PATCH, pattern, handler)
    }

    /// Adds a request handler that matches HTTP DELETE requests for the specified pattern.
    ///
    /// # Examples
    /// ```no_run
    /// use volga::{App, ok};
    ///
    ///# #[tokio::main]
    ///# async fn main() -> std::io::Result<()> {
    /// let mut app = App::new();
    ///
    /// app.map_delete("/delete/{id}", |id: i32| async move {
    ///    ok!("Item with ID: {} has been removed!", id)
    /// });
    ///# app.run().await
    ///# }
    /// ```
    ///
    /// # Panics
    /// if `pattern` is a second name for a route already mapped for this verb, or for the
    /// `GET` that a `HEAD` answers. See
    /// [Ambiguous routes](crate::app::router#ambiguous-routes).
    pub fn map_delete<'a, F, R, Args, M>(&'a mut self, pattern: &'a str, handler: F) -> Route<'a>
    where
        F: GenericHandler<Args, M, Output = R>,
        R: IntoResponse + 'static,
        Args: FromRequest + Send + 'static,
        M: 'static,
    {
        self.map_route(Method::DELETE, pattern, handler)
    }

    /// Adds a request handler that matches HTTP HEAD requests for the specified pattern.
    ///
    /// # Examples
    /// ```no_run
    /// use volga::{App, ok};
    ///
    ///# #[tokio::main]
    ///# async fn main() -> std::io::Result<()> {
    /// let mut app = App::new();
    ///
    /// app.map_head("/resource/{id}", |id: i32| async move {
    ///    ok!([("Custom-Header", "value")])
    /// });
    ///# app.run().await
    ///# }
    /// ```
    ///
    /// # Panics
    /// if `pattern` is a second name for a route already mapped for this verb, or for the
    /// `GET` that a `HEAD` answers. See
    /// [Ambiguous routes](crate::app::router#ambiguous-routes).
    pub fn map_head<'a, F, R, Args, M>(&'a mut self, pattern: &'a str, handler: F) -> Route<'a>
    where
        F: GenericHandler<Args, M, Output = R>,
        R: IntoResponse + 'static,
        Args: FromRequest + Send + 'static,
        M: 'static,
    {
        self.map_route(Method::HEAD, pattern, handler)
    }

    /// Adds a request handler that matches HTTP OPTIONS requests for the specified pattern.
    ///
    /// # Examples
    /// ```no_run
    /// use volga::{App, ok};
    ///
    ///# #[tokio::main]
    ///# async fn main() -> std::io::Result<()> {
    /// let mut app = App::new();
    ///
    /// app.map_options("/resource/{id}", |id: i32| async move {
    ///    ok!([("Allow", "GET, HEAD, POST, OPTIONS")])
    /// });
    ///# app.run().await
    ///# }
    /// ```
    ///
    /// # Panics
    /// if `pattern` is a second name for a route already mapped for this verb, or for the
    /// `GET` that a `HEAD` answers. See
    /// [Ambiguous routes](crate::app::router#ambiguous-routes).
    pub fn map_options<'a, F, R, Args, M>(&'a mut self, pattern: &'a str, handler: F) -> Route<'a>
    where
        F: GenericHandler<Args, M, Output = R>,
        R: IntoResponse + 'static,
        Args: FromRequest + Send + 'static,
        M: 'static,
    {
        self.map_route(Method::OPTIONS, pattern, handler)
    }

    /// Adds a request handler that matches HTTP TRACE requests for the specified pattern.
    ///
    /// # Examples
    /// ```no_run
    /// use volga::{App, ok};
    ///
    ///# #[tokio::main]
    ///# async fn main() -> std::io::Result<()> {
    /// let mut app = App::new();
    ///
    /// app.map_trace("/", |id: i32| async move {
    ///    ok!([("content-type", "message/http")])
    /// });
    ///# app.run().await
    ///# }
    /// ```
    ///
    /// # Panics
    /// if `pattern` is a second name for a route already mapped for this verb, or for the
    /// `GET` that a `HEAD` answers. See
    /// [Ambiguous routes](crate::app::router#ambiguous-routes).
    pub fn map_trace<'a, F, R, Args, M>(&'a mut self, pattern: &'a str, handler: F) -> Route<'a>
    where
        F: GenericHandler<Args, M, Output = R>,
        R: IntoResponse + 'static,
        Args: FromRequest + Send + 'static,
        M: 'static,
    {
        self.map_route(Method::TRACE, pattern, handler)
    }

    /// Adds a request handler that matches HTTP CONNECT requests for the specified pattern.
    ///
    /// # Examples
    /// ```no_run
    /// use volga::{App, status};
    ///
    ///# #[tokio::main]
    ///# async fn main() -> std::io::Result<()> {
    /// let mut app = App::new();
    ///
    /// app.map_connect("/", || async {
    ///    status!(101)
    /// });
    ///# app.run().await
    ///# }
    /// ```
    ///
    /// # Panics
    /// if `pattern` is a second name for a route already mapped for this verb, or for the
    /// `GET` that a `HEAD` answers. See
    /// [Ambiguous routes](crate::app::router#ambiguous-routes).
    pub fn map_connect<'a, F, R, Args, M>(&'a mut self, pattern: &'a str, handler: F) -> Route<'a>
    where
        F: GenericHandler<Args, M, Output = R>,
        R: IntoResponse + 'static,
        Args: FromRequest + Send + 'static,
        M: 'static,
    {
        self.map_route(Method::CONNECT, pattern, handler)
    }

    /// Adds a request handler that matches HTTP QUERY requests for the specified pattern.
    ///
    /// > **Note:** Prefer putting complex selection criteria in the request body.
    /// > Use URI query parameters only for routing/cache-affecting metadata such as tenant,
    /// > locale, version, flags, or pagination compatibility.
    ///
    /// # Examples
    /// ```no_run
    /// use volga::{App, Json, ok};
    /// use serde::Deserialize;
    ///
    /// #[derive(Deserialize)]
    /// struct SearchQuery {
    ///     criteria: String
    /// }
    ///
    ///# #[tokio::main]
    ///# async fn main() -> std::io::Result<()> {
    /// let mut app = App::new();
    ///
    /// app.map_query("/search", |query: Json<SearchQuery>| async {
    ///    // do search by query.criteria....
    ///    ok!("search, result...")
    /// });
    ///# app.run().await
    ///# }
    /// ```
    ///
    /// # Panics
    /// if `pattern` is a second name for a route already mapped for this verb, or for the
    /// `GET` that a `HEAD` answers. See
    /// [Ambiguous routes](crate::app::router#ambiguous-routes).
    pub fn map_query<'a, F, R, Args, M>(&'a mut self, pattern: &'a str, handler: F) -> Route<'a>
    where
        F: GenericHandler<Args, M, Output = R>,
        R: IntoResponse + 'static,
        Args: FromRequest + Send + 'static,
        M: 'static,
    {
        let method = Method::from_bytes(QUERY).expect("invalid QUERY verb");
        self.map_route(method, pattern, handler)
    }

    /// Adds a request handler that matches the given HTTP `method` for the specified pattern.
    ///
    /// This is a generic counterpart to [`map_get`](Self::map_get) and friends, useful when the
    /// method is only known at runtime, when registering the same handler for several methods,
    /// or for non-standard verbs. The `method` accepts both a typed [`Method`] and a string
    /// (e.g. `"QUERY"`), and the `pattern` accepts both a borrowed `&str` (no allocation) and an
    /// owned [`String`] (e.g. built at runtime).
    ///
    /// # Examples
    /// ```no_run
    /// use volga::{App, ok};
    /// use volga::http::Method;
    ///
    ///# #[tokio::main]
    ///# async fn main() -> std::io::Result<()> {
    /// let mut app = App::new();
    ///
    /// app.map(Method::GET, "/hello", || async {
    ///    ok!("Hello World!")
    /// });
    ///
    /// // a string verb and a runtime-built pattern work as well
    /// app.map("QUERY", format!("/search/{}", "v1"), || async {
    ///    ok!("search, result...")
    /// });
    ///# app.run().await
    ///# }
    /// ```
    ///
    /// # Panics
    /// if `method` cannot be converted into a valid [`Method`], or if `pattern` is a second
    /// name for a route already mapped for `method`, or for the `GET` that a `HEAD`
    /// answers. See [Ambiguous routes](crate::app::router#ambiguous-routes).
    pub fn map<'a, V, P, F, R, Args, M>(
        &'a mut self,
        method: V,
        pattern: P,
        handler: F,
    ) -> Route<'a>
    where
        V: TryInto<Method>,
        V::Error: std::fmt::Debug,
        P: Into<Cow<'a, str>>,
        F: GenericHandler<Args, M, Output = R>,
        R: IntoResponse + 'static,
        Args: FromRequest + Send + 'static,
        M: 'static,
    {
        let method = method.try_into().expect("invalid HTTP method");
        self.map_route_impl(method, pattern.into(), handler)
    }

    #[inline]
    fn map_route<'a, F, R, Args, M>(
        &'a mut self,
        method: Method,
        pattern: &'a str,
        handler: F,
    ) -> Route<'a>
    where
        F: GenericHandler<Args, M, Output = R>,
        R: IntoResponse + 'static,
        Args: FromRequest + Send + 'static,
        M: 'static,
    {
        self.map_route_impl(method, Cow::Borrowed(pattern), handler)
    }

    #[inline]
    fn map_route_owned<F, R, Args, M>(
        &mut self,
        method: Method,
        pattern: String,
        handler: F,
    ) -> Route<'_>
    where
        F: GenericHandler<Args, M, Output = R>,
        R: IntoResponse + 'static,
        Args: FromRequest + Send + 'static,
        M: 'static,
    {
        self.map_route_impl(method, Cow::Owned(pattern), handler)
    }

    #[inline]
    fn map_route_impl<'a, F, R, Args, M>(
        &'a mut self,
        method: Method,
        pattern: Cow<'a, str>,
        handler: F,
    ) -> Route<'a>
    where
        F: GenericHandler<Args, M, Output = R>,
        R: IntoResponse + 'static,
        Args: FromRequest + Send + 'static,
        M: 'static,
    {
        let handler = Func::new(handler);

        // A route is keyed by this string in more places than the route tree - the
        // OpenAPI operation among them - and the tree reads two spellings of one route as
        // one. Registering the name it reads keeps those places agreeing with it
        let pattern = if is_canonical_path(pattern.as_ref()) {
            pattern
        } else {
            Cow::Owned(canonical_path(pattern.as_ref()))
        };

        // use &str view only for registration
        let path: &str = pattern.as_ref();

        // A GET route answers HEAD requests as well, and is not mapped a second time for
        // it: routing hands a HEAD request with no route of its own to the GET route, so
        // that request travels through everything this one travels through
        self.pipeline
            .endpoints_mut()
            .map_route(method.clone(), path, handler.clone());

        #[cfg(feature = "openapi")]
        let openapi_key = {
            let key = RouteKey {
                method: method.clone(),
                pattern: path.into(),
            };

            let mut auto = Args::describe_openapi(OpenApiRouteConfig::default());
            auto = R::describe_openapi(auto);

            self.openapi.on_route_mapped(key.clone(), auto);
            key
        };

        Route {
            app: self,
            #[cfg(feature = "middleware")]
            method,
            #[cfg(feature = "middleware")]
            pattern,
            #[cfg(feature = "openapi")]
            openapi_key,
        }
    }
}

/// Represents a route reference
pub struct Route<'a> {
    pub(crate) app: &'a mut App,
    #[cfg(feature = "middleware")]
    pub(crate) method: Method,
    #[cfg(feature = "middleware")]
    pub(crate) pattern: Cow<'a, str>,
    #[cfg(feature = "openapi")]
    openapi_key: RouteKey,
}

/// A route registered by a [`RouteGroup`], remembered until the group closure returns
/// so that the group's configuration can be applied to it whatever the declaration order
#[cfg(any(feature = "middleware", feature = "openapi"))]
#[derive(Debug, Clone)]
pub(crate) struct GroupRoute {
    method: Method,
    pattern: Box<str>,
}

/// Represents a group of routes
pub struct RouteGroup<'a> {
    pub(crate) app: &'a mut App,
    pub(crate) prefix: String,
    /// Routes registered by this group and by its sub-groups
    #[cfg(any(feature = "middleware", feature = "openapi"))]
    pub(crate) routes: Vec<GroupRoute>,
    #[cfg(feature = "middleware")]
    pub(crate) middleware: Vec<MiddlewareFn>,
    /// The prefixes this group and its sub-groups mapped a fallback under
    #[cfg(feature = "middleware")]
    pub(crate) fallbacks: Vec<Box<str>>,
    /// The CORS policy of this group, if it configured one
    #[cfg(feature = "middleware")]
    pub(crate) cors: Option<CorsOverride>,
    /// Static file mounts this group asked for, registered once its closure returns
    #[cfg(feature = "static-files")]
    pub(crate) mounts: Vec<StaticMount>,
    #[cfg(feature = "openapi")]
    pub(crate) openapi_config: OpenApiRouteConfig,
}

impl std::fmt::Debug for Route<'_> {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Route(..)")
    }
}

impl std::fmt::Debug for RouteGroup<'_> {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("RouteGroup(..)")
    }
}

impl<'a> Deref for Route<'a> {
    type Target = App;

    #[inline]
    fn deref(&self) -> &Self::Target {
        self.app
    }
}

impl<'a> DerefMut for Route<'a> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.app
    }
}

#[cfg(feature = "openapi")]
impl<'a> Route<'a> {
    /// Configures OpenAPI metadata for this route.
    pub fn open_api<T>(self, config: T) -> Self
    where
        T: FnOnce(OpenApiRouteConfig) -> OpenApiRouteConfig,
    {
        let key = self.openapi_key.clone();
        self.app.openapi.update_route_config(&key, config);
        self
    }
}

impl<'a> RouteGroup<'a> {
    /// Remembers a route registered by this group so that the group's configuration
    /// reaches it when the group closure returns.
    #[inline]
    #[cfg_attr(
        not(any(feature = "middleware", feature = "openapi")),
        allow(unused_variables)
    )]
    fn record(&mut self, method: &Method, pattern: &str) {
        #[cfg(any(feature = "middleware", feature = "openapi"))]
        self.record_route(GroupRoute {
            method: method.clone(),
            pattern: Box::from(pattern),
        });
    }

    /// Remembers `route` as registered by this group, whether it mapped it itself or a
    /// sub-group did.
    ///
    /// Mapping a route where one is mapped replaces it rather than adding one, so the group
    /// configures it once. A route is compared the way the router reads it rather than the
    /// way it is spelled: `/users/{id:integer}` and `/users/{id}` are one route, and
    /// recording both would put the group's middleware in front of it twice. It is kept
    /// under the spelling it was mapped as last, which is the one the router and the OpenAPI
    /// document are left with. Two *names* for one parameter on one verb are an ambiguity of
    /// their own, reported where the second one is mapped.
    #[inline]
    #[cfg(any(feature = "middleware", feature = "openapi"))]
    fn record_route(&mut self, route: GroupRoute) {
        let recorded = self.routes.iter_mut().find(|recorded| {
            recorded.method == route.method && same_position(&recorded.pattern, &route.pattern)
        });

        match recorded {
            Some(recorded) => recorded.pattern = route.pattern,
            None => self.routes.push(route),
        }
    }

    /// Applies the group's configuration to every route it registered.
    ///
    /// Called once the group closure has returned, so a `wrap` / `with` / `cors_with` /
    /// `open_api` call reaches the routes above it as well as the ones below it.
    /// Middleware is inserted ahead of whatever the route already carries - the
    /// middleware of a route or of a nested group, which applied itself first - so an
    /// outer scope always wraps an inner one.
    pub(crate) fn apply(&mut self) {
        #[cfg(any(feature = "middleware", feature = "openapi"))]
        {
            // Taken out of `self` for the walk, so the routes can be read while the
            // application state they configure is borrowed mutably, and put back for
            // a parent group that has yet to apply its own configuration to them.
            let routes = std::mem::take(&mut self.routes);

            for route in routes.iter() {
                #[cfg(feature = "middleware")]
                {
                    let endpoints = self.app.pipeline.endpoints_mut();

                    if !self.middleware.is_empty() {
                        endpoints.prepend_layers(&route.method, &route.pattern, &self.middleware);
                    }
                    if let Some(cors) = self.cors.clone() {
                        endpoints.bind_cors_if_unset(&route.method, &route.pattern, cors);
                    }
                }

                #[cfg(feature = "openapi")]
                {
                    let key = RouteKey {
                        method: route.method.clone(),
                        pattern: route.pattern.as_ref().into(),
                    };
                    let group_config = self.openapi_config.clone();
                    self.app
                        .openapi
                        .update_route_config(&key, |cfg| cfg.merge_outer(&group_config));
                }
            }

            self.routes = routes;
        }

        // A fallback is not a route, so there is nothing to describe in an OpenAPI document,
        // but it answers under this group's prefix on the group's behalf, and it takes the
        // group's middleware and CORS policy the way a route does
        #[cfg(feature = "middleware")]
        for prefix in self.fallbacks.iter() {
            let endpoints = self.app.pipeline.endpoints_mut();

            if !self.middleware.is_empty() {
                endpoints.prepend_fallback_layers(prefix, &self.middleware);
            }
            if let Some(cors) = self.cors.clone() {
                endpoints.bind_fallback_cors_if_unset(prefix, cors);
            }
        }

        // A mount answers under this group's prefix rather than through a route, so it
        // takes the group's middleware here instead of having it attached to a route. It
        // carries the same pipeline a route carries, so an outer scope wraps an inner one
        // by the same rule.
        #[cfg(feature = "static-files")]
        for mount in self.mounts.iter_mut() {
            mount.prepend(&self.middleware);
        }
    }

    /// Registers the static file mounts this group asked for in the application's
    /// middleware pipeline.
    ///
    /// Called once the group's configuration has been applied, so every mount carries the
    /// middleware of the group that asked for it and of every group around it.
    #[cfg(feature = "static-files")]
    fn mount_static(&mut self) {
        for mount in std::mem::take(&mut self.mounts) {
            mount.mount(self.app);
        }
    }
}

#[cfg(feature = "openapi")]
impl<'a> RouteGroup<'a> {
    /// Configures OpenAPI metadata for this route group.
    pub fn open_api<T>(&mut self, config: T) -> &mut Self
    where
        T: FnOnce(OpenApiRouteConfig) -> OpenApiRouteConfig,
    {
        self.openapi_config = config(self.openapi_config.clone());
        self
    }
}

impl<'a> RouteGroup<'a> {
    /// Maps a sub-group of request handlers combined by `sub_prefix`.
    ///
    /// Inherits the parent group's middleware, CORS policy, and OpenAPI
    /// configuration. Any middleware or settings added to the sub-group
    /// apply only to routes within it (and any further nested groups),
    /// running after the parent's middleware.
    ///
    /// # Ordering
    /// Inheritance does not depend on where the sub-group sits: the parent applies its
    /// configuration to every route it and its sub-groups registered once its own
    /// closure returns, so a sub-group declared before the parent's `with` or
    /// `cors_with` inherits it all the same. A sub-group's own CORS policy replaces the
    /// parent's for its routes rather than being replaced by it.
    ///
    /// # Examples
    /// ```no_run
    /// use volga::{App, ok};
    ///
    ///# #[tokio::main]
    ///# async fn main() -> std::io::Result<()> {
    /// let mut app = App::new();
    ///
    /// app.group("/api", |api| {
    ///     api.map_get("/info", || async { ok!() });
    ///
    ///     api.group("/users", |users| {
    ///         users.map_get("/{id}", |id: i32| async move { ok!(id) });
    ///     });
    /// });
    ///# app.run().await
    ///# }
    /// ```
    pub fn group<F>(&mut self, sub_prefix: &str, f: F)
    where
        F: FnOnce(&mut RouteGroup<'_>),
    {
        let full_prefix = [self.prefix.as_str(), sub_prefix].concat();
        let mut child = RouteGroup {
            app: self.app,
            prefix: full_prefix,
            #[cfg(any(feature = "middleware", feature = "openapi"))]
            routes: Vec::new(),
            #[cfg(feature = "middleware")]
            middleware: Vec::new(),
            #[cfg(feature = "middleware")]
            fallbacks: Vec::new(),
            #[cfg(feature = "middleware")]
            cors: None,
            #[cfg(feature = "static-files")]
            mounts: Vec::new(),
            #[cfg(feature = "openapi")]
            openapi_config: OpenApiRouteConfig::default(),
        };

        #[cfg(feature = "openapi")]
        {
            let tag = child.prefix.clone();
            child.open_api(|cfg| cfg.with_tag(tag));
        }

        f(&mut child);
        child.apply();

        // Taken out while the sub-group is still being read, for the same reason as the
        // mounts below
        #[cfg(feature = "middleware")]
        let fallbacks = std::mem::take(&mut child.fallbacks);

        // A static file mount the sub-group asked for belongs to this group as well. It is
        // taken over before the routes below, so that the sub-group is done being read
        // before this group is read again.
        #[cfg(feature = "static-files")]
        self.mounts.append(&mut child.mounts);

        // Routes mapped by the sub-group belong to this group as well: this group's
        // configuration wraps whatever the sub-group has just applied to them. A route
        // both of them mapped is still one route, so it arrives here through the same
        // check as one this group mapped itself.
        #[cfg(any(feature = "middleware", feature = "openapi"))]
        for route in child.routes.drain(..) {
            self.record_route(route);
        }

        // ... and so do the fallbacks it mapped, by the same rule
        #[cfg(feature = "middleware")]
        for pattern in fallbacks {
            self.record_fallback(pattern);
        }
    }

    /// Maps a request handler that matches the given HTTP `method` for the specified pattern.
    ///
    /// See [`App::map`] for more details.
    pub fn map<V, P, F, R, Args, M>(&mut self, method: V, pattern: P, handler: F) -> Route<'_>
    where
        V: TryInto<Method>,
        V::Error: std::fmt::Debug,
        P: AsRef<str>,
        F: GenericHandler<Args, M, Output = R>,
        R: IntoResponse + 'static,
        Args: FromRequest + Send + 'static,
        M: 'static,
    {
        let method = method.try_into().expect("invalid HTTP method");
        let pattern = join_path(&self.prefix, pattern.as_ref());

        self.record(&method, &pattern);
        self.app.map_route_owned(method, pattern, handler)
    }

    /// Adds a fallback handler for the requests under this group's prefix that no route
    /// answers.
    ///
    /// It is [`App::map_fallback`] for one part of the URL space: a request aimed at a path
    /// under the prefix - or at the prefix itself - that no route is mapped at is answered
    /// here, whatever its method, instead of by the application's fallback. What a request
    /// is answered with is decided by the router the way it decides between routes, so:
    ///
    /// - **The most specific prefix wins.** A literal segment is read before a catch-all at
    ///   every position, so the fallback of `/api` answers `/api/nope` ahead of anything
    ///   mapped under `/` - a `/{*path}` route, or the fallback file of a static file mount
    ///   at the root - and the fallback of `/api/v2` answers `/api/v2/nope` ahead of that
    ///   of `/api`.
    /// - **A route answers first.** A route mapped at the path the request is aimed at
    ///   answers it, for its own method, and a request for another method is answered
    ///   `405` with the methods it does have - exactly as it is without a fallback. That
    ///   holds for a route mapped at the prefix itself as well.
    /// - **The group's middleware runs around it.** Whatever the group - and every group
    ///   around it - put in front of its routes is put in front of its fallback, so an
    ///   unauthenticated request for an unknown path under an `authorize`d group is refused
    ///   the way one for a known path is, rather than told which paths exist. The group's
    ///   CORS policy applies to it too.
    ///
    /// The handler takes the same arguments [`App::map_fallback`] does - anything
    /// implementing [`FromRequestParts`]. It binds the parameters its prefix declares and
    /// nothing else, the same at the prefix and below it, and the path the request was aimed
    /// at is there to read through [`Uri`](crate::http::Uri). Under a prefix that ends in a
    /// catch-all, `/files/{*path}`, it answers what that catch-all reads, and binds it as the
    /// prefix names it. A fallback is not a route: it is neither
    /// listed with the routes nor described in an OpenAPI document, and
    /// [`HttpContext::matched_route`](crate::middleware::HttpContext::matched_route) reads
    /// `false` for a request it answers, so a CORS preflight for a path only a fallback
    /// answers is not answered as though that path were an endpoint.
    ///
    /// Mapping a second fallback where one is mapped replaces it, together with the
    /// middleware bound to it.
    ///
    /// # Example
    /// ```no_run
    /// use volga::{App, http::Uri, not_found, ok};
    ///
    ///# #[tokio::main]
    ///# async fn main() -> std::io::Result<()> {
    /// let mut app = App::new();
    ///
    /// app.group("/api", |api| {
    ///     api.map_get("/models", || async { ok!("models") });
    ///
    ///     // GET /api/nope, POST /api/v1/whatever, DELETE /api -> 404 from the API
    ///     // POST /api/models                               -> 405, a route is there
    ///     api.map_fallback(|uri: Uri| async move {
    ///         not_found!("no endpoint at {}", uri.path())
    ///     });
    /// });
    ///# app.run().await
    ///# }
    /// ```
    pub fn map_fallback<F, Args, R, M>(&mut self, handler: F) -> &mut Self
    where
        F: GenericHandler<Args, M, Output = R>,
        Args: FromRequestParts + Send + 'static,
        R: IntoResponse + 'static,
        M: 'static,
    {
        let handler: RouteHandler = Arc::new(FallbackFunc::new(handler));
        let prefix = join_path(&self.prefix, "");

        self.app
            .pipeline
            .endpoints_mut()
            .map_fallback(&prefix, handler);

        #[cfg(feature = "middleware")]
        self.record_fallback(prefix.into());

        self
    }

    /// Remembers a fallback mapped under `prefix` by this group or by one of its sub-groups,
    /// so that the group's configuration reaches it when the group closure returns.
    ///
    /// A second fallback under one prefix replaces the first rather than adding one, so the
    /// group configures it once - and a prefix is compared the way the router reads it, not
    /// the way it is spelled. Two sub-groups under `/{tenant}` and `/{org}` map their
    /// fallbacks at one resource, and recording both spellings would put this group's
    /// middleware in front of the one fallback left there twice.
    #[inline]
    #[cfg(feature = "middleware")]
    fn record_fallback(&mut self, prefix: Box<str>) {
        if !self
            .fallbacks
            .iter()
            .any(|recorded| same_position(recorded, &prefix))
        {
            self.fallbacks.push(prefix);
        }
    }
}

macro_rules! define_route_group_methods {
    ($(($fn_name:ident, $http_method:expr))*) => {
        impl<'a> RouteGroup<'a> {
            fn new(app: &'a mut App, prefix: &str) -> Self {
                RouteGroup {
                    app,
                    prefix: prefix.to_string(),
                    #[cfg(any(feature = "middleware", feature = "openapi"))]
                    routes: Vec::with_capacity(4),
                    #[cfg(feature = "middleware")]
                    middleware: Vec::with_capacity(4),
                    #[cfg(feature = "middleware")]
                    fallbacks: Vec::new(),
                    #[cfg(feature = "middleware")]
                    cors: None,
                    #[cfg(feature = "static-files")]
                    mounts: Vec::new(),
                    #[cfg(feature = "openapi")]
                    openapi_config: OpenApiRouteConfig::default(),
                }
            }

            $(
            #[doc = concat!("See [`App::", stringify!($fn_name), "`] for more details.")]
            pub fn $fn_name<F, R, Args, M>(&mut self, pattern: &str, handler: F) -> Route<'_>
            where
                F: GenericHandler<Args, M, Output = R>,
                R: IntoResponse + 'static,
                Args: FromRequest + Send + 'static,
                M: 'static,
            {
                let method = $http_method;
                let pattern = join_path(&self.prefix, pattern);

                self.record(&method, &pattern);
                self.app.map_route_owned(method, pattern, handler)
            }
            )*
        }
    };
}

define_route_group_methods! {
    (map_get, Method::GET)
    (map_post, Method::POST)
    (map_put, Method::PUT)
    (map_patch, Method::PATCH)
    (map_delete, Method::DELETE)
    (map_head, Method::HEAD)
    (map_options, Method::OPTIONS)
    (map_trace, Method::TRACE)
    (map_connect, Method::CONNECT)
    (map_query, Method::from_bytes(QUERY).expect("invalid QUERY verb"))
}

#[cfg(test)]
mod tests {
    // Every test here is about what a group records, which is only tracked when there is
    // something to configure with it
    #[cfg(any(feature = "middleware", feature = "openapi"))]
    use super::*;

    /// A group holds one mount for itself and one for each nested group that asked for
    /// one, which is what the `Vec` is for: a parent and a child mount different prefixes
    /// and both have to reach the application pipeline.
    #[cfg(feature = "static-files")]
    #[test]
    fn it_holds_a_mount_for_itself_and_for_each_nested_group() {
        let mut app = App::new();
        let mut counts = Vec::new();

        app.group("/self", |g| {
            g.use_static_assets();
            counts.push(g.mounts.len());
        });

        app.group("/child", |g| {
            g.group("/inner", |inner| {
                inner.use_static_assets();
            });
            counts.push(g.mounts.len());
        });

        app.group("/both", |g| {
            g.use_static_assets();
            g.group("/inner", |inner| {
                inner.use_static_assets();
            });
            counts.push(g.mounts.len());
        });

        app.group("/neither", |g| {
            g.map_get("/route", || async { "a route and nothing else" });
            counts.push(g.mounts.len());
        });

        assert_eq!(counts, vec![1, 1, 2, 0]);
    }

    #[cfg(any(feature = "middleware", feature = "openapi"))]
    #[test]
    fn it_records_routes_mapped_in_a_group() {
        let mut app = App::new();
        let mut routes = Vec::new();

        app.group("/api", |api| {
            api.map_get("/hello", || async { "Hello, World!" });
            api.map_post("/hello", || async { "Hello, World!" });
            routes = api.routes.clone();
        });

        let mapped = routes
            .iter()
            .map(|route| (route.method.clone(), route.pattern.to_string()))
            .collect::<Vec<_>>();

        assert_eq!(
            mapped,
            vec![
                (Method::GET, "/api/hello".to_string()),
                (Method::POST, "/api/hello".to_string()),
            ]
        );
    }

    /// A route mapped again under another spelling of its parameters is one route, recorded
    /// once under the spelling it was mapped as last - by the group itself or by a sub-group
    #[cfg(any(feature = "middleware", feature = "openapi"))]
    #[test]
    fn it_records_a_route_once_under_the_spelling_it_was_mapped_as_last() {
        let mut app = App::new();
        let mut routes = Vec::new();

        app.group("/api", |api| {
            api.map_get("/{id:integer}", || async { "typed" });
            api.map_get("/{id}", || async { "plain" });
            api.map_post("/{id}", || async { "posted" });

            api.map_get("/users/{id}", || async { "parent" });
            api.group("/users", |users| {
                users.map_get("/{id:integer}", || async { "child" });
            });

            routes = api.routes.clone();
        });

        let recorded = routes
            .iter()
            .map(|route| (route.method.clone(), route.pattern.to_string()))
            .collect::<Vec<_>>();

        assert_eq!(
            recorded,
            vec![
                (Method::GET, "/api/{id}".to_string()),
                (Method::POST, "/api/{id}".to_string()),
                (Method::GET, "/api/users/{id:integer}".to_string()),
            ]
        );
    }

    #[cfg(any(feature = "middleware", feature = "openapi"))]
    #[test]
    fn it_records_routes_mapped_by_a_sub_group_in_the_parent() {
        let mut app = App::new();
        let mut count = 0;

        app.group("/api", |api| {
            api.group("/users", |users| {
                users.map_get("/{id}", || async { "Hello, World!" });
            });
            count = api.routes.len();
        });

        assert_eq!(count, 1);
    }

    /// A fallback takes the configuration of every group around it, as a route does, and a
    /// fallback mapped twice at one prefix is one fallback to configure
    #[cfg(feature = "middleware")]
    #[test]
    fn it_records_the_fallbacks_of_a_group_and_its_sub_groups() {
        let mut app = App::new();
        let mut fallbacks = Vec::new();

        app.group("/api", |api| {
            api.map_fallback(|| async { "api" });
            api.group("/v2", |v2| {
                v2.map_fallback(|| async { "v2" });
                v2.map_fallback(|| async { "v2 again" });
            });
            api.group("", |same| {
                same.map_fallback(|| async { "api again" });
            });
            fallbacks = api.fallbacks.clone();
        });

        assert_eq!(
            fallbacks.iter().map(AsRef::as_ref).collect::<Vec<&str>>(),
            ["/api", "/api/v2"]
        );
    }

    /// A parameter is matched by the position it sits at, so two sub-groups naming or typing
    /// one position differently map their fallbacks at one resource, and it is recorded once
    #[cfg(feature = "middleware")]
    #[test]
    fn it_records_one_fallback_for_every_spelling_of_its_position() {
        let mut app = App::new();
        let mut fallbacks = Vec::new();

        app.group("/api", |api| {
            api.group("/{tenant}", |g| {
                g.map_fallback(|| async { "tenant" });
            });
            api.group("/{org}", |g| {
                g.map_fallback(|| async { "org" });
            });
            api.group("/{id:integer}", |g| {
                g.map_fallback(|| async { "id" });
            });
            fallbacks = api.fallbacks.clone();
        });

        assert_eq!(
            fallbacks.iter().map(AsRef::as_ref).collect::<Vec<&str>>(),
            ["/api/{tenant}"]
        );
    }

    /// A catch-all reads everything below the prefix it ends, and nothing can follow one, so
    /// a fallback under such a prefix is mapped at the prefix alone rather than panicking on
    /// a tail of its own
    #[test]
    fn it_maps_a_fallback_under_a_prefix_ending_in_a_catch_all() {
        let mut app = crate::App::new();

        app.group("/files/{*path}", |files| {
            files.map_fallback(|| async { "files" });
        });
    }

    #[cfg(any(feature = "middleware", feature = "openapi"))]
    #[test]
    fn it_does_not_record_an_empty_sub_group_in_the_parent() {
        let mut app = App::new();
        let mut count = 0;

        app.group("/api", |api| {
            api.group("/users", |_users| {});
            count = api.routes.len();
        });

        assert_eq!(count, 0);
    }
}
