use std::future::Future;
use crate::{
    HttpContext, 
    HttpResult, 
    Next
};

pub trait AsyncMiddlewareMapping {
    /// Adds a middleware handler to the application request pipeline
    /// 
    /// # Examples
    /// ```no_run
    ///use volga::{App, AsyncMiddlewareMapping, Results};
    ///
    ///#[tokio::main]
    ///async fn main() -> std::io::Result<()> {
    ///    let mut app = App::new();
    ///
    ///    // Middleware 2
    ///    app.use_middleware(|ctx, next| async move {
    ///        // do something...
    ///        let response = next(ctx).await;
    ///        // do something...
    ///        response
    ///    });
    /// 
    ///    // Middleware 2
    ///    app.use_middleware(|ctx, next| async move {
    ///        next(ctx).await
    ///    });
    ///
    ///    app.run().await
    ///}
    /// ```
    fn use_middleware<F, Fut>(&mut self, handler: F)
    where
        F: Fn(HttpContext, Next) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = HttpResult> + Send;
}