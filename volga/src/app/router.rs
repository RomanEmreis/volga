//! Route mapping helpers

use std::ops::{Deref, DerefMut};
use std::borrow::Cow;
use hyper::Method;
use crate::App;
use crate::http::IntoResponse;
use crate::http::endpoints::{
    args::FromRequest,
    handlers::{Func, GenericHandler},
};

#[cfg(feature = "openapi")]
use crate::openapi::{OpenApiRouteConfig, RouteKey};

#[cfg(feature = "middleware")]
use {
    crate::middleware::MiddlewareFn,
    crate::http::cors::CorsOverride
};

/// Routes mapping 
impl App {
    /// Maps a group of request handlers combined by `prefix`
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
    pub fn group<'a, F>(&'a mut self, prefix: &'a str, f: F)
    where 
        F: FnOnce(&mut RouteGroup<'a>)
    {
        let mut group = RouteGroup::new(self, prefix);
        
        #[cfg(feature = "openapi")]
        group.open_api(|cfg| cfg.with_tag(prefix));
        
        f(&mut group);
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
        Args: FromRequest + Send + 'static
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
        let endpoints = self.pipeline.endpoints_mut();

        // use &str view only for registration
        let path: &str = pattern.as_ref();
        endpoints.map_route(method.clone(), path, handler.clone());

        if self.implicit_head && method == Method::GET {
            let head = Method::HEAD;
            if !endpoints.contains(&head, path) {
                endpoints.map_route(head, path, handler.clone());
            }
        }
        
        #[cfg(feature = "openapi")]
        let openapi_key = {
            let key = RouteKey { method: method.clone(), pattern: path.to_string() };
            
            let mut auto = Args::describe_openapi(OpenApiRouteConfig::default());
            auto = R::describe_openapi(auto);
            
            self.on_route_mapped(key.clone(), auto);
            key
        };

        Route {
            app: self,
            #[cfg(feature = "middleware")]
            method,
            #[cfg(feature = "middleware")]
            pattern,
            #[cfg(feature = "openapi")]
            openapi_key
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

/// Represents a group of routes
pub struct RouteGroup<'a> {
    pub(crate) app: &'a mut App,
    pub(crate) prefix: &'a str,
    pub(crate) route_count: usize,
    #[cfg(feature = "middleware")]
    pub(crate) middleware: Vec<MiddlewareFn>,
    #[cfg(feature = "middleware")]
    pub(crate) cors: CorsOverride,
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
        let state = &mut self.app.openapi_state;
        let entry = state
            .configs
            .get_mut(&key)
            .expect("route config missing");

        let current = std::mem::take(entry);
        let updated = config(current);
        *entry = updated;
        
        if let Some(reg) = self.app.openapi.as_ref() {
            //let cfg = state.configs.get(&key).unwrap();
            reg.rebind_route(&key.method, &key.pattern, entry);
        }

        self
    }
}

#[cfg(feature = "openapi")]
impl<'a> RouteGroup<'a> {
    /// Configures OpenAPI metadata for this route group.
    pub fn open_api<T>(&mut self, config: T) -> &mut Self
    where
        T: FnOnce(OpenApiRouteConfig) -> OpenApiRouteConfig,
    {
        if self.route_count > 0 {
            #[cfg(feature = "tracing")]
            tracing::warn!("RouteGroup::open_api must be called before any map_* in the group");
            #[cfg(not(feature = "tracing"))]
            eprintln!("RouteGroup::open_api must be called before any map_* in the group");
        }

        self.openapi_config = config(self.openapi_config.clone());
        self
    }
}

macro_rules! define_route_group_methods {
    ($(($fn_name:ident, $http_method:expr))*) => {
        impl<'a> RouteGroup<'a> {
            fn new(app: &'a mut App, prefix: &'a str) -> Self {
                RouteGroup {
                    app,
                    prefix,
                    route_count: 0,
                    #[cfg(feature = "middleware")]
                    middleware: Vec::with_capacity(4),
                    #[cfg(feature = "middleware")]
                    cors: CorsOverride::Inherit,
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
                self.route_count += 1;
                let pattern = [self.prefix, pattern].concat();

                #[cfg(feature = "middleware")]
                {
                    let mut route = self
                        .app
                        .map_route_owned($http_method, pattern, handler)
                        .cors_override(self.cors.clone());

                    for filter in self.middleware.iter() {
                        route = route.map_middleware(filter.clone());
                    }

                    #[cfg(feature = "openapi")]
                    {
                        let openapi_config = self.openapi_config.clone();
                        route = route.open_api(|config| config.merge(&openapi_config));
                    }

                    route
                }

                #[cfg(not(feature = "middleware"))]
                {
                    let route = self.app.map_route_owned($http_method, pattern, handler);

                    #[cfg(feature = "openapi")]
                    let route = {
                        let openapi_config = self.openapi_config.clone();
                        route.open_api(|config| config.merge(&openapi_config))
                    };

                    route
                }
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
}
