//! Endpoints mapping utilities

use std::{borrow::Cow, collections::HashMap};
use hyper::{Method, Uri};
use super::endpoints::{
    route::{RouteNode, RoutePipeline, PathArguments},
    handlers::RouteHandler
};

#[cfg(feature = "middleware")]
use super::endpoints::route::Layer;

pub(crate) mod handlers;
pub(crate) mod route;
pub mod args;

const ALLOW_METHOD_SEPARATOR : &str = ",";
const PATH_SEPARATOR : char = '/';

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
    pub(crate) params: PathArguments
}

impl EndpointContext {
    pub(crate) fn into_parts(self) -> (RoutePipeline, PathArguments) {
        (self.pipeline, self.params)
    }

    fn new(pipeline: RoutePipeline, params: PathArguments) -> Self {
        Self { pipeline, params }
    }
}

impl Endpoints {
    pub(crate) fn new() -> Self {
        Self { routes: RouteNode::Static(HashMap::new()) }
    }

    /// Gets a context of the executing route by its `HttpRequest`
    #[inline]
    pub(crate) fn get_endpoint(&self, method: &Method, uri: &Uri) -> RouteOption {
        let path_segments = Self::split_path(uri.path());
        let route_params = match self.routes.find(&path_segments) {
            Some(params) => params,
            None => return RouteOption::RouteNotFound,
        };

        if let RouteNode::Handler(handlers) = &route_params.route {
            return handlers.get(method).map_or_else(
                || {
                    let allowed_methods = handlers
                        .keys()
                        .map(|key| key.as_str())
                        .collect::<Vec<_>>()
                        .join(ALLOW_METHOD_SEPARATOR);
                    RouteOption::MethodNotFound(allowed_methods)
                },
                |handlers| RouteOption::Ok(
                    EndpointContext::new(handlers.clone(), route_params.params)
                ),
            );
        }

        RouteOption::RouteNotFound
    }

    /// Maps the request handler to the current HTTP Verb and route pattern
    #[inline]
    pub(crate) fn map_route(&mut self, method: Method, pattern: &str, handler: RouteHandler) {
        let path_segments = Self::split_path(pattern);
        self.routes.insert(&path_segments, method, handler.into());
    }

    /// Maps the request layer to the current HTTP Verb and route pattern
    #[inline]
    #[cfg(feature = "middleware")]
    pub(crate) fn map_layer(&mut self, method: Method, pattern: &str, handler: Layer) {
        let path_segments = Self::split_path(pattern);
        self.routes.insert(&path_segments, method, handler);
    }
    
    #[inline]
    pub(crate) fn contains(&mut self, method: &Method, pattern: &str) -> bool {
        let path_segments = Self::split_path(pattern);
        self.routes.find(&path_segments).and_then(|params| match &params.route {
            RouteNode::Handler(handlers) => Some(handlers.contains_key(method)),
            _ => None,
        }).unwrap_or(false)
    }
    
    #[cfg(feature = "middleware")]
    pub(crate) fn compose(&mut self) {
        self.routes.compose();
    }

    #[inline]
    fn split_path(path: &str) -> Vec<Cow<'static, str>> {
        path.trim_matches(PATH_SEPARATOR)
            .split(PATH_SEPARATOR)
            .map(|s| Cow::Owned(s.to_owned()))
            .collect()
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