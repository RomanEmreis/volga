use std::future::Future;
use std::sync::Arc;
use hyper::Method;
use crate::{App, HttpResult, HttpRequest, AsyncEndpointsMapping};

#[cfg(feature = "middleware")]
use crate::{HttpContext, Next, AsyncMiddlewareMapping};

use crate::app::endpoints::args::FromRequest;
use crate::app::endpoints::handlers::{GenericHandler};
use crate::app::endpoints::mapping::asynchronous::EndpointsMapping;

impl AsyncEndpointsMapping for App {
    fn map_get<F, Fut>(&mut self, pattern: &str, handler: F)
    where
        F: Fn(HttpRequest) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = HttpResult> + Send + 'static,
    {
        use crate::app::endpoints::handlers::AsyncHandler;

        let endpoints = self.endpoints_mut();
        let handler = Arc::new(AsyncHandler(handler));
        endpoints.map_route(Method::GET, pattern, handler);
    }

    fn map_post<F, Fut>(&mut self, pattern: &str, handler: F)
    where
        F: Fn(HttpRequest) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = HttpResult> + Send + 'static,
    {
        use crate::app::endpoints::handlers::AsyncHandler;

        let endpoints = self.endpoints_mut();
        let handler = Arc::new(AsyncHandler(handler));
        endpoints.map_route(Method::POST, pattern, handler);
    }

    fn map_put<F, Fut>(&mut self, pattern: &str, handler: F)
    where
        F: Fn(HttpRequest) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = HttpResult> + Send + 'static,
    {
        use crate::app::endpoints::handlers::AsyncHandler;

        let endpoints = self.endpoints_mut();
        let handler = Arc::new(AsyncHandler(handler));
        endpoints.map_route(Method::PUT, pattern, handler);
    }

    fn map_delete<F, Fut>(&mut self, pattern: &str, handler: F)
    where
        F: Fn(HttpRequest) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = HttpResult> + Send + 'static,
    {
        use crate::app::endpoints::handlers::AsyncHandler;

        let endpoints = self.endpoints_mut();
        let handler = Arc::new(AsyncHandler(handler));
        endpoints.map_route(Method::DELETE, pattern, handler);
    }

    fn map_patch<F, Fut>(&mut self, pattern: &str, handler: F)
    where
        F: Fn(HttpRequest) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = HttpResult> + Send + 'static,
    {
        use crate::app::endpoints::handlers::AsyncHandler;

        let endpoints = self.endpoints_mut();
        let handler = Arc::new(AsyncHandler(handler));
        endpoints.map_route(Method::PATCH, pattern, handler);
    }
}

#[cfg(feature = "middleware")]
impl AsyncMiddlewareMapping for App {
    fn use_middleware<F, Fut>(&mut self, handler: F)
    where
        F: Fn(HttpContext, Next) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = HttpResult> + Send,
    {
        let middlewares = self.middlewares_mut();
        middlewares.use_middleware(handler);
    }
}

impl EndpointsMapping for App {
    fn map_get<F, Args>(&mut self, pattern: &str, handler: F)
    where
        F: GenericHandler<Args, Output = HttpResult>,
        Args: FromRequest + Send + Sync + 'static
    {
        use crate::app::endpoints::handlers::Func;

        let endpoints = self.endpoints_mut();
        let handler = Arc::new(Func::new(handler));
        endpoints.map_route(Method::GET, pattern, handler);
    }

    fn map_post<F, Args>(&mut self, pattern: &str, handler: F)
    where
        F: GenericHandler<Args, Output = HttpResult>,
        Args: FromRequest + Send + Sync + 'static,
    {
        use crate::app::endpoints::handlers::Func;

        let endpoints = self.endpoints_mut();
        let handler = Arc::new(Func::new(handler));
        endpoints.map_route(Method::POST, pattern, handler);
    }
}