//! Endpoints mapping utilities

use hyper::{Method, Uri};
use super::endpoints::{
    route::{RouteNode, RoutePipeline, PathArgs, make_allowed_str},
    handlers::RouteHandler
};

#[cfg(feature = "middleware")]
use {
    crate::headers::{HeaderMap, ORIGIN, ACCESS_CONTROL_REQUEST_METHOD},
    crate::http::cors::CorsOverride,
    super::endpoints::route::Layer,
};

#[cfg(debug_assertions)]
pub(crate) mod meta;
pub(crate) mod handlers;
pub(crate) mod route;
pub mod args;

/// Describes a mapping between HTTP Verbs, routes and request handlers
pub(crate) struct Endpoints {
    routes: RouteNode
}

/// Specifies statuses that could be returned after route matching
pub(crate) enum FindResult {
    RouteNotFound,
    MethodNotFound(String),
    Ok(Endpoint)
}

/// Describes the endpoint that could be either a request handler 
/// or a middleware pipeline with a request handler at the end.
pub(crate) struct Endpoint {
    /// Request handler or middleware pipeline
    pub(crate) pipeline: RoutePipeline,
    
    /// Current request path parameters with their values
    pub(crate) params: PathArgs,

    #[cfg(feature = "middleware")]
    pub(super) cors: CorsOverride
}

impl Endpoint {
    /// Creates a new endpoint with the given request handler and path parameters
    #[inline]
    fn new(
        pipeline: RoutePipeline,
        params: PathArgs,
        #[cfg(feature = "middleware")] cors: CorsOverride
    ) -> Self {
        Self { pipeline, params, #[cfg(feature = "middleware")] cors }
    }

    /// Converts the endpoint into a tuple of (request handler, path parameters)
    #[inline]
    #[cfg(feature = "middleware")]
    pub(crate) fn into_parts(self) -> (RoutePipeline, PathArgs, CorsOverride) {
        (self.pipeline, self.params, self.cors)
    }

    #[inline]
    #[cfg(not(feature = "middleware"))]
    pub(crate) fn into_parts(self) -> (RoutePipeline, PathArgs) {
        (self.pipeline, self.params)
    }
}

impl Endpoints {
    /// Creates a new endpoints collection
    #[inline]
    pub(crate) fn new() -> Self {
        Self { routes: RouteNode::new() }
    }

