use std::future::Future;
use crate::{HttpResult, HttpRequest};
use crate::app::endpoints::args::FromRequest;
use crate::app::endpoints::handlers::GenericHandler;

pub trait AsyncEndpointsMapping {
    /// Adds a request handler that matches HTTP GET requests for the specified pattern.
    /// 
    /// # Examples
    /// ```no_run
    ///use volga::{App, AsyncEndpointsMapping, Results};
    ///
    ///#[tokio::main]
    ///async fn main() -> std::io::Result<()> {
    ///    let mut app = App::new();
    ///
    ///    app.map_get("/test", |_req| async {
    ///        Results::text("Pass!")
    ///    });
    ///
    ///    app.run().await
    ///}
    /// ```
    fn map_get<F, Fut>(&mut self, pattern: &str, handler: F)
    where
        F: Fn(HttpRequest) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = HttpResult> + Send + 'static;

    /// Adds a request handler that matches HTTP POST requests for the specified pattern.
    /// 
    /// # Examples
    /// ```no_run
    ///use volga::{App, AsyncEndpointsMapping, Results};
    ///
    ///#[tokio::main]
    ///async fn main() -> std::io::Result<()> {
    ///    let mut app = App::new();
    ///
    ///    app.map_post("/test", |_req| async {
    ///        Results::text("Pass!")
    ///    });
    ///
    ///    app.run().await
    ///}
    /// ```
    fn map_post<F, Fut>(&mut self, pattern: &str, handler: F)
    where
        F: Fn(HttpRequest) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = HttpResult> + Send + 'static;

    /// Adds a request handler that matches HTTP PUT requests for the specified pattern.
    /// 
    /// # Examples
    /// ```no_run
    ///use volga::{App, AsyncEndpointsMapping, Results};
    ///
    ///#[tokio::main]
    ///async fn main() -> std::io::Result<()> {
    ///    let mut app = App::new();
    ///
    ///    app.map_put("/test", |_req| async {
    ///        Results::text("Pass!")
    ///    });
    ///
    ///    app.run().await
    ///}
    /// ```
    fn map_put<F, Fut>(&mut self, pattern: &str, handler: F)
    where
        F: Fn(HttpRequest) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = HttpResult> + Send + 'static;

    /// Adds a request handler that matches HTTP DELETE requests for the specified pattern.
    /// 
    /// # Examples
    /// ```no_run
    ///use volga::{App, AsyncEndpointsMapping, Results};
    ///
    ///#[tokio::main]
    ///async fn main() -> std::io::Result<()> {
    ///    let mut app = App::new();
    ///
    ///    app.map_delete("/test", |_req| async {
    ///        Results::text("Pass!")
    ///    });
    ///
    ///    app.run().await
    ///}
    /// ```
    fn map_delete<F, Fut>(&mut self, pattern: &str, handler: F)
    where
        F: Fn(HttpRequest) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = HttpResult> + Send + 'static;

    /// Adds a request handler that matches HTTP PATCH requests for the specified pattern.
    /// 
    /// # Examples
    /// ```no_run
    ///use volga::{App, AsyncEndpointsMapping, Results};
    ///
    ///#[tokio::main]
    ///async fn main() -> std::io::Result<()> {
    ///    let mut app = App::new();
    ///
    ///    app.map_patch("/test", |_req| async {
    ///        Results::text("Pass!")
    ///    });
    ///
    ///    app.run().await
    ///}
    /// ```
    fn map_patch<F, Fut>(&mut self, pattern: &str, handler: F)
    where
        F: Fn(HttpRequest) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = HttpResult> + Send + 'static;
}

pub trait EndpointsMapping {
    /// Adds a request handler that matches HTTP GET requests for the specified pattern.
    /// 
    /// # Examples
    /// ```no_run
    ///use volga::{App, EndpointsMapping, ok};
    ///
    ///#[tokio::main]
    ///async fn main() -> std::io::Result<()> {
    ///    let mut app = App::new();
    ///
    ///    app.map_get("/hello", || async {
    ///        ok!("Hello World!")
    ///    });
    ///
    ///    app.run().await
    ///}
    /// ```
    fn map_get<F, Args>(&mut self, pattern: &str, handler: F)
    where
        F: GenericHandler<Args, Output = HttpResult>,
        Args: FromRequest + Send + Sync + 'static;

    /// Adds a request handler that matches HTTP POST requests for the specified pattern.
    /// 
    /// # Examples
    /// ```no_run
    ///use volga::{App, EndpointsMapping, ok};
    ///
    ///#[tokio::main]
    ///async fn main() -> std::io::Result<()> {
    ///    let mut app = App::new();
    ///
    ///    app.map_post("/hello", || async {
    ///        ok!("Hello World!")
    ///    });
    ///
    ///    app.run().await
    ///}
    /// ```
    fn map_post<F, Args>(&mut self, pattern: &str, handler: F)
    where
        F: GenericHandler<Args, Output = HttpResult>,
        Args: FromRequest + Send + Sync + 'static;
}