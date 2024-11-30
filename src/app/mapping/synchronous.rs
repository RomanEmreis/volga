use std::sync::Arc;
use hyper::Method;
use crate::{App, SyncEndpointsMapping, HttpRequest, HttpResult};

impl SyncEndpointsMapping for App {
    fn map_get<F>(&mut self, pattern: &str, handler: F)
    where
        F: Fn(HttpRequest) -> HttpResult + Send + Sync + 'static,
    {
        use crate::app::endpoints::handlers::SyncHandler;
        
        let endpoints = self.endpoints_mut();
        let handler = Arc::new(SyncHandler(handler));
        endpoints.map_route(Method::GET, pattern, handler);
    }

    fn map_post<F>(&mut self, pattern: &str, handler: F)
    where
        F: Fn(HttpRequest) -> HttpResult + Send + Sync + 'static,
    {
        use crate::app::endpoints::handlers::SyncHandler;

        let endpoints = self.endpoints_mut();
        let handler = Arc::new(SyncHandler(handler));
        endpoints.map_route(Method::POST, pattern, handler);
    }

    fn map_put<F>(&mut self, pattern: &str, handler: F)
    where
        F: Fn(HttpRequest) -> HttpResult + Send + Sync + 'static,
    {
        use crate::app::endpoints::handlers::SyncHandler;

        let endpoints = self.endpoints_mut();
        let handler = Arc::new(SyncHandler(handler));
        endpoints.map_route(Method::PUT, pattern, handler);
    }

    fn map_patch<F>(&mut self, pattern: &str, handler: F)
    where
        F: Fn(HttpRequest) -> HttpResult + Send + Sync + 'static,
    {
        use crate::app::endpoints::handlers::SyncHandler;

        let endpoints = self.endpoints_mut();
        let handler = Arc::new(SyncHandler(handler));
        endpoints.map_route(Method::PATCH, pattern, handler);
    }

    fn map_delete<F>(&mut self, pattern: &str, handler: F)
    where
        F: Fn(HttpRequest) -> HttpResult + Send + Sync + 'static,
    {
        use crate::app::endpoints::handlers::SyncHandler;

        let endpoints = self.endpoints_mut();
        let handler = Arc::new(SyncHandler(handler));
        endpoints.map_route(Method::DELETE, pattern, handler);
    }
}