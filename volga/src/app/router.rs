//! Route mapping helpers

use std::ops::{Deref, DerefMut};
use hyper::Method;
use crate::App;
use crate::http::IntoResponse;
use crate::http::endpoints::{
    args::FromRequest,
    handlers::{Func, GenericHandler},
};

#[cfg(feature = "middleware")]
use crate::middleware::MiddlewareFn;

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
    /// app.map_group("/user")
    ///     .map_get("/{id}", |id: i32| async move {
    ///         // get the user from somewhere
    ///         let user: User = get_user();
    ///         ok!(user)
    ///     })
    ///     .map_post("/create", |user: Json<User>| async move {
    ///         // create a user somewhere
    ///         let user_id = create_user(user);
    ///         ok!(user_id)
    ///     });
    ///# app.run().await
    ///# }
    ///# fn get_user() -> User { unimplemented!() }
    ///# fn create_user(user: Json<User>) -> i32 { unimplemented!() }
    /// ```
    pub fn map_group<'a>(&'a mut self, prefix: &'a str) -> RouteGroup<'a> {
        RouteGroup::new(self, prefix)
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
        Args: FromRequest + Send + Sync + 'static
    {
        let handler = Func::new(handler);
        let endpoints = self.pipeline.endpoints_mut();
        endpoints.map_route(Method::GET, pattern, handler.clone());
        
        let head = Method::HEAD;
        if !endpoints.contains(&head, pattern) { 
            endpoints.map_route(head, pattern, handler.clone());
        }

        #[cfg(feature = "middleware")]
        self.map_preflight_handler(pattern);
        
        Route { 
            app: self,
            #[cfg(feature = "middleware")]
            method: Method::GET,
            #[cfg(feature = "middleware")]
            pattern
        }
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
        Args: FromRequest + Send + Sync + 'static,
    {
        let handler = Func::new(handler);
        self.pipeline
            .endpoints_mut()
            .map_route(Method::POST, pattern, handler);

        #[cfg(feature = "middleware")]
        self.map_preflight_handler(pattern);

        Route {
            app: self,
            #[cfg(feature = "middleware")]
            method: Method::POST,
            #[cfg(feature = "middleware")]
            pattern
        }
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
        Args: FromRequest + Send + Sync + 'static,
    {
        let handler = Func::new(handler);
        self.pipeline
            .endpoints_mut()
            .map_route(Method::PUT, pattern, handler);

        #[cfg(feature = "middleware")]
        self.map_preflight_handler(pattern);

        Route {
            app: self,
            #[cfg(feature = "middleware")]
            method: Method::PUT,
            #[cfg(feature = "middleware")]
            pattern
        }
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
        Args: FromRequest + Send + Sync + 'static,
    {
        let handler = Func::new(handler);
        self.pipeline
            .endpoints_mut()
            .map_route(Method::PATCH, pattern, handler);

        #[cfg(feature = "middleware")]
        self.map_preflight_handler(pattern);

        Route {
            app: self,
            #[cfg(feature = "middleware")]
            method: Method::PATCH,
            #[cfg(feature = "middleware")]
            pattern
        }
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
        Args: FromRequest + Send + Sync + 'static,
    {
        let handler = Func::new(handler);
        self.pipeline
            .endpoints_mut()
            .map_route(Method::DELETE, pattern, handler);

        #[cfg(feature = "middleware")]
        self.map_preflight_handler(pattern);

        Route {
            app: self,
            #[cfg(feature = "middleware")]
            method: Method::DELETE,
            #[cfg(feature = "middleware")]
            pattern
        }
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
        Args: FromRequest + Send + Sync + 'static,
    {
        let handler = Func::new(handler);
        self.pipeline
            .endpoints_mut()
            .map_route(Method::HEAD, pattern, handler);

        #[cfg(feature = "middleware")]
        self.map_preflight_handler(pattern);

        Route {
            app: self,
            #[cfg(feature = "middleware")]
            method: Method::HEAD,
            #[cfg(feature = "middleware")]
            pattern
        }
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
        Args: FromRequest + Send + Sync + 'static,
    {
        let handler = Func::new(handler);
        self.pipeline
            .endpoints_mut()
            .map_route(Method::OPTIONS, pattern, handler);
        Route {
            app: self,
            #[cfg(feature = "middleware")]
            method: Method::OPTIONS,
            #[cfg(feature = "middleware")]
            pattern
        }
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
        Args: FromRequest + Send + Sync + 'static,
    {
        let handler = Func::new(handler);
        self.pipeline
            .endpoints_mut()
            .map_route(Method::TRACE, pattern, handler);

        #[cfg(feature = "middleware")]
        self.map_preflight_handler(pattern);

        Route {
            app: self,
            #[cfg(feature = "middleware")]
            method: Method::TRACE,
            #[cfg(feature = "middleware")]
            pattern
        }
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
        Args: FromRequest + Send + Sync + 'static,
    {
        let handler = Func::new(handler);
        self.pipeline
            .endpoints_mut()
            .map_route(Method::CONNECT, pattern, handler);

        #[cfg(feature = "middleware")]
        self.map_preflight_handler(pattern);
        Route {
            app: self,
            #[cfg(feature = "middleware")]
            method: Method::CONNECT,
            #[cfg(feature = "middleware")]
            pattern
        }
    }

    #[inline]
    #[cfg(feature = "middleware")]
    fn map_preflight_handler(&mut self, pattern: &str) {
        if self.cors_config.is_some() {
            let endpoints = self.pipeline.endpoints_mut();
            let options = Method::OPTIONS;
            if !endpoints.contains(&options, pattern) {
                endpoints.map_route(options, pattern, Func::new(|| async {}));
            }
        }
    }
}

/// Represents a route reference
pub struct Route<'a> {
    pub(crate) app: &'a mut App,
    #[cfg(feature = "middleware")]
    pub(crate) method: Method,
    #[cfg(feature = "middleware")]
    pub(crate) pattern: &'a str,
}

/// Represents a group of routes
pub struct RouteGroup<'a> {
    pub(crate) app: &'a mut App,
    pub(crate) prefix: &'a str,
    #[cfg(feature = "middleware")]
    pub(crate) middleware: Vec<MiddlewareFn>,
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

macro_rules! define_route_group_methods({$($method:ident)*} => {
    impl <'a> RouteGroup<'a> {
        /// Creates a new route group
        fn new(app: &'a mut App, prefix: &'a str) -> Self {
            RouteGroup { 
                app, 
                prefix,
                #[cfg(feature = "middleware")]
                middleware: Vec::with_capacity(4),
            }
        }
            
        $(
        #[doc = concat!("See [`App::", stringify!($method), "`] for more details.")]
        pub fn $method<F, R, Args>(self, pattern: &str, handler: F) -> Self
        where
            F: GenericHandler<Args, Output = R>,
            R: IntoResponse + 'static,
            Args: FromRequest + Send + Sync + 'static
        {
            let pattern = [self.prefix, pattern].concat();
            #[cfg(feature = "middleware")]
            {
                let mut route = self.app.$method(&pattern, handler);
                for filter in self.middleware.iter() {
                    route = route.map_middleware(filter.clone());
                }
                self
            }
            #[cfg(not(feature = "middleware"))]
            {
                self.app.$method(&pattern, handler);
                self
            }
        }
        )*
        }
});

define_route_group_methods! { 
    map_get
    map_post
    map_put
    map_patch
    map_delete
    map_head
    map_options
    map_trace
    map_connect
}