    /// Gets a context of the executing route by its `HttpRequest`
    #[inline]
    pub(crate) fn find(
        &self,
        method: &Method,
        uri: &Uri,
        #[cfg(feature = "middleware")] cors_enabled: bool,
        #[cfg(feature = "middleware")] headers: &HeaderMap
    ) -> FindResult {
        let route_params = match self.routes.find(uri.path()) {
            Some(params) => params,
            None => return FindResult::RouteNotFound,
        };

        let Some(handlers) = &route_params.route.handlers else { 
            return FindResult::RouteNotFound;
        };

        #[cfg(feature = "middleware")]
        if cors_enabled && method == Method::OPTIONS {
            // OPTIONS path: treat CORS preflight specially
            // (Only if it looks like a preflight request)
            let origin_present = headers.contains_key(ORIGIN);
            let acrm = headers
                .get(ACCESS_CONTROL_REQUEST_METHOD)
                .and_then(|v| Method::from_bytes(v.as_bytes()).ok());

            if origin_present && let Some(target_method) = acrm {
                // Check if the target method exists for this path
                return handlers
                    .binary_search_by(|h| h.cmp(&target_method))
                    .map_or_else(
                        |_| FindResult::MethodNotFound(make_allowed_str(handlers)),
                        |i| {
                            let handler = &handlers[i];
                            FindResult::Ok(
                                Endpoint::new(
                                    handler.pipeline.clone(),
                                    route_params.params,
                                    #[cfg(feature = "middleware")] handler.cors.clone())
                            )
                        },
                    );
            }
        }

        // Normal OPTIONS: keep existing behavior (likely 405 unless the user actually mapped OPTIONS)
        handlers
            .binary_search_by(|h| h.cmp(method))
            .map_or_else(
                |_| FindResult::MethodNotFound(make_allowed_str(handlers)),
                |i| {
                    let handler = &handlers[i];
                    FindResult::Ok(
                        Endpoint::new(
                            handler.pipeline.clone(),
                            route_params.params,
                            #[cfg(feature = "middleware")] handler.cors.clone())
                    )
                },
            )
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

    /// Binds CORS headers to the route handler
    #[inline]
    #[cfg(feature = "middleware")]
    pub(crate) fn bind_cors(
        &mut self,
        method: &Method,
        pattern: &str,
        cors: CorsOverride
    ) {
        self
            .routes
            .find_mut(pattern)
            .map(|route| route
                .handler_mut(method)
                .map(|h| h.cors = cors)
            );
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
    use crate::ok;
    use super::{Endpoints, FindResult, handlers::Func};
    use hyper::{Method, Request};
    #[cfg(feature = "middleware")]
    use crate::headers::HeaderMap;

    #[test]
    fn it_maps_and_gets_endpoint() {
        let mut endpoints = Endpoints::new();
        
        let handler = Func::new(|| async { ok!() });
        
        endpoints.map_route(Method::POST, "path/to/handler", handler);
        
        let request = Request::post("https://example.com/path/to/handler").body(()).unwrap();
        let post_handler = endpoints.find(
            request.method(),
            request.uri(),
            #[cfg(feature = "middleware")] false,
            #[cfg(feature = "middleware")] &HeaderMap::new()
        );

        match post_handler {
            FindResult::Ok(_) => (),
            _ => panic!("`post_handler` must be is the `Ok` state")
        }
    }

    #[test]
    fn it_returns_route_not_found() {
        let mut endpoints = Endpoints::new();

        let handler = Func::new(|| async { ok!() });

        endpoints.map_route(Method::POST, "path/to/handler", handler);

        let request = Request::post("https://example.com/path/to/another-handler").body(()).unwrap();
        let post_handler = endpoints.find(
            request.method(),
            request.uri(),
            #[cfg(feature = "middleware")] false,
            #[cfg(feature = "middleware")] &HeaderMap::new()
        );

        match post_handler {
            FindResult::RouteNotFound => (),
            _ => panic!("`post_handler` must be is the `RouteNotFound` state")
        } 
    }

    #[test]
    fn it_returns_method_not_found() {
        let mut endpoints = Endpoints::new();

        let handler = Func::new(|| async { ok!() });

        endpoints.map_route(Method::GET, "path/to/handler", handler);

        let request = Request::post("https://example.com/path/to/handler").body(()).unwrap();
        let post_handler = endpoints.find(
            request.method(),
            request.uri(),
            #[cfg(feature = "middleware")] false,
            #[cfg(feature = "middleware")] &HeaderMap::new()
        );

        match post_handler {
            FindResult::MethodNotFound(allow) => assert_eq!(allow, "GET"),
            _ => panic!("`post_handler` must be is the `MethodNotFound` state")
        }
    }
    
    #[test]
    fn is_has_route_after_map() {
        let mut endpoints = Endpoints::new();

        let handler = Func::new(|| async { ok!() });

        endpoints.map_route(Method::GET, "path/to/handler", handler);

        let has_route = endpoints.contains(&Method::GET, "path/to/handler");
        
        assert!(has_route);
    }

    #[test]
    fn is_doesnt_have_route_after_map_a_different_one() {
        let mut endpoints = Endpoints::new();

        let handler = Func::new(|| async { ok!() });

        endpoints.map_route(Method::GET, "path/to/handler", handler);

        let has_route = endpoints.contains(&Method::PUT, "path/to/handler");

        assert!(!has_route);
    }
}