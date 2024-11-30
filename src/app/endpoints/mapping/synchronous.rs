use crate::{HttpResult, HttpRequest};

pub trait SyncEndpointsMapping {
    /// Adds a request handler that matches HTTP GET requests for the specified pattern.
    /// 
    /// # Examples
    /// ```no_run
    ///use volga::{App, SyncEndpointsMapping, Results};
    ///
    ///#[tokio::main]
    ///async fn main() -> std::io::Result<()> {
    ///    let mut app = App::new();
    ///
    ///    app.map_get("/test", |_req| {
    ///        Results::text("Pass!")
    ///    });
    ///
    ///    app.run().await
    ///}
    /// ```
    fn map_get<F>(&mut self, pattern: &str, handler: F)
    where
        F: Fn(HttpRequest) -> HttpResult + Send + Sync + 'static;

    /// Adds a request handler that matches HTTP POST requests for the specified pattern.
    /// 
    /// # Examples
    /// ```no_run
    ///use volga::{App, SyncEndpointsMapping, Results};
    ///
    ///#[tokio::main]
    ///async fn main() -> std::io::Result<()> {
    ///    let mut app = App::new();
    ///
    ///    app.map_post("/test", |_req| {
    ///        Results::text("Pass!")
    ///    });
    ///
    ///    app.run().await
    ///}
    /// ```
    fn map_post<F>(&mut self, pattern: &str, handler: F)
    where
        F: Fn(HttpRequest) -> HttpResult + Send + Sync + 'static;

    /// Adds a request handler that matches HTTP PUT requests for the specified pattern.
    /// 
    /// # Examples
    /// ```no_run
    ///use volga::{App, SyncEndpointsMapping, Results};
    ///
    ///#[tokio::main]
    ///async fn main() -> std::io::Result<()> {
    ///    let mut app = App::new();
    ///
    ///    app.map_put("/test", |_req| {
    ///        Results::text("Pass!")
    ///    });
    ///
    ///    app.run().await
    ///}
    /// ```
    fn map_put<F>(&mut self, pattern: &str, handler: F)
    where
        F: Fn(HttpRequest) -> HttpResult + Send + Sync + 'static;

    /// Adds a request handler that matches HTTP PATCH requests for the specified pattern.
    /// 
    /// # Examples
    /// ```no_run
    ///use volga::{App, SyncEndpointsMapping, Results};
    ///
    ///#[tokio::main]
    ///async fn main() -> std::io::Result<()> {
    ///    let mut app = App::new();
    ///
    ///    app.map_patch("/test", |_req| {
    ///        Results::text("Pass!")
    ///    });
    ///
    ///    app.run().await
    ///}
    /// ```
    fn map_patch<F>(&mut self, pattern: &str, handler: F)
    where
        F: Fn(HttpRequest) -> HttpResult + Send + Sync + 'static;

    /// Adds a request handler that matches HTTP DELETE requests for the specified pattern.
    /// 
    /// # Examples
    /// ```no_run
    ///use volga::{App, SyncEndpointsMapping, Results};
    ///
    ///#[tokio::main]
    ///async fn main() -> std::io::Result<()> {
    ///    let mut app = App::new();
    ///
    ///    app.map_delete("/test", |_req| {
    ///        Results::text("Pass!")
    ///    });
    ///
    ///    app.run().await
    ///}
    /// ```
    fn map_delete<F>(&mut self, pattern: &str, handler: F)
    where
        F: Fn(HttpRequest) -> HttpResult + Send + Sync + 'static;
}