//! Endpoints mapping utilities

use hyper::{Method, Uri};
use super::endpoints::{
    route::{RouteNode, RoutePipeline, PathArgs},
    handlers::RouteHandler
};

#[cfg(feature = "middleware")]
use super::endpoints::route::Layer;

#[cfg(debug_assertions)]
pub(crate) mod meta;
pub(crate) mod handlers;
pub(crate) mod route;
pub mod args;

const ALLOW_METHOD_SEPARATOR: &str = ",";

/// Describes a mapping between HTTP Verbs, routes and request handlers
pub(crate) struct Endpoints {
    routes: RouteNode
}

/// Specifies statuses that could be returned after route matching
pub(crate) enum RouteOption {
    RouteNotFound,
    MethodNotFound(String),
    Ok(EndpointContext)
}

/// Describes the context of the executing route
pub(crate) struct EndpointContext {
    pub(crate) pipeline: RoutePipeline,
    pub(crate) params: PathArgs
}

impl EndpointContext {
    pub(crate) fn into_parts(self) -> (RoutePipeline, PathArgs) {
        (self.pipeline, self.params)
    }

    fn new(pipeline: RoutePipeline, params: PathArgs) -> Self {
        Self { pipeline, params }
    }
}

impl Endpoints {
    /// Creates a new endpoints collection
    pub(crate) fn new() -> Self {
        Self { routes: RouteNode::new() }
    }

    /// Gets a context of the executing route by its `HttpRequest`
    #[inline]
    pub(crate) fn get_endpoint(&self, method: &Method, uri: &Uri) -> RouteOption {
        let route_params = match self.routes.find(uri.path()) {
            Some(params) => params,
            None => return RouteOption::RouteNotFound,
        };

        let Some(handlers) = &route_params.route.handlers else { 
            return RouteOption::RouteNotFound;
        };

        handlers
            .binary_search_by(|h| h.cmp(method))
            .map_or_else(
                |_| RouteOption::MethodNotFound(handlers.iter()
                    .map(|h| h.method.as_str())
                    .collect::<Vec<_>>()
                    .join(ALLOW_METHOD_SEPARATOR)),
                |i| RouteOption::Ok(
                    EndpointContext::new(handlers[i].pipeline.clone(), route_params.params)
            ))
    }

    /// Maps the request handler to the current HTTP Verb and route pattern
    #[inline]
    pub(crate) fn map_route(&mut self, method: Method, pattern: &str, handler: RouteHandler) {
        self.routes
            .insert(pattern, method, handler.into());
    }

    /// Maps the request layer to the current HTTP Verb and route pattern
    #[inline]
    #[cfg(feature = "middleware")]
    pub(crate) fn map_layer(&mut self, method: Method, pattern: &str, handler: Layer) {
        self.routes
            .insert(pattern, method, handler);
    }
    
    #[inline]
    pub(crate) fn contains(&mut self, method: &Method, pattern: &str) -> bool {
        self.routes.find(pattern)
            .map(|params| params.route.handlers
                .as_ref()
                .is_some_and(|h| h
                    .binary_search_by(|r| r.cmp(method))
                    .is_ok()))
            .unwrap_or(false)
    }

    /// Traverses the route tree and collects all available routes.
    /// Returns a vector of tuples containing (HTTP method, route path)
    #[cfg(debug_assertions)]
    pub(crate) fn collect(&self) -> meta::RoutesInfo {
        self.routes.collect()
    }
    
    #[cfg(feature = "middleware")]
    pub(crate) fn compose(&mut self) {
        self.routes.compose();
    }
}

#[cfg(test)]
mod tests {
    use super::{Endpoints, RouteOption, handlers::Func};
    use crate::Results;
    use hyper::{Method, Request};

    #[test]
    fn it_maps_and_gets_endpoint() {
        let mut endpoints = Endpoints::new();
        
        let handler = Func::new(|| async { Results::ok() });
        
        endpoints.map_route(Method::POST, "path/to/handler", handler);
        
        let request = Request::post("https://example.com/path/to/handler").body(()).unwrap();
        let post_handler = endpoints.get_endpoint(request.method(), request.uri());

        match post_handler {
            RouteOption::Ok(_) => (),
            _ => panic!("`post_handler` must be is the `Ok` state")
        }
    }

    #[test]
    fn it_returns_route_not_found() {
        let mut endpoints = Endpoints::new();

        let handler = Func::new(|| async { Results::ok() });

        endpoints.map_route(Method::POST, "path/to/handler", handler);

        let request = Request::post("https://example.com/path/to/another-handler").body(()).unwrap();
        let post_handler = endpoints.get_endpoint(request.method(), request.uri());

        match post_handler {
            RouteOption::RouteNotFound => (),
            _ => panic!("`post_handler` must be is the `RouteNotFound` state")
        } 
    }

    #[test]
    fn it_returns_method_not_found() {
        let mut endpoints = Endpoints::new();

        let handler = Func::new(|| async { Results::ok() });

        endpoints.map_route(Method::GET, "path/to/handler", handler);

        let request = Request::post("https://example.com/path/to/handler").body(()).unwrap();
        let post_handler = endpoints.get_endpoint(request.method(), request.uri());

        match post_handler {
            RouteOption::MethodNotFound(allow) => assert_eq!(allow, "GET"),
            _ => panic!("`post_handler` must be is the `MethodNotFound` state")
        }
    }
    
    #[test]
    fn is_has_route_after_map() {
        let mut endpoints = Endpoints::new();

        let handler = Func::new(|| async { Results::ok() });

        endpoints.map_route(Method::GET, "path/to/handler", handler);

        let has_route = endpoints.contains(&Method::GET, "path/to/handler");
        
        assert!(has_route);
    }

    #[test]
    fn is_doesnt_have_route_after_map_a_different_one() {
        let mut endpoints = Endpoints::new();

        let handler = Func::new(|| async { Results::ok() });

        endpoints.map_route(Method::GET, "path/to/handler", handler);

        let has_route = endpoints.contains(&Method::PUT, "path/to/handler");

        assert!(!has_route);
    }
}