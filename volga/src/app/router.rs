//! Route mapping helpers

use crate::App;
use crate::http::IntoResponse;
use crate::http::endpoints::{
    args::FromRequest,
    handlers::{Func, GenericHandler},
};
use hyper::Method;
use std::borrow::Cow;
use std::ops::{Deref, DerefMut};

#[cfg(feature = "openapi")]
use crate::openapi::{OpenApiRouteConfig, RouteKey};

#[cfg(feature = "middleware")]
use {crate::http::cors::CorsOverride, crate::middleware::MiddlewareFn};

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
    pub fn map_get<'a, F, R, Args>(&'a mut self, pattern: &'a str, handler: F) -> Route<'a>
    where
        F: GenericHandler<Args, Output = R>,
        R: IntoResponse + 'static,
        Args: FromRequest + Send + 'static,
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
    pub fn map_post<'a, F, R, Args>(&'a mut self, pattern: &'a str, handler: F) -> Route<'a>
    where
        F: GenericHandler<Args, Output = R>,
        R: IntoResponse + 'static,
        Args: FromRequest + Send + 'static,
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
    pub fn map_put<'a, F, R, Args>(&'a mut self, pattern: &'a str, handler: F) -> Route<'a>
    where
        F: GenericHandler<Args, Output = R>,
        R: IntoResponse + 'static,
        Args: FromRequest + Send + 'static,
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
    pub fn map_patch<'a, F, R, Args>(&'a mut self, pattern: &'a str, handler: F) -> Route<'a>
    where
        F: GenericHandler<Args, Output = R>,
        R: IntoResponse + 'static,
        Args: FromRequest + Send + 'static,
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
    pub fn map_delete<'a, F, R, Args>(&'a mut self, pattern: &'a str, handler: F) -> Route<'a>
    where
        F: GenericHandler<Args, Output = R>,
        R: IntoResponse + 'static,
        Args: FromRequest + Send + 'static,
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
    pub fn map_head<'a, F, R, Args>(&'a mut self, pattern: &'a str, handler: F) -> Route<'a>
    where
        F: GenericHandler<Args, Output = R>,
        R: IntoResponse + 'static,
        Args: FromRequest + Send + 'static,
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
    pub fn map_options<'a, F, R, Args>(&'a mut self, pattern: &'a str, handler: F) -> Route<'a>
    where
        F: GenericHandler<Args, Output = R>,
        R: IntoResponse + 'static,
        Args: FromRequest + Send + 'static,
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
    pub fn map_trace<'a, F, R, Args>(&'a mut self, pattern: &'a str, handler: F) -> Route<'a>
    where
        F: GenericHandler<Args, Output = R>,
        R: IntoResponse + 'static,
        Args: FromRequest + Send + 'static,
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
    pub fn map_connect<'a, F, R, Args>(&'a mut self, pattern: &'a str, handler: F) -> Route<'a>
    where
        F: GenericHandler<Args, Output = R>,
        R: IntoResponse + 'static,
        Args: FromRequest + Send + 'static,
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
    pub fn map_query<'a, F, R, Args>(&'a mut self, pattern: &'a str, handler: F) -> Route<'a>
    where
        F: GenericHandler<Args, Output = R>,
        R: IntoResponse + 'static,
        Args: FromRequest + Send + 'static,
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
    /// if `method` cannot be converted into a valid [`Method`].
    pub fn map<'a, M, P, F, R, Args>(&'a mut self, method: M, pattern: P, handler: F) -> Route<'a>
    where
        M: TryInto<Method>,
        M::Error: std::fmt::Debug,
        P: Into<Cow<'a, str>>,
        F: GenericHandler<Args, Output = R>,
        R: IntoResponse + 'static,
        Args: FromRequest + Send + 'static,
    {
        let method = method.try_into().expect("invalid HTTP method");
        self.map_route_impl(method, pattern.into(), handler)
    }

    #[inline]
    fn map_route<'a, F, R, Args>(
        &'a mut self,
        method: Method,
        pattern: &'a str,
        handler: F,
    ) -> Route<'a>
    where
        F: GenericHandler<Args, Output = R>,
        R: IntoResponse + 'static,
        Args: FromRequest + Send + 'static,
    {
        self.map_route_impl(method, Cow::Borrowed(pattern), handler)
    }

    #[inline]
    fn map_route_owned<F, R, Args>(
        &mut self,
        method: Method,
        pattern: String,
        handler: F,
    ) -> Route<'_>
    where
        F: GenericHandler<Args, Output = R>,
        R: IntoResponse + 'static,
        Args: FromRequest + Send + 'static,
    {
        self.map_route_impl(method, Cow::Owned(pattern), handler)
    }

    #[inline]
    fn map_route_impl<'a, F, R, Args>(
        &'a mut self,
        method: Method,
        pattern: Cow<'a, str>,
        handler: F,
    ) -> Route<'a>
    where
        F: GenericHandler<Args, Output = R>,
        R: IntoResponse + 'static,
        Args: FromRequest + Send + 'static,
    {
        let handler = Func::new(handler);

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
    /// The CORS policy of this group, if it configured one
    #[cfg(feature = "middleware")]
    pub(crate) cors: Option<CorsOverride>,
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
        self.routes.push(GroupRoute {
            method: method.clone(),
            pattern: Box::from(pattern),
        });
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
            cors: None,
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

        // Routes mapped by the sub-group belong to this group as well: this group's
        // configuration wraps whatever the sub-group has just applied to them.
        #[cfg(any(feature = "middleware", feature = "openapi"))]
        self.routes.append(&mut child.routes);
    }

    /// Maps a request handler that matches the given HTTP `method` for the specified pattern.
    ///
    /// See [`App::map`] for more details.
    pub fn map<M, P, F, R, Args>(&mut self, method: M, pattern: P, handler: F) -> Route<'_>
    where
        M: TryInto<Method>,
        M::Error: std::fmt::Debug,
        P: AsRef<str>,
        F: GenericHandler<Args, Output = R>,
        R: IntoResponse + 'static,
        Args: FromRequest + Send + 'static,
    {
        let method = method.try_into().expect("invalid HTTP method");
        let pattern = [self.prefix.as_str(), pattern.as_ref()].concat();

        self.record(&method, &pattern);
        self.app.map_route_owned(method, pattern, handler)
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
                    cors: None,
                    #[cfg(feature = "openapi")]
                    openapi_config: OpenApiRouteConfig::default(),
                }
            }

            $(
            #[doc = concat!("See [`App::", stringify!($fn_name), "`] for more details.")]
            pub fn $fn_name<F, R, Args>(&mut self, pattern: &str, handler: F) -> Route<'_>
            where
                F: GenericHandler<Args, Output = R>,
                R: IntoResponse + 'static,
                Args: FromRequest + Send + 'static,
            {
                let method = $http_method;
                let pattern = [self.prefix.as_str(), pattern].concat();

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
    use super::*;

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

        // The implicit HEAD twin is not recorded: the group looks it up when it
        // applies its configuration, so a HEAD mapped by hand later - which replaces
        // the twin - is not configured twice
        assert_eq!(
            mapped,
            vec![
                (Method::GET, "/api/hello".to_string()),
                (Method::POST, "/api/hello".to_string()),
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
